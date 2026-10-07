use crate::closedness::ClosedCopies;
use crate::expression::ExpressionStore;
use crate::operators::Concrete;
use crate::relaxation::RelaxationLoop;
use crate::scope::ScopeFrame;
use crate::unify::{push_branch, unify};
use crate::value::{
    BottomKind, Bound, Conjunct, DisjunctionBranch as ValueBranch, FieldEntry, Imports,
    StructValue, Thunk, ThunkEnv, Value, ValueArena, ValueId,
};
use cue_syntax::ast::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use thiserror::Error;

/// What a comprehension clause read: a decision, or a value that is not
/// concrete enough to decide one yet.
pub(crate) enum Clause<T> {
    Ready(T),
    Incomplete(String),
}

/// The elements a list comprehension yielded, and the clause it could not
/// decide, if any.
#[derive(Default)]
struct ListYield {
    elements: Vec<ValueId>,
    incomplete: Option<String>,
}

#[derive(Error, Debug)]
pub enum EvalError {
    #[error("Parse error: {0}")]
    Parse(#[from] cue_syntax::parser::ParseError),
    #[error("Evaluation error: {0}")]
    Evaluation(String),
    /// A name that has not resolved *yet*. The declaration loop retries a
    /// literal that raises this, so it must stay distinguishable from an
    /// evaluation error the loop should give up on. Same wording on purpose.
    #[error("Evaluation error: {0}")]
    Unresolved(String),
}

pub struct Evaluator {
    pub arena: ValueArena,
    pub(crate) scopes: Vec<ScopeFrame>,
    pub(crate) expressions: ExpressionStore,
    resolving_symbols: HashSet<String>,
    /// Environment of the struct literal being evaluated, which its fields
    /// capture as the scope their recipes run in.
    pub(crate) current_env: Option<Rc<ThunkEnv>>,
    /// Name of the field currently being evaluated in the innermost struct.
    pub(crate) current_field: Option<String>,
    /// Depth of the current struct's scope in `self.scopes`.
    pub(crate) struct_scope_depth: usize,
    pub placeholders: HashMap<String, ValueId>,
    /// How many field recipes re-derivation has run, and how many structs it
    /// gave up on at the sweep cap. Evaluation never reads them; they are what a
    /// test asserts on to keep the pass proportional to what a merge changed
    /// rather than to the size of the struct it changed it in.
    pub derivations: usize,
    pub unsettled: usize,
    /// How many refinement passes the declaration loop ran: passes that
    /// resolved nothing but left a partial bound to more than it was.
    /// Evaluation never reads it; tests pin the loop's patience with it.
    pub refinements: usize,
    /// The packages the file being evaluated imported. A literal captures this
    /// in its `ThunkEnv`, so a recipe derived again after it crossed an import
    /// boundary resolves the packages *its* file imported rather than the
    /// importer's.
    pub imports: Rc<Imports>,
    expr_depth: usize,
    /// The closed copy each definition read so far resolves to.
    pub(crate) closed: ClosedCopies,
    /// Whether the literal being evaluated is a file's top level, which an
    /// embedded definition does not close.
    pub(crate) at_file_root: bool,
    /// Set while evaluating an embedding at a file's top level. Upstream merges
    /// such an embedding without closedness at any depth, so the definitions it
    /// reads are left open.
    pub(crate) reading_root_embedding: bool,
    /// Set by the package loader: which values to record their directory on.
    pub(crate) origin: Option<crate::package::OriginAnnotation>,
    /// Set while some enclosing relaxation loop can still retry. A reference
    /// that has not resolved yet is then not an answer: an existence check or a
    /// comprehension's condition stays pending instead of deciding on it.
    pub(crate) deferring: bool,
    /// Set while evaluating a definition's body, including nested literals.
    /// A select whose base lacks the field is then decided, not pending: a
    /// template cannot grow the field the way an open value may.
    pub(crate) in_definition: bool,
    /// Comprehension syntax shared by the recipes of every literal that
    /// evaluates it, with the names its clauses read.
    comprehensions: HashMap<Rc<ComprehensionDecl>, Rc<HashSet<String>>>,
    /// Set for the literal a comprehension body is about to evaluate, naming
    /// the literal the comprehension is written in. The comprehension's recipe
    /// reads that body's dynamic labels already.
    pub(crate) comprehension_body: Option<crate::value::Enclosing>,
}

const MAX_EXPR_DEPTH: usize = 64;

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Self::with_arena(ValueArena::new())
    }

    /// An evaluator that allocates into an arena another one already holds.
    ///
    /// An imported package is loaded into the arena of the file that imports it,
    /// so the recipes its fields carry stay valid there and the merge that
    /// overrides one of them re-derives its readers like any other merge.
    pub fn with_arena(arena: ValueArena) -> Self {
        let mut evaluator = Self {
            arena,
            scopes: vec![ScopeFrame::default()],
            expressions: ExpressionStore::default(),
            resolving_symbols: HashSet::new(),
            current_env: None,
            current_field: None,
            struct_scope_depth: 0,
            placeholders: HashMap::new(),
            derivations: 0,
            unsettled: 0,
            refinements: 0,
            imports: Rc::new(Imports::default()),
            expr_depth: 0,
            closed: ClosedCopies::default(),
            at_file_root: false,
            reading_root_embedding: false,
            origin: None,
            deferring: false,
            in_definition: false,
            comprehensions: HashMap::new(),
            comprehension_body: None,
        };
        evaluator.register_builtins();
        evaluator
    }

    fn register_builtins(&mut self) {
        for (name, id) in crate::builtins::type_builtins(&mut self.arena) {
            self.insert_binding(&name, id);
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(ScopeFrame::default());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert_binding(&mut self, name: &str, val: ValueId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, val);
        }
    }

    /// Drop a binding this scope added, exposing whatever an enclosing scope
    /// binds the same name to.
    pub(crate) fn remove_binding(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.remove(name);
        }
    }

    /// Build a bound constraint over an evaluated target. The target must
    /// be a concrete value of a kind the operator accepts; anything else is
    /// decided now. Retryable bottoms stay lazy so forward references keep
    /// working; settled failures propagate as the bound's own failure; bool
    /// is not an ordered type; and a bound, basic type, or top is never a
    /// concrete value to bound (upstream: `incomplete`).
    fn eval_bound(&mut self, bound: Bound, target_id: ValueId) -> Result<ValueId, EvalError> {
        let lazy = || Value::Bounds {
            base_type: None,
            constraints: vec![(bound, target_id)],
        };
        let target_ok = match (bound, self.arena.get(target_id)) {
            (
                Bound::Less | Bound::LessEqual | Bound::Greater | Bound::GreaterEqual,
                Some(Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Bytes(_)),
            )
            | (
                Bound::Equal | Bound::NotEqual,
                Some(
                    Value::Int(_)
                    | Value::Float(_)
                    | Value::String(_)
                    | Value::Bool(_)
                    | Value::List { .. }
                    | Value::Struct(_),
                ),
            )
            | (Bound::RegexMatch | Bound::RegexNotMatch, Some(Value::String(_))) => true,
            // `!=null` (and `==null`) filter nulls downstream, so they
            // stay lazy. Null is not orderable.
            (
                Bound::Equal | Bound::NotEqual | Bound::RegexMatch | Bound::RegexNotMatch,
                Some(Value::Null),
            ) => true,
            // Any value that may still resolve stays lazy: the declaration
            // is re-derived once it does.
            (_, Some(Value::Bottom(reason))) if reason.kind.may_resolve_later() => true,
            _ => false,
        };
        if target_ok {
            return Ok(self.arena.alloc(lazy()));
        }
        match self.arena.get(target_id) {
            // A settled failure is the bound's own failure.
            Some(Value::Bottom(_)) => Ok(target_id),
            // Bool is not an ordered type.
            Some(Value::Bool(_)) => Ok(self.arena.bottom_of(
                BottomKind::Conflict,
                format!("cannot use bool for bound {bound}"),
            )),
            // Null is not orderable either.
            Some(Value::Null) => Ok(self.arena.bottom_of(
                BottomKind::Conflict,
                format!("cannot use null for bound {bound}"),
            )),
            _ => {
                let description = Self::describe_bound_target(&self.arena, target_id);
                Ok(self.arena.bottom_of(
                    BottomKind::Incomplete,
                    format!("non-concrete value {description} for bound {bound}"),
                ))
            }
        }
    }

    /// How a bound target reads inside a "non-concrete value" diagnosis:
    /// `<3`, `!=3`, `int`. Message cosmetics only — the oracle compares codes.
    fn describe_bound_target(arena: &ValueArena, target_id: ValueId) -> String {
        let scalar = |id: ValueId| -> Option<String> {
            match arena.get(id) {
                Some(Value::Int(i)) => Some(i.to_string()),
                Some(Value::Float(f)) => Some(format!("{f:?}")),
                Some(Value::String(s)) => Some(format!("{s:?}")),
                Some(Value::Bool(b)) => Some(b.to_string()),
                Some(Value::Bytes(_)) => Some("bytes".to_string()),
                _ => None,
            }
        };
        match arena.get(target_id) {
            Some(Value::Bounds { constraints, .. }) => match constraints.as_slice() {
                [(op, inner)] => match scalar(*inner) {
                    Some(text) => format!("{op}{text}"),
                    None => format!("{op}…"),
                },
                _ => "bound".to_string(),
            },
            Some(Value::Type(kind)) => kind.to_string(),
            Some(Value::Top) => "top".to_string(),
            _ => scalar(target_id).unwrap_or_else(|| "value".to_string()),
        }
    }

    pub fn lookup_binding(&self, name: &str) -> Option<ValueId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    /// Evaluate an entire CUE source file.
    pub fn eval_file(&mut self, file: &SourceFile) -> Result<ValueId, EvalError> {
        crate::references::check_file(file).map_err(EvalError::Evaluation)?;
        for imp in &file.imports {
            let pkg_name = imp
                .alias
                .clone()
                .unwrap_or_else(|| imp.path.default_alias())
                .into_inner();
            Rc::make_mut(&mut self.imports)
                .aliases
                .insert(pkg_name, imp.path.as_str().to_string());
        }

        self.eval_root_decls(&file.decls)
    }

    /// Evaluate the top level of a file or package. It is never closed: a
    /// definition embedded there constrains nothing beside it.
    ///
    /// The driver lives on [`RelaxationLoop`](crate::relaxation::RelaxationLoop).
    pub fn eval_root_decls(&mut self, decls: &[Decl]) -> Result<ValueId, EvalError> {
        RelaxationLoop::new(self).eval_root_decls(decls)
    }

    /// Unify two values in this evaluator's arena and re-derive any reader
    /// fields affected by the merge. Re-derivation lives on
    /// [`RelaxationLoop`](crate::relaxation::RelaxationLoop).
    pub fn unify_and_rederive(&mut self, v1: ValueId, v2: ValueId) -> Result<ValueId, EvalError> {
        RelaxationLoop::new(self).unify_and_rederive(v1, v2)
    }

    /// What reading `name` yields: a definition's closed copy, anything else as
    /// it is.
    fn read_definition(&mut self, name: &str, val: ValueId) -> ValueId {
        crate::closedness::read_definition(
            &mut self.arena,
            &mut self.closed,
            self.reading_root_embedding,
            name,
            val,
        )
    }

    /// Rebind the names this literal declares to the struct's merged values, so
    /// the declarations that follow read what embeddings and comprehensions
    /// added to them.
    ///
    /// A field only an embedding contributed, or only a comprehension
    /// generated, was not declared here: a reference to that name means the
    /// enclosing scope's, as upstream has it, so it is not bound.
    pub(crate) fn bind_struct_fields(&mut self, value: &StructValue) {
        for (name, entry) in value
            .fields
            .iter()
            .chain(&value.definitions)
            .chain(&value.hidden)
        {
            if self.declares_field(name) {
                self.insert_binding(name, entry.val);
            }
        }
    }

    /// Whether the literal being evaluated declares this name as a field.
    ///
    /// Enclosing declarations that shadow existing names have pending bindings
    /// in their own frames, so a reader waits until they are evaluated.
    pub(crate) fn declares_field(&self, name: &str) -> bool {
        self.current_env
            .as_ref()
            .is_some_and(|env| env.owns_field(name))
    }

    pub(crate) fn unify_decl_field(
        &mut self,
        fields: &mut crate::value::FieldMap,
        name: &str,
        val: ValueId,
        optional: bool,
        conjunct: Conjunct,
    ) -> Result<ValueId, EvalError> {
        let entry = fields
            .entry(name.to_string())
            .and_modify(|entry| {
                entry.val = unify(&mut self.arena, entry.val, val);
                entry.optional &= optional;
                entry.extend_conjuncts([conjunct.clone()]);
            })
            .or_insert_with(|| FieldEntry::with_conjuncts(val, optional, vec![conjunct]));
        Ok(entry.val)
    }

    /// Evaluate a field expression and keep the recipe that produced it: the
    /// expression and the scope its struct literal was written in. A merge that
    /// changes what the expression read runs it again from there.
    pub(crate) fn eval_field_value(
        &mut self,
        expr: &Expr,
        env: &Rc<ThunkEnv>,
    ) -> Result<(ValueId, Conjunct), EvalError> {
        let val = self.eval_expr(expr)?;
        Ok((val, self.field_conjunct(expr, env, val)))
    }

    pub(crate) fn field_conjunct(
        &mut self,
        expr: &Expr,
        env: &Rc<ThunkEnv>,
        val: ValueId,
    ) -> Conjunct {
        let Some(source) = self.expressions.prepare_recipe(expr, &env.reachable_lets()) else {
            // No binding can change this expression's value after a merge.
            return Conjunct::Value(val);
        };
        Conjunct::Thunk(Thunk {
            expr: source.expr,
            env: env.clone(),
            deps: source.deps,
            closes: false,
        })
    }

    /// Decide a comprehension's `if` guard. The guard reads its condition's
    /// default, as every position that needs a concrete value does; one that
    /// is not concrete yet stays undecided rather than counting as `false`.
    pub(crate) fn comprehension_guard(
        &mut self,
        condition: &Expr,
    ) -> Result<Clause<bool>, EvalError> {
        let id = self.eval_expr(condition)?;
        match crate::operators::concrete(&mut self.arena, id) {
            Concrete::Value(id) => match self.arena.get(id) {
                Some(Value::Bool(b)) => Ok(Clause::Ready(*b)),
                Some(other) => {
                    let kind = crate::operators::kind_name(other);
                    Err(EvalError::Evaluation(format!(
                        "cannot use {} (type {kind}) as type bool",
                        self.describe(id)
                    )))
                }
                None => Err(EvalError::Evaluation("invalid condition".to_string())),
            },
            Concrete::Pending(message) => Err(EvalError::Unresolved(message)),
            Concrete::Incomplete(message) => Ok(Clause::Incomplete(message)),
            Concrete::Error(message) => Err(EvalError::Evaluation(message)),
        }
    }

    /// Evaluate a comprehension's `for` source: a list or a struct, with its
    /// default selected.
    pub(crate) fn comprehension_source(
        &mut self,
        source: &Expr,
    ) -> Result<Clause<Value>, EvalError> {
        let id = self.eval_expr(source)?;
        match crate::operators::concrete(&mut self.arena, id) {
            Concrete::Value(id) => match self.arena.get(id) {
                Some(value @ (Value::List { .. } | Value::Struct(_))) => {
                    Ok(Clause::Ready(value.clone()))
                }
                Some(other) => {
                    let kind = crate::operators::kind_name(other);
                    Err(EvalError::Evaluation(format!(
                        "cannot range over {} (found {kind}, want list or struct)",
                        self.describe(id)
                    )))
                }
                None => Err(EvalError::Evaluation("invalid source".to_string())),
            },
            // A source that is not resolved yet has nothing to yield *yet*;
            // yielding nothing would settle the comprehension as empty.
            Concrete::Pending(message) => Err(EvalError::Unresolved(message)),
            Concrete::Incomplete(message) => Ok(Clause::Incomplete(message)),
            Concrete::Error(message) => Err(EvalError::Evaluation(message)),
        }
    }

    /// Evaluate a dynamic field's label: a string, with its default selected.
    pub(crate) fn dynamic_label(&mut self, label: &Expr) -> Result<Clause<String>, EvalError> {
        let id = self.eval_expr(label)?;
        match crate::operators::concrete(&mut self.arena, id) {
            Concrete::Value(id) => match self.arena.get(id) {
                Some(Value::String(name)) => Ok(Clause::Ready(name.clone())),
                // A concrete integer label is definitely wrong, not merely
                // unresolved: upstream reports `eval`.
                Some(Value::Int(_)) => Err(EvalError::Evaluation(
                    "integer fields not supported".to_string(),
                )),
                Some(other) => {
                    let kind = crate::operators::kind_name(other);
                    Err(EvalError::Evaluation(format!(
                        "invalid index {} (invalid type {kind})",
                        self.describe(id)
                    )))
                }
                None => Err(EvalError::Evaluation("invalid label".to_string())),
            },
            Concrete::Pending(message) => Err(EvalError::Unresolved(message)),
            Concrete::Incomplete(message) => Ok(Clause::Incomplete(format!(
                "key value of dynamic field must be concrete: {message}"
            ))),
            Concrete::Error(message) => Err(EvalError::Evaluation(message)),
        }
    }

    /// Shared syntax for a comprehension, and the names its clauses read.
    pub(crate) fn intern_comprehension(
        &mut self,
        comp: &ComprehensionDecl,
    ) -> (Rc<ComprehensionDecl>, Rc<HashSet<String>>) {
        if let Some((comp, deps)) = self.comprehensions.get_key_value(comp) {
            return (comp.clone(), deps.clone());
        }
        let deps = Rc::new(crate::deps::clause_deps(comp));
        let comp = Rc::new(comp.clone());
        self.comprehensions.insert(comp.clone(), deps.clone());
        (comp, deps)
    }

    /// `merged`, the value of the conjunction `expr`, with the fields its
    /// struct literals declare first: upstream inserts those before it
    /// resolves the references beside them (see [`crate::arc_order`]).
    fn literal_fields_first(&mut self, expr: &Expr, merged: ValueId) -> ValueId {
        let mut labels = Vec::new();
        crate::arc_order::expr_literal_labels(expr, &mut labels);
        crate::arc_order::with_literal_fields_first(&mut self.arena, merged, &labels)
    }

    /// A concrete value as a diagnostic names it.
    fn describe(&self, id: ValueId) -> String {
        crate::export::to_json(&self.arena, id)
            .map(|json| json.to_string())
            .unwrap_or_else(|_| "value".to_string())
    }

    /// The elements a list comprehension yields, or the bottom that stands for
    /// the enclosing list when a clause fails or cannot be decided yet.
    fn eval_list_comprehension(
        &mut self,
        comp: &cue_syntax::ast::ListComprehension,
    ) -> Result<Result<Vec<ValueId>, ValueId>, EvalError> {
        let mut out = ListYield::default();
        match self.eval_list_comprehension_clause(0, comp, &mut out) {
            Err(EvalError::Unresolved(m)) => {
                return Ok(Err(self.arena.bottom_of(BottomKind::Unresolved, m)));
            }
            // A definite error in a clause is the list's value, the way
            // an error in an element would be.
            Err(EvalError::Evaluation(m)) => {
                return Ok(Err(self.arena.bottom_of(BottomKind::Conflict, m)));
            }
            other => other?,
        }
        // An undecided clause leaves the whole list undecided: the field
        // holding it derives it again once a merge decides the clause.
        if let Some(message) = out.incomplete {
            return Ok(Err(self.arena.bottom_of(BottomKind::Incomplete, message)));
        }
        Ok(Ok(out.elements))
    }

    fn eval_list_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &cue_syntax::ast::ListComprehension,
        out: &mut ListYield,
    ) -> Result<(), EvalError> {
        if out.incomplete.is_some() {
            return Ok(());
        }
        if clause_idx >= comp.clauses.len() {
            let val_id = self.eval_expr(&comp.expr)?;
            out.elements.push(val_id);
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => match self.comprehension_guard(condition)? {
                Clause::Ready(true) => {
                    self.eval_list_comprehension_clause(clause_idx + 1, comp, out)?;
                }
                Clause::Ready(false) => {}
                Clause::Incomplete(message) => out.incomplete = Some(message),
            },
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                self.push_scope();
                self.insert_binding(ident, val_id);
                let result = self.eval_list_comprehension_clause(clause_idx + 1, comp, out);
                self.pop_scope();
                result?;
            }
            ComprehensionClause::For { key, value, source } => {
                let src_val = match self.comprehension_source(source)? {
                    Clause::Ready(value) => value,
                    Clause::Incomplete(message) => {
                        out.incomplete = Some(message);
                        return Ok(());
                    }
                };

                match src_val {
                    Value::List {
                        elements: src_elems,
                        ..
                    } => {
                        for (idx, &elem_id) in src_elems.iter().enumerate() {
                            self.push_scope();
                            self.insert_binding(value, elem_id);
                            if let Some(k_name) = key {
                                let k_id = self.arena.int(idx as i64);
                                self.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_list_comprehension_clause(clause_idx + 1, comp, out);
                            self.pop_scope();
                            result?;
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in s.fields.iter().filter(|(_, e)| !e.optional) {
                            self.push_scope();
                            self.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.arena.string(k.as_str());
                                self.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_list_comprehension_clause(clause_idx + 1, comp, out);
                            self.pop_scope();
                            result?;
                        }
                    }
                    _ => unreachable!("comprehension_source yields a list or a struct"),
                }
            }
        }
        Ok(())
    }

    /// Evaluate an AST Expression.
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<ValueId, EvalError> {
        if self.expr_depth >= MAX_EXPR_DEPTH {
            return Err(EvalError::Evaluation(
                "recursion depth limit exceeded during expression evaluation".to_string(),
            ));
        }
        self.expr_depth += 1;
        let res = self.eval_expr_inner(expr);
        self.expr_depth = self.expr_depth.saturating_sub(1);
        res
    }

    /// A bare reference to the field being defined contributes no new
    /// constraint in a conjunction: x: x & 1 is simply x: 1. Restrict this to
    /// the reference itself; a cycle inside arithmetic or interpolation is
    /// not an identity and must still fail.
    fn is_unbound_self_reference(&self, expr: &Expr, val: ValueId) -> bool {
        matches!(expr, Expr::Ident(name) | Expr::HiddenIdent(name)
            if self.current_field.as_deref() == Some(name))
            && matches!(self.arena.get(val), Some(Value::Bottom(reason))
                if reason.kind == BottomKind::Cycle)
    }

    fn eval_expr_inner(&mut self, expr: &Expr) -> Result<ValueId, EvalError> {
        match expr {
            Expr::Bottom => Ok(self.arena.bottom("explicit bottom")),
            Expr::Top => Ok(self.arena.top()),
            Expr::Null => Ok(self.arena.null()),
            Expr::Bool(b) => Ok(self.arena.bool(*b)),
            Expr::Number(n) => Ok(self.eval_number(n)),
            Expr::String(s) => Ok(self.arena.string(s.value.as_str())),
            Expr::Bytes(b) => Ok(self.arena.alloc(Value::Bytes(b.value.clone()))),
            Expr::Ident(id)
            | Expr::DefIdent(id)
            | Expr::HiddenIdent(id)
            | Expr::HiddenDefIdent(id) => {
                if self.resolving_symbols.contains(id) {
                    if id.starts_with('#') || id.starts_with("_#") {
                        return Ok(self.arena.alloc(Value::RecursiveRef {
                            name: id.clone(),
                            target: self.lookup_binding(id),
                        }));
                    } else {
                        return Ok(self.arena.bottom(format!(
                            "cycle error: cyclic value dependency detected on '{id}'"
                        )));
                    }
                }

                // Loop bindings inside the literal take precedence over its fields.
                let local_scope_count = self.scopes.len().saturating_sub(self.struct_scope_depth);
                let local_val = self
                    .scopes
                    .iter()
                    .rev()
                    .take(local_scope_count)
                    .find_map(|scope| scope.get(id).copied());

                if let Some(val) = local_val {
                    if self.current_field.as_deref() == Some(id)
                        && self.declares_field(id)
                        && matches!(self.arena.get(val), Some(Value::Bottom(reason))
                            if reason.kind == BottomKind::Unresolved)
                    {
                        return Ok(self.arena.bottom_of(
                            BottomKind::Cycle,
                            format!("cycle with field: {id}: incomplete value"),
                        ));
                    }
                    return Ok(self.read_definition(id, val));
                }

                // An uncomputed field shadows bindings outside its literal.
                if self.declares_field(id) {
                    if self.current_field.as_deref() == Some(id) {
                        if id.starts_with('#') || id.starts_with("_#") {
                            return Ok(self.arena.alloc(Value::RecursiveRef {
                                name: id.clone(),
                                target: self.lookup_binding(id),
                            }));
                        } else {
                            return Ok(self.arena.bottom_of(
                                BottomKind::Cycle,
                                format!("cycle with field: {id}: incomplete value"),
                            ));
                        }
                    } else {
                        // Forward reference to a sibling field in the same struct
                        return Ok(self
                            .arena
                            .bottom_of(BottomKind::Unresolved, "incomplete value"));
                    }
                }

                // The literal does not declare this name: read its enclosing scope.
                let outer_val = self
                    .scopes
                    .iter()
                    .rev()
                    .skip(local_scope_count)
                    .find_map(|scope| scope.get(id).copied());

                if let Some(val) = outer_val {
                    Ok(self.read_definition(id, val))
                } else {
                    Ok(self.arena.bottom_of(
                        BottomKind::ReferenceNotFound,
                        format!("reference \"{id}\" not found"),
                    ))
                }
            }
            Expr::Struct(s) => {
                let nested = RelaxationLoop::new(self).eval_nested_decls(&s.decls);
                match nested {
                    Ok(value) => RelaxationLoop::new(self).finish_decls(value),
                    // Carry an incomplete nested value to the enclosing retry loop.
                    Err(EvalError::Unresolved(message)) => {
                        Ok(self.arena.bottom_of(BottomKind::Unresolved, message))
                    }
                    // A definite nested error bottoms its own struct instead
                    // of aborting the enclosing file: siblings still evaluate.
                    Err(EvalError::Evaluation(message)) => {
                        Ok(self.arena.bottom_of(BottomKind::Conflict, message))
                    }
                    Err(error) => Err(error),
                }
            }
            Expr::List(l) => {
                let mut elements = Vec::new();
                for elem in &l.elements {
                    // A comprehension splices its yields into this list; a
                    // failed or undecided one is the whole list's value.
                    if let Expr::ListComp(comp) = elem {
                        match self.eval_list_comprehension(comp)? {
                            Ok(yields) => elements.extend(yields),
                            Err(bottom) => return Ok(bottom),
                        }
                    } else {
                        elements.push(self.eval_expr(elem)?);
                    }
                }
                // `[...]` names no element type but is still open, and the value model
                // says "open to this" with `Some`. Reading the missing type as a closed
                // list made `[...] & [1, 2]` a length conflict.
                let ellipsis = match (&l.ellipsis, l.open) {
                    (Some(el), _) => Some(self.eval_expr(el)?),
                    (None, true) => Some(self.arena.top()),
                    (None, false) => None,
                };
                Ok(self.arena.alloc(Value::List { elements, ellipsis }))
            }
            Expr::Binary { op, left, right }
                if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
                    && (matches!(**left, Expr::Bottom) || matches!(**right, Expr::Bottom)) =>
            {
                let operand = if matches!(**left, Expr::Bottom) {
                    right
                } else {
                    left
                };
                self.eval_bottom_check(*op, operand)
            }
            Expr::Binary { op, left, right } => {
                let left_id = self.eval_expr(left)?;
                let right_id = self.eval_expr(right)?;

                match op {
                    BinaryOp::Unify => {
                        if self.is_unbound_self_reference(left, left_id) {
                            return Ok(right_id);
                        }
                        if self.is_unbound_self_reference(right, right_id) {
                            return Ok(left_id);
                        }
                        let merged = unify(&mut self.arena, left_id, right_id);
                        let merged = RelaxationLoop::new(self).rederive(merged)?;
                        Ok(self.literal_fields_first(expr, merged))
                    }
                    BinaryOp::Disjoin => {
                        let branches = vec![
                            ValueBranch {
                                default: false,
                                val: left_id,
                            },
                            ValueBranch {
                                default: false,
                                val: right_id,
                            },
                        ];
                        Ok(self.arena.alloc(Value::Disjunction { branches }))
                    }
                    _ => Ok(crate::operators::binary(
                        &mut self.arena,
                        *op,
                        left_id,
                        right_id,
                    )),
                }
            }
            Expr::Unary { op, expr } => {
                let target_id = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg | UnaryOp::Pos | UnaryOp::Not => {
                        Ok(crate::operators::unary(&mut self.arena, *op, target_id))
                    }
                    // Bounds convert from their syntax position; the default
                    // marker (`*v`) is not a bound and passes through.
                    op => match Bound::try_from(*op) {
                        Ok(bound) => self.eval_bound(bound, target_id),
                        Err(_) => Ok(target_id),
                    },
                }
            }
            Expr::Disjunction { branches } => {
                let mut eval_branches = Vec::new();
                for b in branches {
                    let val_id = self.eval_expr(&b.expr)?;
                    push_branch(
                        &self.arena,
                        &mut eval_branches,
                        ValueBranch {
                            default: b.default,
                            val: val_id,
                        },
                    );
                }
                // A custom error branch is definite: settle it the way the
                // rederive loop would, so the disjunction neither waits on
                // it nor keeps it beside a surviving value. Disjunctions
                // without one allocate untouched.
                let has_custom = eval_branches.iter().any(|branch| {
                    matches!(self.arena.get(branch.val), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Custom)
                });
                if !has_custom {
                    return Ok(self.arena.alloc(Value::Disjunction {
                        branches: eval_branches,
                    }));
                }
                let mut kept = Vec::new();
                let mut errors = Vec::new();
                for branch in eval_branches {
                    match self.arena.get(branch.val) {
                        Some(Value::Bottom(reason))
                            if !reason.kind.may_resolve_later()
                                && reason.kind != BottomKind::Incomplete =>
                        {
                            errors.push(reason.clone());
                        }
                        _ => kept.push(branch),
                    }
                }
                // Every kept branch already failed and a custom branch names
                // the failure: fold the pending failures to one survivor so
                // the disjunction reports instead of waiting forever. The
                // survivor stays retryable, so a merge that supplies a
                // succeeding branch still heals this on the next pass; only
                // its code is decided now, most-decided failure first.
                if !kept.is_empty()
                    && kept
                        .iter()
                        .all(|branch| matches!(self.arena.get(branch.val), Some(Value::Bottom(_))))
                {
                    kept.sort_by_key(|branch| match self.arena.get(branch.val) {
                        Some(Value::Bottom(reason)) => match reason.kind {
                            BottomKind::UndefinedFieldDefinite => 0,
                            BottomKind::Incomplete => 1,
                            BottomKind::UndefinedField => 2,
                            _ => 3,
                        },
                        _ => 4,
                    });
                    kept.truncate(1);
                }
                Ok(crate::unify::settle_disjunction(
                    &mut self.arena,
                    kept,
                    &errors,
                ))
            }
            Expr::Selector { expr, field } => {
                if let Expr::Ident(pkg_name) = expr.as_ref() {
                    let imports = self.imports.clone();
                    if let Some(package) = imports.package_of(pkg_name)
                        && let Some(s) = self.arena.fields(package)
                    {
                        // A loaded package is complete: a name it lacks is a
                        // definite error, not a field a later pass may supply.
                        let Some(f) = s.fields.get(field).or_else(|| s.definitions.get(field))
                        else {
                            return Ok(self.arena.bottom_of(
                                BottomKind::Conflict,
                                format!("undefined field: {field}"),
                            ));
                        };
                        let val = f.val;
                        return Ok(self.read_definition(field, val));
                    }
                    if let Ok(res) = crate::stdlib::call_stdlib_func(
                        &mut self.arena,
                        imports.path_of(pkg_name),
                        field,
                        &[],
                    ) {
                        return Ok(res);
                    }
                }
                let val_id = self.eval_expr(expr)?;
                Ok(crate::operators::select(
                    &mut self.arena,
                    &mut self.closed,
                    self.reading_root_embedding,
                    self.in_definition,
                    val_id,
                    field,
                ))
            }
            Expr::Index { expr, index } => {
                let target_id = self.eval_expr(expr)?;
                let index_id = self.eval_expr(index)?;
                Ok(crate::operators::index(
                    &mut self.arena,
                    self.in_definition,
                    target_id,
                    index_id,
                ))
            }
            Expr::Slice { expr, low, high } => {
                let target_id = self.eval_expr(expr)?;
                let target_id = crate::operators::operand(&mut self.arena, target_id);
                if let Some(Value::Bottom(_)) = self.arena.get(target_id) {
                    return Ok(target_id);
                }
                // Bounds evaluate only against a list, as before: a non-list
                // target reports itself without touching them.
                if !matches!(self.arena.get(target_id), Some(Value::List { .. })) {
                    return Ok(self.arena.bottom("slice unsupported on non-list"));
                }
                let mut low_id = None;
                if let Some(l) = low {
                    low_id = Some(self.eval_expr(l)?);
                }
                let mut high_id = None;
                if let Some(h) = high {
                    high_id = Some(self.eval_expr(h)?);
                }
                Ok(crate::operators::slice(
                    &mut self.arena,
                    target_id,
                    low_id,
                    high_id,
                ))
            }
            Expr::Call { func, args } => self.eval_call(func, args),
            Expr::Spread { expr } => {
                let val_id = self.eval_expr(expr)?;
                // Explicit open: the struct allows new fields in the merge
                // around it but stays closed itself, so the merged result is
                // still closed. Anything else (including errors) passes
                // through, which is also what `1...`-style tolerance needs.
                if let Some(Value::Struct(s)) = self.arena.get(val_id).cloned() {
                    let mut opened = s;
                    opened.is_open = true;
                    opened.spread_open = true;
                    Ok(self.arena.alloc(Value::Struct(opened)))
                } else {
                    Ok(val_id)
                }
            }
            Expr::Interpolation { parts, .. } => {
                let mut result_str = String::new();
                // An operand not concrete yet makes the result incomplete, but a
                // later operand's error still decides it (`"\(a.x) \(a.y)"` with
                // `a.y` undefined is that error), as upstream reports.
                let mut incomplete = None;
                for part in parts {
                    match part {
                        InterpolationPart::Lit(s) => result_str.push_str(s),
                        InterpolationPart::Expr(e) => {
                            let val_id = self.eval_expr(e)?;
                            let val_id = crate::operators::operand(&mut self.arena, val_id);
                            match self.arena.get(val_id) {
                                Some(Value::String(s)) => result_str.push_str(s),
                                Some(Value::Int(i)) => result_str.push_str(&i.to_string()),
                                Some(Value::Float(f)) => result_str.push_str(&f.to_string()),
                                Some(Value::Bool(b)) => result_str.push_str(&b.to_string()),
                                Some(Value::Bottom(reason))
                                    if reason.kind == BottomKind::Incomplete =>
                                {
                                    incomplete.get_or_insert(val_id);
                                }
                                // A propagated error is already the cause. Decorating it
                                // on each derivation changes the value forever, preventing
                                // the merge from settling and retaining growing messages.
                                Some(Value::Bottom(_)) => return Ok(val_id),
                                // Not concrete yet: a merge may still supply the value
                                // (`t & {p: "x"}` for `t: {p: string, l: "\(p)"}`), so
                                // the field stays incomplete instead of conflicting.
                                Some(value)
                                    if crate::operators::incomplete_interpolation_operand(
                                        value,
                                    ) =>
                                {
                                    if incomplete.is_none() {
                                        incomplete = Some(self.arena.bottom_of(
                                            BottomKind::Incomplete,
                                            "invalid interpolation: non-concrete value",
                                        ));
                                    }
                                }
                                other => {
                                    return Ok(self.arena.bottom(format!(
                                        "string interpolation requires concrete scalar value, got {other:?} for expr {e:?}"
                                    )));
                                }
                            }
                        }
                    }
                }
                Ok(incomplete.unwrap_or_else(|| self.arena.string(result_str)))
            }
            // Only a hand-built tree puts a comprehension outside a list; it
            // stands for the closed list of its yields.
            Expr::ListComp(comp) => Ok(match self.eval_list_comprehension(comp)? {
                Ok(elements) => self.arena.alloc(Value::List {
                    elements,
                    ellipsis: None,
                }),
                Err(bottom) => bottom,
            }),
        }
    }

    /// `x == _|_` and `x != _|_`: whether `x` is an error, not arithmetic on
    /// one. An operand that has not resolved yet is no answer while a loop
    /// around it can still retry, so it is returned as it is and the check
    /// stays pending; once nothing can retry, it counts as missing.
    fn eval_bottom_check(&mut self, op: BinaryOp, operand: &Expr) -> Result<ValueId, EvalError> {
        if self.selects_unset_optional(operand)? {
            return Ok(self.arena.bool(op == BinaryOp::Equal));
        }
        let id = match self.eval_expr(operand) {
            Ok(id) => id,
            Err(EvalError::Unresolved(message)) => {
                self.arena.bottom_of(BottomKind::Unresolved, message)
            }
            Err(error) => return Err(error),
        };
        let is_bottom = match self.arena.get(id) {
            Some(Value::Bottom(r)) if r.kind.may_resolve_later() && self.deferring => {
                return Ok(id);
            }
            Some(Value::Bottom(_)) => true,
            _ => false,
        };
        Ok(self.arena.bool(is_bottom == (op == BinaryOp::Equal)))
    }

    /// Whether `operand` names an optional field its struct has not set. A
    /// selector reads such a field's constraint, but for an existence check it
    /// is not there. A package member is never optional, so imports are not
    /// looked at.
    fn selects_unset_optional(&mut self, operand: &Expr) -> Result<bool, EvalError> {
        let Expr::Selector { expr, field } = operand else {
            return Ok(false);
        };
        if let Expr::Ident(name) = expr.as_ref()
            && self.imports.package_of(name).is_some()
        {
            return Ok(false);
        }
        let base = match self.eval_expr(expr) {
            Ok(base) => base,
            Err(EvalError::Unresolved(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        Ok(matches!(
            self.arena.get(base),
            Some(Value::Struct(s)) if s.fields.get(field).is_some_and(|e| e.optional)
        ))
    }

    /// Evaluate a call's arguments, then apply it. Argument evaluation is
    /// the only part that reads expressions; application is the pure
    /// [`builtins::call`](crate::builtins::call).
    pub(crate) fn eval_call(
        &mut self,
        func_expr: &Expr,
        arg_exprs: &[Expr],
    ) -> Result<ValueId, EvalError> {
        let evaluated_args = arg_exprs
            .iter()
            .map(|arg| {
                let arg_id = self.eval_expr(arg)?;
                Ok(crate::operators::argument(&mut self.arena, arg_id))
            })
            .collect::<Result<Vec<_>, EvalError>>()?;
        Ok(crate::builtins::call(
            &mut self.arena,
            &self.imports,
            func_expr,
            &evaluated_args,
        ))
    }

    /// Map a pre-validated literal to its value. Infallible by construction:
    /// the parser is the only producer of [`NumberLit`](cue_syntax::NumberLit).
    fn eval_number(&mut self, lit: &cue_syntax::NumberLit) -> ValueId {
        match lit.value() {
            cue_syntax::NumberValue::Int(i) => self.arena.alloc(Value::Int(i.clone())),
            cue_syntax::NumberValue::Float(f) => self.arena.alloc(Value::Float(*f)),
        }
    }

    /// Export an evaluated value to JSON, selecting defaults and omitting optional fields.
    pub fn to_json(&self, val_id: ValueId) -> Result<serde_json::Value, String> {
        crate::export::to_json(&self.arena, val_id)
    }

    /// Export with a caller-supplied diagnostic path.
    pub fn to_json_at_path(
        &self,
        val_id: ValueId,
        path: &str,
    ) -> Result<serde_json::Value, String> {
        crate::export::to_json_at_path(&self.arena, val_id, path)
    }
}
