use crate::closedness::{ClosedCopies, open_for_embedding, reclose};
use crate::unify::unify;
use crate::unify::{Equivalence, compare_values};
use crate::value::{
    BottomKind, BoundOp, Conjunct, DisjunctionBranch as ValueBranch, FieldEntry, Imports,
    StructValue, Thunk, ThunkEnv, TypeKind, Value, ValueArena, ValueId,
};
use cue_syntax::ast::*;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::str::FromStr;
use thiserror::Error;

/// How deep the unresolved-reference walk descends. The value graph it walks
/// can be cyclic, and the walk carries no visited set because it runs on every
/// declaration of every relaxation pass.
const MAX_UNRESOLVED_DEPTH: usize = 64;

/// Sweeps one merged struct may take to settle. A chain of n references needs
/// n of them, so this only stops a recipe that never settles at all.
const MAX_REDERIVE_SWEEPS: usize = 256;

/// What one re-derivation sweep did: which fields moved, so the next sweep knows
/// what to derive, and whether anything was written at all, which is what the
/// caller's copy on write turns on.
#[derive(Debug, Default)]
struct Sweep {
    moved: HashSet<String>,
    wrote: bool,
}

/// How many passes of a literal may be spent refining a partial value that no
/// declaration has finished reading. Bounds the chain of self-references that
/// can resolve, and stops a structural cycle refining forever.
///
/// Tied to [`MAX_UNRESOLVED_DEPTH`] rather than picked: a cycle unrolls a level
/// or more per pass, and a partial deeper than that walk's budget is *judged
/// resolved* and written with a bottom buried inside it. Eight leaves room for
/// a cycle through four fields. A chain longer than eight links therefore does
/// not resolve, where upstream resolves any length.
const MAX_REFINEMENT_PASSES: usize = MAX_UNRESOLVED_DEPTH / 8;

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

/// Which map of a struct a field lives in. Definitions and hidden fields keep
/// their sigil in the name, so the three never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Field,
    Definition,
    Hidden,
}

impl Section {
    fn map(self, s: &StructValue) -> &BTreeMap<String, FieldEntry> {
        match self {
            Section::Field => &s.fields,
            Section::Definition => &s.definitions,
            Section::Hidden => &s.hidden,
        }
    }

    fn map_mut(self, s: &mut StructValue) -> &mut BTreeMap<String, FieldEntry> {
        match self {
            Section::Field => &mut s.fields,
            Section::Definition => &mut s.definitions,
            Section::Hidden => &mut s.hidden,
        }
    }
}

const SECTIONS: [Section; 3] = [Section::Field, Section::Definition, Section::Hidden];

pub struct Evaluator {
    pub arena: ValueArena,
    scopes: Vec<HashMap<String, ValueId>>,
    resolving_symbols: HashSet<String>,
    /// Environment of the struct literal being evaluated, which its fields
    /// capture as the scope their recipes run in.
    current_env: Option<Rc<ThunkEnv>>,
    pub placeholders: HashMap<String, ValueId>,
    /// How many field recipes re-derivation has run, and how many structs it
    /// gave up on at the sweep cap. Evaluation never reads them; they are what a
    /// test asserts on to keep the pass proportional to what a merge changed
    /// rather than to the size of the struct it changed it in.
    pub derivations: usize,
    pub unsettled: usize,
    /// The packages the file being evaluated imported. A literal captures this
    /// in its `ThunkEnv`, so a recipe derived again after it crossed an import
    /// boundary resolves the packages *its* file imported rather than the
    /// importer's.
    pub imports: Rc<Imports>,
    expr_depth: usize,
    /// The closed copy each definition read so far resolves to.
    closed: ClosedCopies,
    /// Whether the literal being evaluated is a file's top level, which an
    /// embedded definition does not close.
    at_file_root: bool,
    /// Set while evaluating an embedding at a file's top level. Upstream merges
    /// such an embedding without closedness at any depth, so the definitions it
    /// reads are left open.
    reading_root_embedding: bool,
    /// Set by the package loader: which values to record their directory on.
    pub(crate) origin: Option<crate::package::OriginAnnotation>,
    /// Set while some enclosing relaxation loop can still retry. A reference
    /// that has not resolved yet is then not an answer: an existence check or a
    /// comprehension's condition stays pending instead of deciding on it.
    deferring: bool,
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
            scopes: vec![HashMap::new()],
            resolving_symbols: HashSet::new(),
            current_env: None,
            placeholders: HashMap::new(),
            derivations: 0,
            unsettled: 0,
            imports: Rc::new(Imports::default()),
            expr_depth: 0,
            closed: ClosedCopies::default(),
            at_file_root: false,
            reading_root_embedding: false,
            origin: None,
            deferring: false,
        };
        evaluator.register_builtins();
        evaluator
    }

    fn register_builtins(&mut self) {
        // Top-level type builtins
        let top = self.arena.alloc(Value::Type(TypeKind::Top));
        let null = self.arena.alloc(Value::Type(TypeKind::Null));
        let bool_t = self.arena.alloc(Value::Type(TypeKind::Bool));
        let int_t = self.arena.alloc(Value::Type(TypeKind::Int));
        let uint_t = self.arena.alloc(Value::Type(TypeKind::Uint));
        let uint8_t = self.arena.alloc(Value::Type(TypeKind::Uint8));
        let uint16_t = self.arena.alloc(Value::Type(TypeKind::Uint16));
        let uint32_t = self.arena.alloc(Value::Type(TypeKind::Uint32));
        let uint64_t = self.arena.alloc(Value::Type(TypeKind::Uint64));
        let int8_t = self.arena.alloc(Value::Type(TypeKind::Int8));
        let int16_t = self.arena.alloc(Value::Type(TypeKind::Int16));
        let int32_t = self.arena.alloc(Value::Type(TypeKind::Int32));
        let int64_t = self.arena.alloc(Value::Type(TypeKind::Int64));
        let float_t = self.arena.alloc(Value::Type(TypeKind::Float));
        let float32_t = self.arena.alloc(Value::Type(TypeKind::Float32));
        let float64_t = self.arena.alloc(Value::Type(TypeKind::Float64));
        let num_t = self.arena.alloc(Value::Type(TypeKind::Number));
        let string_t = self.arena.alloc(Value::Type(TypeKind::String));
        let bytes_t = self.arena.alloc(Value::Type(TypeKind::Bytes));

        self.insert_binding("_", top);
        self.insert_binding("null", null);
        self.insert_binding("bool", bool_t);
        self.insert_binding("int", int_t);
        self.insert_binding("uint", uint_t);
        self.insert_binding("uint8", uint8_t);
        self.insert_binding("uint16", uint16_t);
        self.insert_binding("uint32", uint32_t);
        self.insert_binding("uint64", uint64_t);
        self.insert_binding("int8", int8_t);
        self.insert_binding("int16", int16_t);
        self.insert_binding("int32", int32_t);
        self.insert_binding("int64", int64_t);
        self.insert_binding("float", float_t);
        self.insert_binding("float32", float32_t);
        self.insert_binding("float64", float64_t);
        self.insert_binding("number", num_t);
        self.insert_binding("string", string_t);
        self.insert_binding("bytes", bytes_t);
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert_binding(&mut self, name: &str, val: ValueId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), val);
        }
    }

    /// Drop a binding this scope added, exposing whatever an enclosing scope
    /// binds the same name to.
    fn remove_binding(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.remove(name);
        }
    }

    pub fn lookup_binding(&self, name: &str) -> Option<ValueId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    /// Evaluate an entire CUE source file.
    pub fn eval_file(&mut self, file: &SourceFile) -> Result<ValueId, EvalError> {
        for imp in &file.imports {
            let pkg_name = if let Some(alias) = &imp.alias {
                alias.clone()
            } else {
                imp.path
                    .split('/')
                    .next_back()
                    .unwrap_or(&imp.path)
                    .to_string()
            };
            Rc::make_mut(&mut self.imports)
                .aliases
                .insert(pkg_name, imp.path.clone());
        }

        let mut root_struct = StructValue::new(false);
        self.eval_root_decls(&file.decls, &mut root_struct)?;
        Ok(self.arena.alloc(Value::Struct(root_struct)))
    }

    /// Evaluate the top level of a file or package. It is never closed: a
    /// definition embedded there constrains nothing beside it.
    pub fn eval_root_decls(
        &mut self,
        decls: &[Decl],
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        let enclosing = std::mem::replace(&mut self.at_file_root, true);
        let result = self.eval_decls_into_struct(decls, target_struct);
        self.at_file_root = enclosing;
        result
    }

    pub fn eval_decls_into_struct(
        &mut self,
        decls: &[Decl],
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        // The literal being evaluated owns `current_env` while it runs, and the
        // one it is written inside takes it back afterwards.
        let enclosing = self.current_env.take();
        let result = self.eval_decls_scoped(decls, target_struct);
        self.current_env = enclosing;
        result
    }

    fn eval_decls_scoped(
        &mut self,
        decls: &[Decl],
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        // A reference must see every declaration of a static field, including
        // declarations appearing after the reference in source order.
        let decls = Self::collect_field_declarations(decls);
        // Captured once per literal and shared by its thunks: every field of this
        // struct was written in the same lexical scope.
        let env = Rc::new(ThunkEnv::new(
            self.scopes.clone(),
            Self::collect_let_declarations(&decls),
            Self::collect_field_names(&decls),
            self.imports.clone(),
        ));
        self.current_env = Some(env.clone());
        let dynamic_base = decls
            .iter()
            .any(|decl| {
                matches!(
                    decl,
                    Decl::Field(FieldDecl {
                        label: Label::Dynamic(_),
                        ..
                    })
                )
            })
            .then(|| (target_struct.clone(), self.scopes.clone()));
        // Pass 1: Pre-register definition placeholders for recursive and forward references
        for decl in &decls {
            if let Decl::Field(f) = decl
                && let Some(name) = f.label.name()
                && f.label.is_definition()
                && !self.placeholders.contains_key(name)
            {
                let placeholder = self.arena.alloc(Value::RecursiveRef {
                    name: name.to_string(),
                    target: None,
                });
                self.insert_binding(name, placeholder);
                self.placeholders.insert(name.to_string(), placeholder);
            }
        }

        // Multi-pass relaxation loop for forward / order-independent references
        let mut pending_decls: Vec<&Decl> = decls.iter().collect();
        let max_iterations = decls.len() + 3;
        let mut iteration = 0;
        let mut refinements = 0;

        while !pending_decls.is_empty() && iteration < max_iterations + refinements {
            iteration += 1;
            // What the pending declarations are bound to going in. A pass that
            // resolves nothing may still have refined one of these, and then the
            // next pass has something new to read.
            let before: Vec<Option<ValueId>> = pending_decls
                .iter()
                .map(|decl| Self::pending_binding_name(decl).and_then(|n| self.lookup_binding(n)))
                .collect();
            let mut next_pending = Vec::new();
            let mut made_progress = false;

            for &decl in &pending_decls {
                let resolved = self.eval_single_decl(decl, target_struct, &env, false)?;
                if resolved {
                    made_progress = true;
                } else {
                    next_pending.push(decl);
                }
            }

            if !made_progress {
                let refining = self.refined_any(&pending_decls, &before);
                if refining && refinements < MAX_REFINEMENT_PASSES {
                    refinements += 1;
                    pending_decls = next_pending;
                    continue;
                }
                if refining {
                    // Still growing with the allowance spent: the value does not
                    // converge. Drop the partials so the final pass reports the
                    // reference at the link it is written on rather than at the
                    // bottom of however many levels were unrolled getting here.
                    for &decl in &next_pending {
                        if let Some(name) = Self::pending_binding_name(decl) {
                            self.remove_binding(name);
                        }
                    }
                }
                // Saturated / cannot resolve further; final evaluation accepts bottom errors
                for &decl in &next_pending {
                    self.eval_single_decl(decl, target_struct, &env, true)?;
                }
                break;
            }

            pending_decls = next_pending;
        }

        // Once computed labels are known, collect their declarations with the
        // static fields and evaluate references again from the original scope.
        // Otherwise an earlier reference can retain a pre-merge snapshot.
        if let Some((base_struct, base_scopes)) = dynamic_base {
            let mut named_decls = decls.clone();
            for decl in &mut named_decls {
                if let Decl::Field(field) = decl
                    && let Label::Dynamic(expr) = &field.label
                {
                    let label = self.eval_expr(expr)?;
                    match self.arena.get(label) {
                        Some(Value::String(name)) => field.label = Label::String(name.clone()),
                        _ => {
                            return Err(EvalError::Unresolved(
                                "unresolved reference or non-string dynamic field label"
                                    .to_string(),
                            ));
                        }
                    }
                }
            }
            *target_struct = base_struct;
            self.scopes = base_scopes;
            return self.eval_decls_into_struct(&named_decls, target_struct);
        }

        // A comprehension, an embedding or a pattern constraint can change a field
        // after another field has already read it. Those are the paths whose
        // readers need deriving again; an ordinary literal is derived again by
        // the merge that changed it.
        let needs_rederive = !target_struct.pattern_constraints.is_empty()
            || decls
                .iter()
                .any(|d| matches!(d, Decl::Comprehension(_) | Decl::Embedding(_)));

        // Apply pattern constraints to matching fields
        let pattern_constraints = target_struct.pattern_constraints.clone();
        for pc in pattern_constraints {
            let field_names: Vec<String> = target_struct.fields.keys().cloned().collect();
            for field_name in field_names {
                let name_id = self.arena.string(field_name.clone());
                let match_res = crate::unify::unify(&mut self.arena, pc.pattern_val, name_id);
                if !matches!(self.arena.get(match_res), Some(Value::Bottom(_)))
                    && let Some(entry) = target_struct.fields.get_mut(&field_name)
                {
                    let new_val = crate::unify::unify(&mut self.arena, entry.val, pc.target_val);
                    entry.val = new_val;
                }
            }
        }

        if needs_rederive {
            let mut visiting = HashSet::new();
            self.rederive_struct(target_struct, &mut visiting)?;
        }

        Ok(())
    }

    /// Field names one literal declares outright. A reference inside it resolves
    /// to these at whatever value the merged struct gives them; anything else it
    /// names belongs to an enclosing scope and keeps resolving there.
    /// The name a pending declaration binds its partial value to, if any. The
    /// same three conditions as the binding itself: a definition or a hidden
    /// field is not read this way.
    fn pending_binding_name(decl: &Decl) -> Option<&str> {
        match decl {
            Decl::Field(f) if !f.label.is_definition() && !f.label.is_hidden() => f.label.name(),
            _ => None,
        }
    }

    /// Whether a pass that resolved no declaration nevertheless left one of them
    /// bound to more than it was.
    ///
    /// This is what lets a chain of self-references resolve. `stages: {a: …, b:
    /// {needs: [stages.a]}, c: {needs: [stages.b]}}` is one declaration at this
    /// level, so no pass of it ever "resolves" anything until the whole chain
    /// does; without this the loop would give up after the first. Each pass
    /// binds a partial `stages` one link deeper, and a chain of n links needs n
    /// of them - a property of the user's graph, not of this literal's
    /// declaration count, which is why the allowance is separate.
    ///
    /// It is bounded because a structural cycle refines forever: `a: {x: a}`
    /// grows a level per pass and never finishes.
    fn refined_any(&self, pending: &[&Decl], before: &[Option<ValueId>]) -> bool {
        pending.iter().zip(before).any(|(decl, &prev)| {
            let Some(name) = Self::pending_binding_name(decl) else {
                return false;
            };
            match (prev, self.lookup_binding(name)) {
                (None, Some(_)) => true,
                (Some(prev), Some(now)) => {
                    prev != now && compare_values(&self.arena, prev, now) != Equivalence::Equal
                }
                _ => false,
            }
        })
    }

    fn collect_field_names(decls: &[Decl]) -> HashSet<String> {
        decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Field(field) => field.label.name().map(str::to_string),
                _ => None,
            })
            .collect()
    }

    /// `let` and alias declarations of one literal, in declaration order. They are
    /// scope bindings rather than fields, so a thunk cannot read them back from
    /// the struct it is forced against and has to derive them again.
    fn collect_let_declarations(decls: &[Decl]) -> Vec<(String, Rc<Expr>)> {
        decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Let { ident, expr } | Decl::Alias { ident, expr } => {
                    Some((ident.clone(), Rc::new(expr.clone())))
                }
                _ => None,
            })
            .collect()
    }

    fn collect_field_declarations(decls: &[Decl]) -> Vec<Decl> {
        let mut collected: Vec<Decl> = Vec::with_capacity(decls.len());
        let mut positions = HashMap::new();
        for decl in decls {
            if let Decl::Field(field) = decl
                && let Some(name) = field.label.name()
            {
                // Quoted labels share the ordinary namespace, even when their
                // text starts with a definition or hidden-field prefix.
                let key = (
                    name.to_string(),
                    field.label.is_definition(),
                    field.label.is_hidden(),
                );
                if let Some(&index) = positions.get(&key) {
                    let Decl::Field(previous) = &mut collected[index] else {
                        unreachable!();
                    };
                    previous.optional &= field.optional;
                    previous.value = Expr::Binary {
                        op: BinaryOp::Unify,
                        left: Box::new(previous.value.clone()),
                        right: Box::new(field.value.clone()),
                    };
                    continue;
                }
                positions.insert(key, collected.len());
            }
            collected.push(decl.clone());
        }
        collected
    }

    /// Whether a value is the placeholder of a definition not evaluated yet.
    fn is_placeholder(&self, val_id: ValueId) -> bool {
        matches!(
            self.arena.get(val_id),
            Some(Value::RecursiveRef { target: None, .. })
        )
    }

    /// Whether a value carries a reference nothing has resolved yet.
    ///
    /// Two callers, one meaning: the relaxation loop retries a declaration that
    /// answers yes, and a re-derived recipe that answers yes keeps the value it
    /// had rather than replacing it with a reference the merge cannot satisfy.
    ///
    /// Every composite is walked, not only the ones that hold a field. A
    /// disjunction is the one that bites: `string | *"\(pkg.name)"` deriving to
    /// a default branch of bottom exports as `_|_`, so judging it resolved
    /// overwrites a good value with a broken one.
    fn is_unresolved(&self, val_id: ValueId) -> bool {
        self.is_unresolved_within(val_id, MAX_UNRESOLVED_DEPTH)
    }

    fn is_unresolved_within(&self, val_id: ValueId, depth: usize) -> bool {
        // A value graph can be cyclic. Past the budget, answer as this walk did
        // before it descended into composites at all: resolved, and written.
        let Some(depth) = depth.checked_sub(1) else {
            return false;
        };
        let any = |ids: &mut dyn Iterator<Item = ValueId>| -> bool {
            for id in ids {
                if self.is_unresolved_within(id, depth) {
                    return true;
                }
            }
            false
        };
        match self.arena.get(val_id) {
            Some(Value::Bottom(reason)) => reason.kind.may_resolve_later(),
            Some(Value::Struct(s)) => any(&mut s
                .fields
                .values()
                .chain(s.definitions.values())
                .chain(s.hidden.values())
                .map(|f| f.val)),
            Some(Value::List { elements, ellipsis }) => {
                any(&mut elements.iter().copied().chain(*ellipsis))
            }
            Some(Value::Disjunction { branches }) => any(&mut branches.iter().map(|b| b.val)),
            Some(Value::Bounds { constraints, .. }) => {
                any(&mut constraints.iter().map(|(_, id)| *id))
            }
            Some(Value::Validators(targets)) => any(&mut targets.iter().copied()),
            Some(Value::BuiltinValidator { target, .. }) => {
                self.is_unresolved_within(*target, depth)
            }
            _ => false,
        }
    }

    fn eval_single_decl(
        &mut self,
        decl: &Decl,
        target_struct: &mut StructValue,
        env: &Rc<ThunkEnv>,
        final_pass: bool,
    ) -> Result<bool, EvalError> {
        // A literal's final pass settles only its own declarations; a loop
        // around it that still retries may yet bind what they read.
        let enclosing = self.deferring;
        self.deferring = enclosing || !final_pass;
        let result = self.eval_decl_pass(decl, target_struct, env, final_pass);
        self.deferring = enclosing;
        result
    }

    fn eval_decl_pass(
        &mut self,
        decl: &Decl,
        target_struct: &mut StructValue,
        env: &Rc<ThunkEnv>,
        final_pass: bool,
    ) -> Result<bool, EvalError> {
        match decl {
            Decl::Field(f) => match &f.label {
                Label::Pattern(pattern_expr) => {
                    let pattern_val = self.eval_expr(pattern_expr)?;
                    let target_val = self.eval_expr(&f.value)?;
                    target_struct.add_pattern_constraint(pattern_val, target_val);
                    Ok(!self.is_unresolved(target_val))
                }
                Label::Dynamic(dyn_expr) => {
                    let label_val_id = self.eval_expr(dyn_expr)?;
                    if let Some(Value::String(name)) = self.arena.get(label_val_id) {
                        let name = name.clone();
                        let (val_id, conjunct) = self.eval_field_value(&f.value, env)?;
                        let is_unresolved = self.is_unresolved(val_id);
                        if is_unresolved && !final_pass {
                            return Ok(false);
                        }
                        let val_id = self.unify_decl_field(
                            &mut target_struct.fields,
                            &name,
                            val_id,
                            f.optional,
                            conjunct,
                        )?;
                        self.note_field_name(&name);
                        self.insert_binding(&name, val_id);
                        Ok(!is_unresolved)
                    } else if self.is_unresolved(label_val_id) {
                        if final_pass {
                            return Err(EvalError::Unresolved(
                                "unresolved reference in dynamic field label".to_string(),
                            ));
                        }
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }
                _ => {
                    let (val_id, conjunct) = self.eval_field_value(&f.value, env)?;
                    // A definition read before its declaration was evaluated
                    // absorbs whatever it is unified with: it is no value yet.
                    // Only at the top: inside a value it is how a recursive
                    // definition refers to itself.
                    let placeholder = self.is_placeholder(val_id);
                    let is_unresolved = placeholder || self.is_unresolved(val_id);
                    // A retry must not meet a transient unresolved-reference bottom
                    // with a resolved declaration: bottom would permanently win.
                    if is_unresolved && !final_pass {
                        // Bind what this pass *did* resolve, so the next one can
                        // read it. Without this a field cannot see the field
                        // that encloses it - `stages: {build: …, test: {needs:
                        // [stages.build]}}` evaluates `stages.build` while
                        // `stages` is still being built, so `stages` is unbound,
                        // and no later pass learns anything new because nothing
                        // ever bound it. A sibling forward reference works for
                        // the opposite reason: its name is bound by the time the
                        // reader is retried.
                        //
                        // Nothing is written into the struct - the return below
                        // still comes before `unify_decl_field` - so a transient
                        // bottom cannot win the field. Only the scope gains a
                        // name, and only for the next pass.
                        if let Some(name) = f.label.name()
                            && !f.label.is_definition()
                            && !f.label.is_hidden()
                            && !placeholder
                        {
                            let partial = match target_struct.fields.get(name) {
                                Some(entry) => unify(&mut self.arena, entry.val, val_id),
                                None => val_id,
                            };
                            self.insert_binding(name, partial);
                        }
                        return Ok(false);
                    }
                    if let Some(name) = f.label.name() {
                        let fields = if f.label.is_definition() {
                            &mut target_struct.definitions
                        } else if f.label.is_hidden() {
                            &mut target_struct.hidden
                        } else {
                            &mut target_struct.fields
                        };
                        let val_id =
                            self.unify_decl_field(fields, name, val_id, f.optional, conjunct)?;
                        if f.label.is_definition()
                            && let Some(&placeholder_id) = self.placeholders.get(name)
                        {
                            // A recursive reference reads the definition, so it
                            // meets the closed value like any other reader.
                            let closed = self.closed.close(&mut self.arena, val_id);
                            if let Some(Value::RecursiveRef { target, .. }) =
                                self.arena.get_mut(placeholder_id)
                            {
                                *target = Some(closed);
                            }
                        }
                        self.insert_binding(name, val_id);
                    }
                    Ok(!is_unresolved)
                }
            },
            Decl::Alias { ident, expr } | Decl::Let { ident, expr } => {
                // The binding is part of the literal's environment, so a field
                // that reads it derives it again beside itself.
                let val_id = self.eval_expr(expr)?;
                let is_unresolved = self.is_unresolved(val_id);
                self.insert_binding(ident, val_id);
                Ok(!is_unresolved)
            }
            Decl::Embedding(expr) => {
                let enclosing = self.reading_root_embedding;
                self.reading_root_embedding |= self.at_file_root;
                let embedded_id = self.eval_expr(expr);
                self.reading_root_embedding = enclosing;
                let embedded_id = embedded_id?;
                let is_unresolved = self.is_unresolved(embedded_id);
                if is_unresolved && !final_pass {
                    return Ok(false);
                }
                let current_id = self.arena.alloc(Value::Struct(target_struct.clone()));
                let (embedded_id, was_closed) = open_for_embedding(&mut self.arena, embedded_id);
                let unified_id = unify(&mut self.arena, current_id, embedded_id);
                let unified_id = if was_closed && !self.at_file_root {
                    reclose(&mut self.arena, unified_id)
                } else {
                    unified_id
                };
                if let Some(Value::Struct(s)) = self.arena.get(unified_id) {
                    // An embedding may change a field another field has already
                    // read; the pass at the end of the literal derives those
                    // readers again.
                    *target_struct = s.clone();
                    self.bind_struct_fields(target_struct);
                } else if let Some(Value::Bottom(reason)) = self.arena.get(unified_id) {
                    // An embedding whose reference has not resolved yet is the
                    // enclosing literal's business, not a failure: it carries the
                    // bottom to its own retry loop.
                    let message = reason.to_string();
                    return Err(if reason.kind.may_resolve_later() {
                        EvalError::Unresolved(message)
                    } else {
                        EvalError::Evaluation(message)
                    });
                }
                Ok(!is_unresolved)
            }
            Decl::Ellipsis(_) => {
                target_struct.is_open = true;
                Ok(true)
            }
            Decl::Comprehension(comp) => {
                // A comprehension that waits on a reference yields nothing yet:
                // the fields it generated before it stopped are dropped with it.
                let mut scratch = target_struct.clone();
                match self.eval_comprehension(comp, &mut scratch) {
                    Err(EvalError::Unresolved(_)) if !final_pass => return Ok(false),
                    other => other?,
                }
                *target_struct = scratch;
                // Loop-local bindings have been popped; subsequent references
                // must resolve to the final generated field values.
                self.bind_struct_fields(target_struct);
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// What reading `name` yields: a definition's closed copy, anything else as
    /// it is.
    fn read_definition(&mut self, name: &str, val: ValueId) -> ValueId {
        if !self.reading_root_embedding && (name.starts_with('#') || name.starts_with("_#")) {
            self.closed.close(&mut self.arena, val)
        } else {
            val
        }
    }

    /// Make the struct's fields visible to the declarations that follow.
    ///
    /// This does not record them as names the literal declares: a field an
    /// embedding contributed, or one a comprehension generated, was not written
    /// here, so a reference to that name means the enclosing scope's and must go
    /// on meaning it after a merge.
    fn bind_struct_fields(&mut self, value: &StructValue) {
        for (name, entry) in value
            .fields
            .iter()
            .chain(&value.definitions)
            .chain(&value.hidden)
        {
            self.insert_binding(name, entry.val);
        }
    }

    /// Record a field name as reachable from the literal being evaluated.
    /// Whether the literal being evaluated declares this name as a field.
    ///
    /// Only the innermost literal is asked, which is all the evaluator keeps:
    /// an enclosing literal's environment is saved and restored around this
    /// one. A name an *enclosing* literal declares therefore reads as not
    /// found until the pass that binds it, which is a wording difference on a
    /// value that is an error either way.
    fn declares_field(&self, name: &str) -> bool {
        self.current_env
            .as_ref()
            .is_some_and(|env| env.owns_field(name))
    }

    fn note_field_name(&self, name: &str) {
        if let Some(env) = &self.current_env {
            env.note_field(name);
        }
    }

    fn unify_decl_field(
        &mut self,
        fields: &mut std::collections::BTreeMap<String, FieldEntry>,
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
                entry.conjuncts.push(conjunct.clone());
            })
            .or_insert_with(|| FieldEntry::with_conjuncts(val, optional, vec![conjunct]));
        Ok(entry.val)
    }

    /// Evaluate a field expression and keep the recipe that produced it: the
    /// expression and the scope its struct literal was written in. A merge that
    /// changes what the expression read runs it again from there.
    fn eval_field_value(
        &mut self,
        expr: &Expr,
        env: &Rc<ThunkEnv>,
    ) -> Result<(ValueId, Conjunct), EvalError> {
        let val = self.eval_expr(expr)?;
        let deps = Rc::new(crate::deps::recipe_deps(expr, &env.lets));
        Ok((
            val,
            Conjunct::Thunk(Thunk {
                expr: Rc::new(expr.clone()),
                env: env.clone(),
                deps,
            }),
        ))
    }

    /// Derive again the fields of a struct the evaluator has just merged.
    ///
    /// A field beside the one the merge changed was evaluated against the value
    /// that field had before, so its recipe runs again here, against the merged
    /// struct, and replaces what it produced the first time. Replacement rather
    /// than unification: the cached value came from the same recipe, and
    /// `"v5" & "v9"` is bottom.
    fn rederive(&mut self, id: ValueId) -> Result<ValueId, EvalError> {
        let mut visiting = HashSet::new();
        self.rederive_value(id, &mut visiting)
    }

    /// Unify two values in this evaluator's arena and re-derive any reader fields affected by the merge.
    pub fn unify_and_rederive(&mut self, v1: ValueId, v2: ValueId) -> Result<ValueId, EvalError> {
        let merged = unify(&mut self.arena, v1, v2);
        self.rederive(merged)
    }

    fn rederive_value(
        &mut self,
        id: ValueId,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<ValueId, EvalError> {
        let Some(Value::Struct(s)) = self.arena.get(id) else {
            return Ok(id);
        };
        if !visiting.insert(id) {
            // Already being derived further up: a cyclic graph, not a second copy.
            return Ok(id);
        }
        let mut s = s.clone();
        let changed = self.rederive_struct(&mut s, visiting);
        visiting.remove(&id);
        // Copy on write, so a value merged into this one keeps the id it had.
        Ok(if changed? {
            self.arena.alloc(Value::Struct(s))
        } else {
            id
        })
    }

    fn rederive_struct(
        &mut self,
        s: &mut StructValue,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<bool, EvalError> {
        // Nothing merged into this struct, so no recipe's inputs moved: a name
        // the literal does not declare resolves in the scope it was written in,
        // which no merge can change.
        // A field the merge gave a second recipe to is one whose value it may
        // have moved; everything else in the struct is exactly what it was.
        let mut moved: HashSet<String> = SECTIONS
            .iter()
            .flat_map(|section| section.map(s))
            .filter(|(_, entry)| entry.conjuncts.len() > 1)
            .map(|(name, _)| name.clone())
            .collect();
        if moved.is_empty() {
            return Ok(false);
        }
        // A pattern constraint applies to a field without appearing among its
        // conjuncts, so once the merge brought one in, treat every field as
        // moved rather than reason about which labels it matches.
        if !s.pattern_constraints.is_empty() {
            moved = SECTIONS
                .iter()
                .flat_map(|section| section.map(s))
                .map(|(name, _)| name.clone())
                .collect();
        }

        let order = Self::derivation_order(s);
        let mut changed = false;
        // Two things move a field here. Deriving the field beside it, and
        // descending into a field the unifier merged, which settles a nested
        // override that a field above it reads. Both feed the same worklist, so
        // both run in one loop until nothing moves: in dependency order one
        // sweep settles a chain, whichever way its names sort, and a cycle among
        // recipes needs another. The cap is a backstop that leaves the values as
        // they are rather than inventing a cycle the file does not have.
        let mut settled = false;
        for _ in 0..MAX_REDERIVE_SWEEPS {
            let descent = self.rederive_children(s, visiting)?;
            changed |= descent.wrote;
            moved.extend(descent.moved);

            let sweep = self.rederive_sweep(s, &order, &moved)?;
            changed |= sweep.wrote;
            moved = sweep.moved;
            if moved.is_empty() {
                settled = true;
                break;
            }
        }
        if !settled {
            self.unsettled += 1;
        }
        Ok(changed)
    }

    /// Descend into the fields the unifier merged, reporting the ones whose
    /// value the descent changed.
    ///
    /// Where the unifier merged two sides, their nested merges are below this
    /// value and are reached the same way. A field above one of them reads the
    /// settled value, not the value the merge left behind, so what moves here
    /// joins what the sweep beside it has to derive again.
    fn rederive_children(
        &mut self,
        s: &mut StructValue,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<Sweep, EvalError> {
        let mut descent = Sweep::default();
        for section in SECTIONS {
            let names: Vec<String> = section.map(s).keys().cloned().collect();
            for name in names {
                let Some(entry) = section.map(s).get(&name) else {
                    continue;
                };
                if entry.conjuncts.len() < 2 {
                    continue;
                }
                let val = entry.val;
                let derived = self.rederive_value(val, visiting)?;
                if derived == val {
                    continue;
                }
                if let Some(entry) = section.map_mut(s).get_mut(&name) {
                    entry.val = derived;
                }
                descent.wrote = true;
                descent.moved.insert(name);
            }
        }
        Ok(descent)
    }

    /// The order to derive a struct's fields in: a field after the fields it
    /// reads, so one sweep carries a change the whole length of a chain.
    ///
    /// Only names this struct holds are edges - anything else a recipe mentions
    /// comes from an enclosing scope, which a merge here cannot change. Recipes
    /// that read each other have no such order, and are left to the sweep loop.
    fn derivation_order(s: &StructValue) -> Vec<(Section, String)> {
        let mut nodes: Vec<(Section, String)> = Vec::new();
        for section in SECTIONS {
            nodes.extend(section.map(s).keys().map(|name| (section, name.clone())));
        }
        let held: HashSet<&str> = nodes.iter().map(|(_, name)| name.as_str()).collect();

        // Fields that read this one, and how many of a field's reads are still
        // to come: Kahn's algorithm over the reads-within-this-struct graph.
        let mut readers: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut pending: Vec<usize> = vec![0; nodes.len()];
        for (index, (section, name)) in nodes.iter().enumerate() {
            let Some(entry) = section.map(s).get(name) else {
                continue;
            };
            let mut read: HashSet<&str> = entry.deps().filter(|dep| held.contains(dep)).collect();
            // A recipe that reads its own field is a cycle of one, and waiting
            // for itself would keep it out of the order entirely.
            read.remove(name.as_str());
            pending[index] = read.len();
            for dep in read {
                readers.entry(dep).or_default().push(index);
            }
        }

        let mut order: Vec<(Section, String)> = Vec::with_capacity(nodes.len());
        let mut ready: Vec<usize> = (0..nodes.len()).filter(|i| pending[*i] == 0).collect();
        let mut placed = vec![false; nodes.len()];
        while let Some(index) = ready.pop() {
            if std::mem::replace(&mut placed[index], true) {
                continue;
            }
            order.push(nodes[index].clone());
            if let Some(dependents) = readers.get(nodes[index].1.as_str()) {
                for dependent in dependents {
                    pending[*dependent] = pending[*dependent].saturating_sub(1);
                    if pending[*dependent] == 0 {
                        ready.push(*dependent);
                    }
                }
            }
        }
        // Whatever a cycle left behind keeps its map order.
        order.extend(
            nodes
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !placed[*index])
                .map(|(_, node)| node),
        );
        order
    }

    /// One sweep over the recipes that read a name the last sweep moved,
    /// reporting the names this one moved.
    ///
    /// A recipe that produces the same value again has not moved: deriving one
    /// expression twice yields two ids for one value, and treating that as a
    /// change would never converge.
    fn rederive_sweep(
        &mut self,
        s: &mut StructValue,
        order: &[(Section, String)],
        moved: &HashSet<String>,
    ) -> Result<Sweep, EvalError> {
        // Within the sweep a name that moves counts immediately, so a field
        // later in the order sees it without waiting for the next sweep.
        let mut live = moved.clone();
        let mut sweep = Sweep::default();
        for (section, name) in order {
            let Some(entry) = section.map(s).get(name).cloned() else {
                continue;
            };
            if !entry.reads_any(&live) {
                continue;
            }
            let Some(val) = self.derive_field(&entry, s, name)? else {
                continue;
            };
            if val == entry.val {
                continue;
            }
            let verdict = compare_values(&self.arena, val, entry.val);
            if verdict == Equivalence::Equal {
                continue;
            }
            // Keep the derived value either way: it was produced from the
            // merged struct, so it is at least as derived as the one it
            // replaces.
            if let Some(entry) = section.map_mut(s).get_mut(name) {
                entry.val = val;
            }
            sweep.wrote = true;
            // A value too large to compare settles here rather than moving. The
            // other choice - counting it as movement - is what makes a struct of
            // large literals sweep to the cap, re-deriving everything each time.
            if verdict == Equivalence::Unknown {
                continue;
            }
            live.insert(name.clone());
            sweep.moved.insert(name.clone());
        }
        Ok(sweep)
    }

    /// Unify one field's recipes again. `None` keeps the value it has: a recipe
    /// that derives to an unresolved reference once no loop can retry is one
    /// the merge cannot satisfy, not a field that lost its value. While a loop
    /// can still retry, the unresolved value is written instead (see
    /// [`Self::derive_thunk`]).
    fn derive_field(
        &mut self,
        entry: &FieldEntry,
        s: &StructValue,
        name: &str,
    ) -> Result<Option<ValueId>, EvalError> {
        self.derivations += 1;
        let mut val: Option<ValueId> = None;
        for conjunct in &entry.conjuncts {
            let conjunct_val = match conjunct {
                Conjunct::Value(val) => *val,
                Conjunct::Thunk(thunk) => match self.derive_thunk(thunk, s)? {
                    Some(derived) => derived,
                    None => return Ok(None),
                },
            };
            val = Some(match val {
                None => conjunct_val,
                Some(previous) => unify(&mut self.arena, previous, conjunct_val),
            });
        }
        let Some(mut val) = val else {
            return Ok(None);
        };

        // Pattern constraints are part of the field's value, not of its recipe.
        for pc in s.pattern_constraints.clone() {
            if crate::unify::field_matches_pattern(&self.arena, pc.pattern_val, name) {
                val = unify(&mut self.arena, val, pc.target_val);
            }
        }
        Ok(Some(val))
    }

    /// Evaluate one recipe in the scope it was written in, under one frame
    /// holding the merged values of the names its literal declares.
    ///
    /// Only those names: a literal that reads `policy` without declaring it
    /// means the `policy` of the scope it was written in, not one a repeated
    /// declaration contributed. A nested literal needs no frame of its own,
    /// because deriving this field evaluates that literal again from here.
    fn derive_thunk(
        &mut self,
        thunk: &Thunk,
        s: &StructValue,
    ) -> Result<Option<ValueId>, EvalError> {
        let saved_scopes = std::mem::replace(&mut self.scopes, thunk.env.scopes.clone());
        let saved_env = self.current_env.replace(thunk.env.clone());
        // The recipe may have been written in another file, and `pkg.Name` means
        // what `pkg` names there.
        let saved_imports = std::mem::replace(&mut self.imports, thunk.env.imports.clone());

        self.push_scope();
        for section in SECTIONS {
            for (name, entry) in section.map(s) {
                if thunk.env.owns_field(name) {
                    self.insert_binding(name, entry.val);
                }
            }
        }

        // A binding is a recipe too, and reads the merged values beside it.
        let mut result = Ok(None);
        for (name, expr) in thunk.env.lets.clone() {
            match self.eval_expr(&expr) {
                // A binding that cannot be derived here keeps the value it was
                // captured with; deriving again may only improve it.
                Ok(val) if !self.is_unresolved(val) => self.insert_binding(&name, val),
                Ok(_) => {}
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        if result.is_ok() {
            // While a loop around can still retry, the value this recipe had
            // is stale - it was derived before the merge - so the reference it
            // waits on is the answer: keeping the stale value would pass the
            // partial off as resolved.
            result = self
                .eval_expr(&thunk.expr)
                .map(|val| (self.deferring || !self.is_unresolved(val)).then_some(val));
        }

        self.scopes = saved_scopes;
        self.current_env = saved_env;
        self.imports = saved_imports;
        result
    }

    fn eval_comprehension(
        &mut self,
        comp: &ComprehensionDecl,
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        if comp.clauses.is_empty() {
            return Ok(());
        }

        self.eval_comprehension_clause(0, comp, target_struct)
    }

    fn eval_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &ComprehensionDecl,
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        if clause_idx >= comp.clauses.len() {
            // Reached the body: evaluate decls in struct_lit
            self.eval_decls_into_struct(&comp.struct_lit.decls, target_struct)?;
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => {
                let cond_val = self.eval_expr(condition)?;
                match self.arena.get(cond_val) {
                    Some(Value::Bool(true)) => {
                        self.eval_comprehension_clause(clause_idx + 1, comp, target_struct)?;
                    }
                    Some(Value::Bottom(r)) if r.kind.may_resolve_later() && self.deferring => {
                        return Err(EvalError::Unresolved(r.to_string()));
                    }
                    _ => {}
                }
            }
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                self.push_scope();
                self.insert_binding(ident, val_id);
                let result = self.eval_comprehension_clause(clause_idx + 1, comp, target_struct);
                self.pop_scope();
                result?;
            }
            ComprehensionClause::For { key, value, source } => {
                let src_id = self.eval_expr(source)?;
                // A source that is not resolved yet has nothing to yield *yet*;
                // yielding nothing would settle the comprehension as empty.
                let src_val = match self.arena.get(src_id) {
                    Some(Value::Bottom(r)) if r.kind.may_resolve_later() => {
                        return Err(EvalError::Unresolved(r.to_string()));
                    }
                    Some(Value::RecursiveRef { name, .. }) => {
                        return Err(EvalError::Unresolved(format!("{name} not evaluated yet")));
                    }
                    Some(v) => v.clone(),
                    None => return Ok(()),
                };

                match src_val {
                    Value::List { elements, .. } => {
                        for (idx, &elem_id) in elements.iter().enumerate() {
                            self.push_scope();
                            self.insert_binding(value, elem_id);
                            if let Some(k_name) = key {
                                let k_id = self.arena.int(idx as i64);
                                self.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_comprehension_clause(clause_idx + 1, comp, target_struct);
                            self.pop_scope();
                            result?;
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in s.fields.iter().filter(|(_, e)| !e.optional) {
                            self.push_scope();
                            self.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.arena.string(k.clone());
                                self.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_comprehension_clause(clause_idx + 1, comp, target_struct);
                            self.pop_scope();
                            result?;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn eval_list_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &cue_syntax::ast::ListComprehension,
        elements: &mut Vec<ValueId>,
    ) -> Result<(), EvalError> {
        if clause_idx >= comp.clauses.len() {
            let val_id = self.eval_expr(&comp.expr)?;
            elements.push(val_id);
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => {
                let cond_val = self.eval_expr(condition)?;
                match self.arena.get(cond_val) {
                    Some(Value::Bool(true)) => {
                        self.eval_list_comprehension_clause(clause_idx + 1, comp, elements)?;
                    }
                    Some(Value::Bottom(r)) if r.kind.may_resolve_later() && self.deferring => {
                        return Err(EvalError::Unresolved(r.to_string()));
                    }
                    _ => {}
                }
            }
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                self.push_scope();
                self.insert_binding(ident, val_id);
                let result = self.eval_list_comprehension_clause(clause_idx + 1, comp, elements);
                self.pop_scope();
                result?;
            }
            ComprehensionClause::For { key, value, source } => {
                let src_id = self.eval_expr(source)?;
                // A source that is not resolved yet has nothing to yield *yet*;
                // yielding nothing would settle the comprehension as empty.
                let src_val = match self.arena.get(src_id) {
                    Some(Value::Bottom(r)) if r.kind.may_resolve_later() => {
                        return Err(EvalError::Unresolved(r.to_string()));
                    }
                    Some(Value::RecursiveRef { name, .. }) => {
                        return Err(EvalError::Unresolved(format!("{name} not evaluated yet")));
                    }
                    Some(v) => v.clone(),
                    None => return Ok(()),
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
                                self.eval_list_comprehension_clause(clause_idx + 1, comp, elements);
                            self.pop_scope();
                            result?;
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in s.fields.iter().filter(|(_, e)| !e.optional) {
                            self.push_scope();
                            self.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.arena.string(k.clone());
                                self.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_list_comprehension_clause(clause_idx + 1, comp, elements);
                            self.pop_scope();
                            result?;
                        }
                    }
                    _ => {}
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

    fn eval_expr_inner(&mut self, expr: &Expr) -> Result<ValueId, EvalError> {
        match expr {
            Expr::Bottom => Ok(self.arena.bottom("explicit bottom")),
            Expr::Top => Ok(self.arena.top()),
            Expr::Null => Ok(self.arena.null()),
            Expr::Bool(b) => Ok(self.arena.bool(*b)),
            Expr::Number(n) => self.eval_number(n),
            Expr::String(s) => Ok(self.arena.string(s.value.clone())),
            Expr::Bytes(b) => Ok(self.arena.alloc(Value::Bytes(b.value.as_bytes().to_vec()))),
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

                if let Some(val) = self.lookup_binding(id) {
                    Ok(self.read_definition(id, val))
                } else if self.declares_field(id) {
                    // The literal being evaluated declares this name, so it is
                    // not missing - it is not computed yet, which after the
                    // final pass means it never will be.
                    Ok(self
                        .arena
                        .bottom_of(BottomKind::Unresolved, "incomplete value"))
                } else {
                    Ok(self.arena.bottom_of(
                        BottomKind::ReferenceNotFound,
                        format!("reference \"{id}\" not found"),
                    ))
                }
            }
            Expr::Struct(s) => {
                let mut struct_val = StructValue::new(false);
                self.push_scope();
                let enclosing = std::mem::replace(&mut self.at_file_root, false);
                let result = self.eval_decls_into_struct(&s.decls, &mut struct_val);
                self.at_file_root = enclosing;
                self.pop_scope();
                if let Err(error) = result {
                    // Carry an incomplete nested struct to the enclosing retry
                    // loop, which may resolve its dynamic labels in a later pass.
                    return match error {
                        EvalError::Unresolved(message) => {
                            Ok(self.arena.bottom_of(BottomKind::Unresolved, message))
                        }
                        error => Err(error),
                    };
                }
                Ok(self.arena.alloc(Value::Struct(struct_val)))
            }
            Expr::List(l) => {
                let mut elements = Vec::new();
                for elem in &l.elements {
                    let elem_id = self.eval_expr(elem)?;
                    if matches!(elem, Expr::ListComp(_)) {
                        if let Some(Value::List {
                            elements: inner_elems,
                            ..
                        }) = self.arena.get(elem_id)
                        {
                            elements.extend(inner_elems.clone());
                        } else {
                            elements.push(elem_id);
                        }
                    } else {
                        elements.push(elem_id);
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
                        let merged = unify(&mut self.arena, left_id, right_id);
                        self.rederive(merged)
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
                    _ => Ok(self.eval_binary_arithmetic(*op, left_id, right_id)),
                }
            }
            Expr::Unary { op, expr } => {
                let target_id = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match self.arena.get(target_id) {
                        Some(Value::Int(i)) => Ok(self.arena.int(-i)),
                        Some(Value::Float(f)) => Ok(self.arena.float(-f)),
                        _ => Ok(self.arena.bottom("cannot negate non-numeric value")),
                    },
                    UnaryOp::Pos => Ok(target_id),
                    UnaryOp::Not => match self.arena.get(target_id) {
                        Some(Value::Bool(b)) => Ok(self.arena.bool(!b)),
                        _ => Ok(self.arena.bottom("cannot apply '!' to non-boolean value")),
                    },
                    UnaryOp::Less => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::Less, target_id)],
                    })),
                    UnaryOp::LessEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::LessEqual, target_id)],
                    })),
                    UnaryOp::Greater => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::Greater, target_id)],
                    })),
                    UnaryOp::GreaterEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::GreaterEqual, target_id)],
                    })),
                    UnaryOp::NotEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::NotEqual, target_id)],
                    })),
                    UnaryOp::RegexMatch => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::RegexMatch, target_id)],
                    })),
                    UnaryOp::RegexNotMatch => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::RegexNotMatch, target_id)],
                    })),
                    _ => Ok(target_id),
                }
            }
            Expr::Disjunction { branches } => {
                let mut eval_branches = Vec::new();
                for b in branches {
                    let val_id = self.eval_expr(&b.expr)?;
                    eval_branches.push(ValueBranch {
                        default: b.default,
                        val: val_id,
                    });
                }
                Ok(self.arena.alloc(Value::Disjunction {
                    branches: eval_branches,
                }))
            }
            Expr::Selector { expr, field } => {
                if let Expr::Ident(pkg_name) = expr.as_ref() {
                    let imports = self.imports.clone();
                    if let Some(pkg_struct_id) = imports.package_of(pkg_name)
                        && let Some(Value::Struct(s)) = self.arena.get(pkg_struct_id)
                    {
                        // A loaded package is complete: a name it lacks is the
                        // field's error, not the alias's.
                        let Some(f) = s.fields.get(field).or_else(|| s.definitions.get(field))
                        else {
                            return Ok(self.arena.bottom_of(
                                BottomKind::UndefinedField,
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
                if let Some(Value::Bottom(_)) = self.arena.get(val_id) {
                    return Ok(val_id);
                }
                // A definition read before its declaration was evaluated has no
                // fields *yet*.
                if let Some(Value::RecursiveRef { name, target: None }) = self.arena.get(val_id) {
                    let message = format!("{name} not evaluated yet");
                    return Ok(self.arena.bottom_of(BottomKind::Unresolved, message));
                }
                if let Some(Value::Struct(s)) = self.arena.get(val_id) {
                    if let Some(f) = s.fields.get(field).or_else(|| s.definitions.get(field)) {
                        let val = f.val;
                        Ok(self.read_definition(field, val))
                    } else {
                        // The base resolved and has no such field. It may still
                        // gain one on a later pass, which is why this kind is
                        // one the relaxation loop retries.
                        Ok(self.arena.bottom_of(
                            BottomKind::UndefinedField,
                            format!("undefined field: {field}"),
                        ))
                    }
                } else {
                    Ok(self.arena.bottom("selector on non-struct"))
                }
            }
            Expr::Index { expr, index } => {
                let target_id = self.eval_expr(expr)?;
                if let Some(Value::Bottom(_)) = self.arena.get(target_id) {
                    return Ok(target_id);
                }
                let index_id = self.eval_expr(index)?;
                if let Some(Value::Bottom(_)) = self.arena.get(index_id) {
                    return Ok(index_id);
                }

                match (self.arena.get(target_id), self.arena.get(index_id)) {
                    (Some(Value::List { elements, .. }), Some(Value::Int(i))) => {
                        if let Some(idx) = i.to_usize() {
                            if let Some(&elem) = elements.get(idx) {
                                Ok(elem)
                            } else {
                                Ok(self.arena.bottom(format!(
                                    "list index {idx} out of bounds (len: {})",
                                    elements.len()
                                )))
                            }
                        } else {
                            Ok(self.arena.bottom("invalid list index"))
                        }
                    }
                    (Some(Value::Struct(s)), Some(Value::String(key))) => {
                        if let Some(f) = s.fields.get(key).or_else(|| s.definitions.get(key)) {
                            Ok(f.val)
                        } else {
                            Ok(self.arena.bottom_of(
                                BottomKind::UndefinedField,
                                format!("undefined field: {key}"),
                            ))
                        }
                    }
                    _ => Ok(self.arena.bottom("indexing unsupported on target")),
                }
            }
            Expr::Slice { expr, low, high } => {
                let target_id = self.eval_expr(expr)?;
                if let Some(Value::List { elements, ellipsis }) = self.arena.get(target_id).cloned()
                {
                    let len = elements.len();
                    let start = if let Some(l) = low {
                        let l_id = self.eval_expr(l)?;
                        if let Some(Value::Int(i)) = self.arena.get(l_id) {
                            i.to_usize().unwrap_or(0).min(len)
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    let end = if let Some(h) = high {
                        let h_id = self.eval_expr(h)?;
                        if let Some(Value::Int(i)) = self.arena.get(h_id) {
                            i.to_usize().unwrap_or(len).min(len)
                        } else {
                            len
                        }
                    } else {
                        len
                    };

                    if start <= end {
                        let sliced = elements[start..end].to_vec();
                        Ok(self.arena.alloc(Value::List {
                            elements: sliced,
                            ellipsis,
                        }))
                    } else {
                        Ok(self
                            .arena
                            .bottom(format!("invalid slice range: [{start}:{end}]")))
                    }
                } else {
                    Ok(self.arena.bottom("slice unsupported on non-list"))
                }
            }
            Expr::Call { func, args } => self.eval_call(func, args),
            Expr::Interpolation { parts, .. } => {
                let mut result_str = String::new();
                for part in parts {
                    match part {
                        InterpolationPart::Lit(s) => result_str.push_str(s),
                        InterpolationPart::Expr(e) => {
                            let mut val_id = self.eval_expr(e)?;
                            while let Some(Value::Disjunction { branches }) = self.arena.get(val_id)
                            {
                                if let Some(b) = branches.iter().find(|b| b.default) {
                                    val_id = b.val;
                                } else if let Some(b) = branches.first() {
                                    val_id = b.val;
                                } else {
                                    break;
                                }
                            }
                            match self.arena.get(val_id) {
                                Some(Value::String(s)) => result_str.push_str(s),
                                Some(Value::Int(i)) => result_str.push_str(&i.to_string()),
                                Some(Value::Float(f)) => result_str.push_str(&f.to_string()),
                                Some(Value::Bool(b)) => result_str.push_str(&b.to_string()),
                                Some(Value::Bottom(_)) => return Ok(val_id),
                                other => {
                                    return Ok(self.arena.bottom(format!(
                                        "string interpolation requires concrete scalar value, got {other:?} for expr {e:?}"
                                    )));
                                }
                            }
                        }
                    }
                }
                Ok(self.arena.string(result_str))
            }
            Expr::ListComp(comp) => {
                let mut elements = Vec::new();
                match self.eval_list_comprehension_clause(0, comp, &mut elements) {
                    Err(EvalError::Unresolved(m)) => {
                        return Ok(self.arena.bottom_of(BottomKind::Unresolved, m));
                    }
                    other => other?,
                }
                Ok(self.arena.alloc(Value::List {
                    elements,
                    ellipsis: None,
                }))
            }
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

    fn eval_call(&mut self, func_expr: &Expr, arg_exprs: &[Expr]) -> Result<ValueId, EvalError> {
        let mut evaluated_args = Vec::new();
        for a in arg_exprs {
            let arg_id = self.eval_expr(a)?;
            let resolved_arg = if let Some(Value::Disjunction { branches }) = self.arena.get(arg_id)
            {
                branches
                    .iter()
                    .find(|b| b.default)
                    .map(|b| b.val)
                    .unwrap_or(arg_id)
            } else {
                arg_id
            };
            evaluated_args.push(resolved_arg);
        }

        // Top-level builtins: len(x), or(list), close(x)
        if let Expr::Ident(name) = func_expr {
            match name.as_str() {
                "len" => {
                    if let Some(&arg0) = evaluated_args.first() {
                        match self.arena.get(arg0) {
                            Some(Value::String(s)) => {
                                return Ok(self.arena.int(s.chars().count() as i64));
                            }
                            Some(Value::Bytes(b)) => return Ok(self.arena.int(b.len() as i64)),
                            Some(Value::List { elements, .. }) => {
                                return Ok(self.arena.int(elements.len() as i64));
                            }
                            // An optional field that nothing set is not there.
                            Some(Value::Struct(s)) => {
                                let set = s.fields.values().filter(|e| !e.optional).count();
                                return Ok(self.arena.int(set as i64));
                            }
                            _ => return Ok(self.arena.bottom("len: unsupported type")),
                        }
                    }
                    return Ok(self.arena.bottom("len requires 1 argument"));
                }
                "or" => {
                    if let Some(&arg0) = evaluated_args.first()
                        && let Some(Value::Bottom(_)) = self.arena.get(arg0)
                    {
                        return Ok(arg0);
                    }
                    let Some(Value::List { elements, .. }) = evaluated_args
                        .first()
                        .and_then(|&a| self.arena.get(a))
                        .cloned()
                    else {
                        return Ok(self.arena.bottom("or requires 1 list argument"));
                    };
                    // Upstream reports this as incomplete, not as a conflict: the
                    // list is usually built by a comprehension over fields that a
                    // later conjunct has yet to add.
                    if elements.is_empty() {
                        return Ok(self
                            .arena
                            .bottom_of(BottomKind::Unresolved, "empty list in call to or"));
                    }
                    let branches = elements
                        .into_iter()
                        .map(|val| ValueBranch {
                            default: false,
                            val,
                        })
                        .collect();
                    return Ok(self.arena.alloc(Value::Disjunction { branches }));
                }
                "close" => {
                    if let Some(&arg0) = evaluated_args.first()
                        && let Some(Value::Struct(s)) = self.arena.get(arg0)
                    {
                        let mut closed = s.clone();
                        closed.is_closed = true;
                        return Ok(self.arena.alloc(Value::Struct(closed)));
                    }
                    return Ok(self.arena.bottom("close requires 1 struct argument"));
                }
                _ => {}
            }
        }

        // Builtin functions dispatch: strings.*, math.*, list.*, regexp.*
        if let Expr::Selector { expr, field } = func_expr
            && let Expr::Ident(pkg) = &**expr
        {
            let imports = self.imports.clone();

            match crate::stdlib::call_stdlib_func(
                &mut self.arena,
                imports.path_of(pkg),
                field,
                &evaluated_args,
            ) {
                Ok(res_id) => return Ok(res_id),
                Err(err) => return Ok(self.arena.bottom(err)),
            }
        }

        Ok(self.arena.bottom("unsupported function call"))
    }

    fn eval_number(&mut self, n_str: &str) -> Result<ValueId, EvalError> {
        let cleaned = n_str.replace('_', "");

        // Hex, binary, octal
        if (cleaned.starts_with("0x") || cleaned.starts_with("0X"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 16)
        {
            return Ok(self.arena.int(i));
        } else if (cleaned.starts_with("0b") || cleaned.starts_with("0B"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 2)
        {
            return Ok(self.arena.int(i));
        } else if (cleaned.starts_with("0o") || cleaned.starts_with("0O"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 8)
        {
            return Ok(self.arena.int(i));
        }

        // SI multipliers
        let multiplier: Option<i64> = if cleaned.ends_with("Ki") {
            Some(1024)
        } else if cleaned.ends_with("Mi") {
            Some(1024 * 1024)
        } else if cleaned.ends_with("Gi") {
            Some(1024 * 1024 * 1024)
        } else if cleaned.ends_with("Ti") {
            Some(1024 * 1024 * 1024 * 1024)
        } else if cleaned.ends_with("Pi") {
            Some(1024 * 1024 * 1024 * 1024 * 1024)
        } else if cleaned.ends_with('k') || cleaned.ends_with('K') {
            Some(1000)
        } else if cleaned.ends_with('M') {
            Some(1_000_000)
        } else if cleaned.ends_with('G') {
            Some(1_000_000_000)
        } else if cleaned.ends_with('T') {
            Some(1_000_000_000_000)
        } else if cleaned.ends_with('P') {
            Some(1_000_000_000_000_000)
        } else {
            None
        };

        if let Some(mult) = multiplier {
            let num_part = if cleaned.ends_with("Ki")
                || cleaned.ends_with("Mi")
                || cleaned.ends_with("Gi")
                || cleaned.ends_with("Ti")
                || cleaned.ends_with("Pi")
            {
                &cleaned[..cleaned.len() - 2]
            } else {
                &cleaned[..cleaned.len() - 1]
            };
            if let Ok(base) = num_part.parse::<i64>() {
                return Ok(self.arena.int(base * mult));
            }
        }

        if (cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E'))
            && let Ok(f) = cleaned.parse::<f64>()
        {
            return Ok(self.arena.float(f));
        }
        if let Ok(i) = BigInt::from_str(&cleaned) {
            return Ok(self.arena.int(i));
        }
        Ok(self.arena.bottom(format!("invalid number '{n_str}'")))
    }

    fn eval_binary_arithmetic(&mut self, op: BinaryOp, left: ValueId, right: ValueId) -> ValueId {
        let l_val = match self.arena.get(left) {
            Some(Value::Bottom(_)) => return left,
            Some(v) => v.clone(),
            None => return self.arena.bottom("invalid left operand"),
        };
        let r_val = match self.arena.get(right) {
            Some(Value::Bottom(_)) => return right,
            Some(v) => v.clone(),
            None => return self.arena.bottom("invalid right operand"),
        };

        match (op, l_val, r_val) {
            // Arithmetic
            (BinaryOp::Add, Value::Int(a), Value::Int(b)) => self.arena.int(a + b),
            (BinaryOp::Sub, Value::Int(a), Value::Int(b)) => self.arena.int(a - b),
            (BinaryOp::Mul, Value::Int(a), Value::Int(b)) => self.arena.int(a * b),
            (BinaryOp::Div, Value::Int(a), Value::Int(b)) => {
                if b != 0.into() {
                    self.arena.int(a / b)
                } else {
                    self.arena.bottom("division by zero")
                }
            }
            (BinaryOp::Add, Value::Float(a), Value::Float(b)) => self.arena.float(a + b),
            (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => self.arena.float(a - b),
            (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => self.arena.float(a * b),
            (BinaryOp::Div, Value::Float(a), Value::Float(b)) => self.arena.float(a / b),

            // Mixed Int & Float
            (BinaryOp::Add, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) + b)
            }
            (BinaryOp::Add, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a + b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Sub, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) - b)
            }
            (BinaryOp::Sub, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a - b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Mul, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) * b)
            }
            (BinaryOp::Mul, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a * b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Div, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) / b)
            }
            (BinaryOp::Div, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a / b.to_f64().unwrap_or(1.0))
            }

            (BinaryOp::Add, Value::String(a), Value::String(b)) => {
                self.arena.string(format!("{a}{b}"))
            }
            (BinaryOp::Mul, Value::String(a), Value::Int(b)) => {
                if let Some(count) = b.to_usize() {
                    self.arena.string(a.repeat(count))
                } else {
                    self.arena.bottom("invalid string repetition factor")
                }
            }
            (BinaryOp::Mul, Value::Int(a), Value::String(b)) => {
                if let Some(count) = a.to_usize() {
                    self.arena.string(b.repeat(count))
                } else {
                    self.arena.bottom("invalid string repetition factor")
                }
            }

            // List Concatenation and Repetition
            (
                BinaryOp::Add,
                Value::List {
                    elements: mut e1,
                    ellipsis: _,
                },
                Value::List {
                    elements: e2,
                    ellipsis,
                },
            ) => {
                e1.extend(e2);
                self.arena.alloc(Value::List {
                    elements: e1,
                    ellipsis,
                })
            }
            (
                BinaryOp::Mul,
                Value::List {
                    elements: e1,
                    ellipsis,
                },
                Value::Int(b),
            ) => {
                if let Some(count) = b.to_usize() {
                    let mut repeated = Vec::new();
                    for _ in 0..count {
                        repeated.extend(e1.clone());
                    }
                    self.arena.alloc(Value::List {
                        elements: repeated,
                        ellipsis,
                    })
                } else {
                    self.arena.bottom("invalid list repetition factor")
                }
            }

            // Comparisons
            (BinaryOp::Equal, Value::Int(a), Value::Int(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a != b),
            (BinaryOp::Less, Value::Int(a), Value::Int(b)) => self.arena.bool(a < b),
            (BinaryOp::LessEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a <= b),
            (BinaryOp::Greater, Value::Int(a), Value::Int(b)) => self.arena.bool(a > b),
            (BinaryOp::GreaterEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a >= b),

            (BinaryOp::Equal, Value::Float(a), Value::Float(b)) => {
                self.arena.bool((a - b).abs() < f64::EPSILON)
            }
            (BinaryOp::NotEqual, Value::Float(a), Value::Float(b)) => {
                self.arena.bool((a - b).abs() >= f64::EPSILON)
            }
            (BinaryOp::Less, Value::Float(a), Value::Float(b)) => self.arena.bool(a < b),
            (BinaryOp::LessEqual, Value::Float(a), Value::Float(b)) => self.arena.bool(a <= b),
            (BinaryOp::Greater, Value::Float(a), Value::Float(b)) => self.arena.bool(a > b),
            (BinaryOp::GreaterEqual, Value::Float(a), Value::Float(b)) => self.arena.bool(a >= b),

            (BinaryOp::Equal, Value::String(a), Value::String(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::String(a), Value::String(b)) => self.arena.bool(a != b),
            (BinaryOp::Equal, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a != b),
            (BinaryOp::LogicalAnd, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a && b),
            (BinaryOp::LogicalOr, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a || b),

            _ => self.arena.bottom("unsupported binary operation"),
        }
    }

    /// Export evaluated value to JSON if concrete.
    pub fn to_json(&self, val_id: ValueId) -> Result<serde_json::Value, String> {
        self.to_json_at_path(val_id, "$")
    }

    /// Export evaluated value to JSON with path tracking for precise error diagnostics.
    pub fn to_json_at_path(
        &self,
        val_id: ValueId,
        path: &str,
    ) -> Result<serde_json::Value, String> {
        match self.arena.get(val_id) {
            Some(Value::Null) => Ok(serde_json::Value::Null),
            Some(Value::Bool(b)) => Ok(serde_json::Value::Bool(*b)),
            Some(Value::Int(i)) => {
                if let Some(n) = i.to_i64() {
                    Ok(serde_json::json!(n))
                } else {
                    Ok(serde_json::Value::String(i.to_string()))
                }
            }
            Some(Value::Float(f)) => Ok(serde_json::json!(f)),
            Some(Value::String(s)) => Ok(serde_json::Value::String(s.clone())),
            Some(Value::List { elements, .. }) => {
                let mut arr = Vec::new();
                for (idx, &elem) in elements.iter().enumerate() {
                    let elem_path = format!("{}[{}]", path, idx);
                    arr.push(self.to_json_at_path(elem, &elem_path)?);
                }
                Ok(serde_json::Value::Array(arr))
            }
            Some(Value::Struct(s)) => {
                let mut map = serde_json::Map::new();
                for (k, entry) in &s.fields {
                    // An optional field is a constraint on a field that may
                    // appear, not a field: `cue export` emits none of them
                    // whatever they hold, down to `a?: 1` exporting `{}`. This
                    // used to ask instead whether the constraint looked
                    // concrete, which let `b?: {x?: int}` through as `{}` and
                    // `c?: [...string]` as `[]`.
                    if entry.optional {
                        continue;
                    }
                    let field_path = if path == "$" {
                        k.clone()
                    } else {
                        format!("{}.{}", path, k)
                    };
                    map.insert(k.clone(), self.to_json_at_path(entry.val, &field_path)?);
                }
                Ok(serde_json::Value::Object(map))
            }
            Some(Value::Disjunction { branches }) => {
                if let Some(default_branch) = branches.iter().find(|b| b.default) {
                    self.to_json_at_path(default_branch.val, path)
                } else if let Some(first_branch) = branches.first() {
                    self.to_json_at_path(first_branch.val, path)
                } else if path == "$" {
                    Err("cannot export non-concrete disjunction to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export non-concrete disjunction at '{path}' to JSON"
                    ))
                }
            }
            Some(Value::Bottom(b)) => {
                if path == "$" {
                    Err(format!("cannot export bottom: {b}"))
                } else {
                    Err(format!("cannot export bottom at '{path}': {b}"))
                }
            }
            Some(Value::Top) => {
                if path == "$" {
                    Err("cannot export non-concrete top value to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export non-concrete top value at '{path}' to JSON"
                    ))
                }
            }
            Some(Value::Type(t)) => {
                if path == "$" {
                    Err(format!("cannot export type {t} to JSON"))
                } else {
                    Err(format!("cannot export type {t} at '{path}' to JSON"))
                }
            }
            Some(Value::Bounds { .. }) => {
                if path == "$" {
                    Err("cannot export bound constraint to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export bound constraint at '{path}' to JSON"
                    ))
                }
            }
            Some(Value::BuiltinValidator { .. }) => {
                if path == "$" {
                    Err("cannot export validator constraint to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export validator constraint at '{path}' to JSON"
                    ))
                }
            }
            Some(Value::Validators(_)) => {
                if path == "$" {
                    Err("cannot export validator constraints to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export validator constraints at '{path}' to JSON"
                    ))
                }
            }
            // A required field still holding the lazy node that stops a
            // recursive definition expanding is precisely a structural cycle:
            // the value is infinite. It used to be exported as the internal
            // placeholder string. An *optional* recursive field never reaches
            // here, because the struct arm above drops it first - which is what
            // upstream does with `needs?: [...#Stage]` too.
            Some(Value::RecursiveRef { name, .. }) => {
                let name = name.clone();
                if path == "$" {
                    Err(format!("structural cycle: '{name}'"))
                } else {
                    Err(format!("structural cycle at '{path}': '{name}'"))
                }
            }
            Some(Value::Bytes(b)) => Ok(serde_json::Value::String(
                String::from_utf8_lossy(b).to_string(),
            )),
            None => Err("invalid value id".to_string()),
        }
    }
}
