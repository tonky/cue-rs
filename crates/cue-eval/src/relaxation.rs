//! The relaxation loop: settling declarations order-independently.
//!
//! `RelaxationLoop` owns the declaration driver — scoped evaluation,
//! forward-reference passes, refinement allowance and nested literals.
//! It borrows the evaluator, so every value and binding operation stays on
//! `Evaluator`; what moved here is the scheduling of declarations.

use crate::closedness::{Closure, open_for_embedding, reclose};
use crate::declaration::DeclarationValue;
use crate::eval::{Clause, EvalError, Evaluator};
use crate::schedule::{
    SECTIONS, Section, Sweep, collect_field_declarations, collect_field_names, derivation_order,
    expand_field_aliases, merge_generated, pending_binding_name, refined_any, seed_moved,
    waits_only_on_external,
};
use crate::unify::{Equivalence, compare_values, push_branch, unify};
use crate::value::{
    BottomKind, Conjunct, DeclOutcome, DeclRecipe, DeclSource, DisjunctionBranch as ValueBranch,
    FieldEntry, StructValue, Thunk, ThunkEnv, Value, ValueArena, ValueId,
};
use cue_syntax::ast::*;
use std::collections::HashSet;
use std::rc::Rc;

/// How many passes of a literal may be spent refining a partial value that no
/// declaration has finished reading. Bounds the chain of self-references that
/// can resolve, and stops a structural cycle refining forever.
///
/// Tied to [`MAX_UNRESOLVED_DEPTH`](crate::value::MAX_UNRESOLVED_DEPTH) rather
/// than picked: a cycle unrolls a level or more per pass, and a partial deeper
/// than that walk's budget is *judged resolved* and written with a bottom
/// buried inside it. Eight leaves room for a cycle through four fields. A chain
/// longer than eight links therefore does not resolve, where upstream resolves
/// any length.
const MAX_REFINEMENT_PASSES: usize = crate::value::MAX_UNRESOLVED_DEPTH / 8;

/// Sweeps one merged struct may take to settle. A chain of n references needs
/// n of them, so this only stops a recipe that never settles at all.
const MAX_REDERIVE_SWEEPS: usize = 256;

/// Drives one evaluator's declarations to settled values.
///
/// Construct per entry point; nested literals reborrow through a new owner.
pub(crate) struct RelaxationLoop<'a> {
    eval: &'a mut Evaluator,
    /// Why the comprehension being run could not decide a clause, if it
    /// could not.
    incomplete: Option<String>,
    /// Set while the dynamic-label pass evaluates a literal again with its
    /// labels resolved: those labels are not literal fields, so the pass
    /// around it orders the fields instead.
    labels_resolved: bool,
}

impl<'a> RelaxationLoop<'a> {
    pub(crate) fn new(eval: &'a mut Evaluator) -> Self {
        Self {
            eval,
            incomplete: None,
            labels_resolved: false,
        }
    }

    /// Evaluate the top level of a file or package. It is never closed: a
    /// definition embedded there constrains nothing beside it.
    pub(crate) fn eval_root_decls(&mut self, decls: &[Decl]) -> Result<ValueId, EvalError> {
        let enclosing = std::mem::replace(&mut self.eval.at_file_root, true);
        let result = self
            .eval_decls(decls)
            .and_then(|value| self.finish_decls(value));
        self.eval.at_file_root = enclosing;
        result
    }

    fn eval_decls(&mut self, decls: &[Decl]) -> Result<DeclarationValue, EvalError> {
        let mut target = DeclarationValue::default();
        self.eval_decls_into(decls, &mut target)?;
        Ok(target)
    }

    pub(crate) fn finish_decls(&mut self, value: DeclarationValue) -> Result<ValueId, EvalError> {
        let value = value.finish(&mut self.eval.arena);
        if self.eval.arena.metadata(value).is_some() {
            self.rederive(value)
        } else {
            Ok(value)
        }
    }

    fn eval_decls_into(
        &mut self,
        decls: &[Decl],
        target: &mut DeclarationValue,
    ) -> Result<(), EvalError> {
        // The literal being evaluated owns `current_env` while it runs, and the
        // one it is written inside takes it back afterwards.
        let enclosing = self.eval.current_env.take();
        let saved_depth = self.eval.struct_scope_depth;
        self.eval.struct_scope_depth = self.eval.scopes.len().saturating_sub(1);
        let result = self.eval_decls_scoped(decls, target);
        self.eval.struct_scope_depth = saved_depth;
        self.eval.current_env = enclosing;
        result
    }
    fn eval_decls_scoped(
        &mut self,
        decls: &[Decl],
        target: &mut DeclarationValue,
    ) -> Result<(), EvalError> {
        let comprehension_body = std::mem::take(&mut self.eval.comprehension_body);
        let labels_resolved = std::mem::take(&mut self.labels_resolved);
        // A reference must see every declaration of a static field, including
        // declarations appearing after the reference in source order.
        let aliased = expand_field_aliases(decls).map_err(EvalError::Evaluation)?;
        let collected = collect_field_declarations(&aliased);
        let decls: &[Decl] = &collected;
        // Captured once per literal and shared by its thunks: every field of this
        // struct was written in the same lexical scope.
        let env = Rc::new(
            ThunkEnv::new(
                self.eval.scopes.clone(),
                self.collect_let_declarations(decls),
                collect_field_names(decls),
                self.eval.imports.clone(),
            )
            .with_enclosing(comprehension_body.clone()),
        );
        self.eval.current_env = Some(env.clone());
        // Reserve names that shadow an existing outer binding. Nested literals
        // must not read that outer value while this field is still uncomputed.
        // Without an outer binding a missing lookup already waits, so it needs
        // no placeholder. All reservations can share one pending value.
        let mut pending = None;
        for decl in decls {
            if let Decl::Field(field) = decl
                && !field.label.is_definition()
                && let Some(name) = field.label.ident_name()
                && !self
                    .eval
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(name))
                && self.eval.lookup_binding(name).is_some()
            {
                let val = *pending.get_or_insert_with(|| {
                    self.eval
                        .arena
                        .bottom_of(BottomKind::Unresolved, "incomplete value")
                });
                self.eval.insert_binding(name, val);
            }
        }
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
            .then(|| (target.clone(), self.eval.scopes.clone()));
        // Pass 1: Pre-register definition placeholders for recursive and forward references
        for decl in decls {
            if let Decl::Field(f) = decl
                && let Some(name) = f.label.name()
                && f.label.is_definition()
                && !self.eval.placeholders.contains_key(name)
            {
                let placeholder = self.eval.arena.alloc(Value::RecursiveRef {
                    name: name.to_string(),
                    target: None,
                });
                self.eval.insert_binding(name, placeholder);
                self.eval.placeholders.insert(name.to_string(), placeholder);
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
                .map(|decl| pending_binding_name(decl).and_then(|n| self.eval.lookup_binding(n)))
                .collect();
            let mut next_pending = Vec::new();
            let mut made_progress = false;

            for &decl in &pending_decls {
                let resolved = self.eval_single_decl(decl, target, &env, false)?;
                if resolved {
                    made_progress = true;
                } else {
                    next_pending.push(decl);
                }
            }

            if !made_progress {
                let refining = refined_any(
                    &self.eval.arena,
                    &|name| self.eval.lookup_binding(name),
                    &pending_decls,
                    &before,
                );
                // Refinement credit earned only by waits its own passes cannot
                // settle is illusory: re-running the same declarations with
                // the same inputs derives the same stuck values. Skip that
                // pass and go straight to the final below — which still runs,
                // since final-mode derivation heals spurious partial-meet
                // errors that correctness relies on — without consuming the
                // allowance a genuinely growing value needs.
                if refining && !next_pending.is_empty() {
                    let lets = self.collect_let_declarations(decls);
                    let mut internal: HashSet<String> = collect_field_names(decls);
                    internal.extend(lets.iter().map(|(name, _)| name.clone()));
                    if next_pending.iter().all(|decl| {
                        waits_only_on_external(
                            &self.eval.arena,
                            &|name| self.eval.lookup_binding(name),
                            decl,
                            &internal,
                            &lets,
                        )
                    }) {
                        // Fall through to the final pass.
                        for &decl in &next_pending {
                            self.eval_single_decl(decl, target, &env, true)?;
                        }
                        break;
                    }
                }
                if refining && refinements < MAX_REFINEMENT_PASSES {
                    refinements += 1;
                    // Cumulative only; the allowance above stays per-literal.
                    self.eval.refinements += 1;
                    pending_decls = next_pending;
                    continue;
                }
                if refining {
                    // Still growing with the allowance spent: the value does not
                    // converge. Drop the partials so the final pass reports the
                    // reference at the link it is written on rather than at the
                    // bottom of however many levels were unrolled getting here.
                    for &decl in &next_pending {
                        if let Some(name) = pending_binding_name(decl) {
                            self.eval.remove_binding(name);
                        }
                    }
                }
                // Saturated / cannot resolve further; final evaluation accepts bottom errors
                for &decl in &next_pending {
                    self.eval_single_decl(decl, target, &env, true)?;
                }
                break;
            }

            pending_decls = next_pending;
        }

        // Once computed labels are known, collect their declarations with the
        // static fields and evaluate references again from the original scope.
        // Otherwise an earlier reference can retain a pre-merge snapshot.
        if let Some((base_struct, base_scopes)) = dynamic_base {
            let mut named_decls = decls.to_vec();
            let mut undecided = Vec::new();
            for decl in &mut named_decls {
                if let Decl::Field(field) = decl
                    && let Label::Dynamic(expr) = &field.label
                {
                    let defaults = self.eval.arena.defaults_chosen();
                    match self.eval.dynamic_label(expr)? {
                        // A label read through a default names a field a merge
                        // may rename. It is generated the way a comprehension
                        // generates one, which a merge derives again.
                        Clause::Ready(_)
                            if comprehension_body.is_none()
                                && self.eval.arena.defaults_chosen() != defaults =>
                        {
                            *decl = Decl::Comprehension(ComprehensionDecl {
                                clauses: vec![ComprehensionClause::If {
                                    condition: Expr::Bool(true),
                                }],
                                struct_lit: StructLit {
                                    decls: vec![Decl::Field(field.clone())],
                                    form: Default::default(),
                                },
                            });
                        }
                        Clause::Ready(name) => field.label = Label::String(name),
                        // A label that is not concrete yet waits on the struct
                        // for a merge to supply it, instead of being dropped.
                        Clause::Incomplete(_) => undecided.push(Rc::new(field.clone())),
                    }
                }
            }
            named_decls.retain(|decl| {
                !matches!(
                    decl,
                    Decl::Field(FieldDecl {
                        label: Label::Dynamic(_),
                        ..
                    })
                )
            });
            *target = base_struct;
            self.eval.scopes = base_scopes;
            self.labels_resolved = true;
            // Still the same body, declaring into the same literal around it.
            self.eval.comprehension_body = comprehension_body;
            self.eval_decls_into(&named_decls, target)?;
            for field in undecided {
                let own = self.run_field(&field)?;
                merge_generated(&mut self.eval.arena, &mut target.structure, own.structure);
            }
            put_literal_fields_first(target, decls);
            return Ok(());
        }

        // A comprehension, an embedding or a pattern constraint can change a field
        // after another field has already read it. Those are the paths whose
        // readers need deriving again; an ordinary literal is derived again by
        // the merge that changed it.
        let needs_rederive = !target.structure.pattern_constraints.is_empty()
            || decls
                .iter()
                .any(|d| matches!(d, Decl::Comprehension(_) | Decl::Embedding(_)));

        // Apply pattern constraints to matching fields
        let pattern_constraints = target.structure.pattern_constraints.clone();
        for pc in pattern_constraints {
            let field_names: Vec<String> = target.structure.fields.keys().cloned().collect();
            for field_name in field_names {
                let name_id = self.eval.arena.string(field_name.as_str());
                let match_res = crate::unify::unify(&mut self.eval.arena, pc.pattern_val, name_id);
                if !matches!(self.eval.arena.get(match_res), Some(Value::Bottom(_)))
                    && let Some(entry) = target.structure.fields.get_mut(&field_name)
                {
                    let new_val =
                        crate::unify::unify(&mut self.eval.arena, entry.val, pc.target_val);
                    entry.val = new_val;
                }
            }
        }

        if needs_rederive {
            let mut visiting = HashSet::new();
            self.rederive_struct(&mut target.structure, &mut visiting)?;
        }
        if !labels_resolved {
            put_literal_fields_first(target, decls);
        }

        Ok(())
    }

    /// `let` and alias declarations of one literal, in declaration order. They are
    /// scope bindings rather than fields, so a thunk cannot read them back from
    /// the struct it is forced against and has to derive them again.
    fn collect_let_declarations(&mut self, decls: &[Decl]) -> Vec<(String, Rc<Expr>)> {
        decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Let { ident, expr } | Decl::Alias { ident, expr } => {
                    Some((ident.clone(), self.eval.expressions.intern(expr)))
                }
                _ => None,
            })
            .collect()
    }
    fn eval_single_decl(
        &mut self,
        decl: &Decl,
        target: &mut DeclarationValue,
        env: &Rc<ThunkEnv>,
        final_pass: bool,
    ) -> Result<bool, EvalError> {
        // A literal's final pass settles only its own declarations; a loop
        // around it that still retries may yet bind what they read.
        let enclosing = self.eval.deferring;
        self.eval.deferring = enclosing || !final_pass;
        let result = self.eval_decl_pass(decl, target, env, final_pass);
        self.eval.deferring = enclosing;
        result
    }

    /// The first required field an embedding would smuggle into a literal
    /// that is already closing: present in the embedded struct, declared
    /// neither by the literal nor by one of its patterns. The first
    /// embedding into an empty literal has nothing to violate.
    fn embedded_foreign_field(
        arena: &ValueArena,
        current: &StructValue,
        embedded_id: ValueId,
    ) -> Option<String> {
        if current.fields.is_empty() {
            return None;
        }
        let embedded = arena.fields(embedded_id)?;
        embedded
            .fields
            .iter()
            .filter(|(_, entry)| !entry.optional)
            .map(|(name, _)| name.as_str())
            .find(|name| {
                !current.fields.contains_key(*name)
                    && !current
                        .pattern_constraints
                        .iter()
                        .any(|pc| crate::unify::field_matches_pattern(arena, pc.pattern_val, name))
            })
            .map(str::to_string)
    }

    fn eval_decl_pass(
        &mut self,
        decl: &Decl,
        target: &mut DeclarationValue,
        env: &Rc<ThunkEnv>,
        final_pass: bool,
    ) -> Result<bool, EvalError> {
        match decl {
            Decl::Field(f) => match &f.label {
                Label::Pattern(pattern_expr) => {
                    let pattern_val = self.eval.eval_expr(pattern_expr)?;
                    let target_val = self.eval.eval_expr(&f.value)?;
                    let is_unresolved = self.eval.arena.is_unresolved(pattern_val)
                        || self.eval.arena.is_unresolved(target_val);
                    if is_unresolved && !final_pass {
                        return Ok(false);
                    }
                    target
                        .structure
                        .add_pattern_constraint(pattern_val, target_val);
                    Ok(!is_unresolved)
                }
                Label::Dynamic(dyn_expr) => {
                    let label = match self.eval.dynamic_label(dyn_expr) {
                        Ok(label) => label,
                        Err(EvalError::Unresolved(message)) => {
                            if final_pass {
                                return Err(EvalError::Unresolved(format!(
                                    "unresolved reference in dynamic field label: {message}"
                                )));
                            }
                            return Ok(false);
                        }
                        Err(error) => return Err(error),
                    };
                    // An undecided label is recorded once the labels are
                    // normalized, below the declaration loop.
                    if let Clause::Ready(name) = label {
                        let saved_field = self.eval.current_field.replace(name.clone());
                        let res = self.eval.eval_field_value(&f.value, env);
                        self.eval.current_field = saved_field;
                        let (val_id, conjunct) = res?;
                        let is_unresolved = self.eval.arena.is_unresolved(val_id);
                        if is_unresolved && !final_pass {
                            return Ok(false);
                        }
                        let val_id = self.eval.unify_decl_field(
                            &mut target.structure.fields,
                            &name,
                            val_id,
                            f.optional,
                            conjunct,
                        )?;
                        // A dynamic label declares no name a reference reads,
                        // but it adds to a field an identifier label declared.
                        if self.eval.declares_field(&name) {
                            self.eval.insert_binding(&name, val_id);
                        }
                        Ok(!is_unresolved)
                    } else {
                        Ok(true)
                    }
                }
                _ => {
                    let saved_field = self.eval.current_field.clone();
                    if let Some(name) = f.label.name() {
                        self.eval.current_field = Some(name.to_string());
                    }
                    // A definition's body decides its selects: a template
                    // cannot grow a field on a later pass the way an open
                    // value may. Nested literals inherit the flag; the
                    // save/restore keeps sibling fields unaffected.
                    let saved_in_definition = self.eval.in_definition;
                    if f.label.is_definition() {
                        self.eval.in_definition = true;
                    }
                    let res = self.eval.eval_field_value(&f.value, env);
                    self.eval.in_definition = saved_in_definition;
                    self.eval.current_field = saved_field;
                    let (val_id, conjunct) = res?;
                    // A definition read before its declaration was evaluated
                    // absorbs whatever it is unified with: it is no value yet.
                    // Only at the top: inside a value it is how a recursive
                    // definition refers to itself.
                    let placeholder = self.eval.arena.is_placeholder(val_id);
                    let is_unresolved = placeholder || self.eval.arena.is_unresolved(val_id);
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
                            && self.eval.declares_field(name)
                            && !f.label.is_definition()
                            && !f.label.is_hidden()
                            && !placeholder
                        {
                            let partial = match target.structure.fields.get(name) {
                                Some(entry) => unify(&mut self.eval.arena, entry.val, val_id),
                                None => val_id,
                            };
                            self.eval.insert_binding(name, partial);
                        }
                        return Ok(false);
                    }
                    if let Some(name) = f.label.name() {
                        let fields = if f.label.is_definition() {
                            &mut target.structure.definitions
                        } else if f.label.is_hidden() {
                            &mut target.structure.hidden
                        } else {
                            &mut target.structure.fields
                        };
                        let val_id = self
                            .eval
                            .unify_decl_field(fields, name, val_id, f.optional, conjunct)?;
                        if f.label.is_definition()
                            && let Some(&placeholder_id) = self.eval.placeholders.get(name)
                        {
                            // A recursive reference reads the definition, so it
                            // meets the closed value like any other reader.
                            let closed = self.eval.closed.close(&mut self.eval.arena, val_id);
                            if let Some(Value::RecursiveRef { target, .. }) =
                                self.eval.arena.get_mut(placeholder_id)
                            {
                                *target = Some(closed);
                            }
                        }
                        // Bound only under a name an identifier label declared
                        // here; `"a": 1` alone leaves `a` to the enclosing scope.
                        if self.eval.declares_field(name) {
                            self.eval.insert_binding(name, val_id);
                        }
                    }
                    Ok(!is_unresolved)
                }
            },
            Decl::Alias { ident, expr } | Decl::Let { ident, expr } => {
                // The binding is part of the literal's environment, so a field
                // that reads it derives it again beside itself.
                let val_id = self.eval.eval_expr(expr)?;
                let is_unresolved = self.eval.arena.is_unresolved(val_id);
                self.eval.insert_binding(ident, val_id);
                Ok(!is_unresolved)
            }
            Decl::Embedding(expr) => {
                let enclosing = self.eval.reading_root_embedding;
                self.eval.reading_root_embedding |= self.eval.at_file_root;
                let embedded = self.eval.eval_expr(expr);
                self.eval.reading_root_embedding = enclosing;
                let embedded_id = embedded?;
                let is_unresolved = self.eval.arena.is_placeholder(embedded_id)
                    || self.eval.arena.is_unresolved(embedded_id);
                if is_unresolved && !final_pass {
                    return Ok(false);
                }
                // A literal that reached its final pass may still be inside an
                // outer retry loop. An unbound definition is pending input,
                // not a recursive value that can replace this literal's fields.
                let embedded_id = if self.eval.arena.is_placeholder(embedded_id) {
                    self.eval.arena.bottom_of(
                        BottomKind::Unresolved,
                        "embedded definition not evaluated yet",
                    )
                } else {
                    embedded_id
                };
                let (embedded_id, was_closed) =
                    open_for_embedding(&mut self.eval.arena, embedded_id);
                if matches!(self.eval.arena.get(embedded_id), Some(Value::Struct(_)))
                    && !self.eval.arena.has_embedded_recipe(embedded_id)
                {
                    target.has_struct_embedding = true;
                    // Embedding into a literal that is already closing (closed
                    // by an earlier embedding, or closing on finish) must not
                    // smuggle in fields the literal does not declare:
                    // upstream reports `field not allowed` at the foreign
                    // field. Spread opens the literal, so neither it nor
                    // anything after it is checked; the first embedding into
                    // an empty literal has nothing to violate. A literal's
                    // own field declarations are never checked here: the
                    // single-file oracle accepts them after value embeddings
                    // (`{Old1, c: 3}` exports); only package-boundary
                    // unification rejects those, which cue-rs does not model.
                    let embed_violation = if target.structure.is_closed || target.close_on_finish {
                        let spread_open = target.structure.is_open
                            || target.structure.spread_open
                            || matches!(expr, Expr::Spread { .. });
                        if spread_open {
                            None
                        } else {
                            Self::embedded_foreign_field(
                                &self.eval.arena,
                                &target.structure,
                                embedded_id,
                            )
                        }
                    } else {
                        None
                    };
                    let current_id = self
                        .eval
                        .arena
                        .alloc(Value::Struct(Box::new(target.structure.clone())));
                    let unified_id = unify(&mut self.eval.arena, current_id, embedded_id);
                    let unified_id = if was_closed && !self.eval.at_file_root {
                        reclose(&mut self.eval.arena, unified_id)
                    } else {
                        unified_id
                    };
                    let unified_id = match embed_violation {
                        Some(name) => {
                            let not_allowed = self
                                .eval
                                .arena
                                .bottom_of(BottomKind::Conflict, "field not allowed");
                            self.eval.arena.bottom_at(&name, not_allowed)
                        }
                        None => unified_id,
                    };
                    if let Some(Value::Struct(s)) = self.eval.arena.get(unified_id) {
                        target.structure = s.as_ref().clone();
                        self.eval.bind_struct_fields(&target.structure);
                    } else {
                        target.constrain(&mut self.eval.arena, unified_id);
                    }
                } else {
                    if let Some(metadata) = self.eval.arena.metadata(embedded_id).cloned() {
                        let current = self
                            .eval
                            .arena
                            .alloc(Value::Struct(Box::new(target.structure.clone())));
                        let fields = unify(&mut self.eval.arena, current, metadata.fields);
                        if let Some(Value::Struct(fields)) = self.eval.arena.get(fields) {
                            target.structure = fields.as_ref().clone();
                            self.eval.bind_struct_fields(&target.structure);
                            let payload =
                                crate::metadata::payload(&mut self.eval.arena, embedded_id);
                            let conjuncts = if matches!(
                                metadata.source,
                                crate::value::MetadataSource::Closed { .. }
                            ) {
                                vec![Conjunct::Value(embedded_id)]
                            } else {
                                metadata
                                    .conjuncts()
                                    .map(<[_]>::to_vec)
                                    .unwrap_or_else(|| vec![Conjunct::Value(payload)])
                            };
                            target.constrain_with(&mut self.eval.arena, payload, conjuncts);
                        } else {
                            target.constrain(&mut self.eval.arena, fields);
                        }
                    } else {
                        let conjunct = self.eval.field_conjunct(expr, env, embedded_id);
                        target.constrain_with(&mut self.eval.arena, embedded_id, vec![conjunct]);
                    }
                    target.close_on_finish |= was_closed && !self.eval.at_file_root;
                }
                Ok(!is_unresolved)
            }
            Decl::Ellipsis(_) => {
                target.structure.is_open = true;
                Ok(true)
            }
            Decl::Comprehension(comp) => {
                // A comprehension that waits on a reference yields nothing yet:
                // the fields it generated before it stopped are dropped with it.
                let mut scratch = target.clone();
                let (comp, deps) = self.eval.intern_comprehension(comp);
                let own = match self.run_comprehension(&comp, deps) {
                    Err(EvalError::Unresolved(_)) if !final_pass => return Ok(false),
                    other => other?,
                };
                scratch.merge_constraints(&mut self.eval.arena, &own, self.eval.at_file_root);
                let mut body_labels = Vec::new();
                crate::arc_order::decl_literal_labels(&comp.struct_lit.decls, &mut body_labels);
                for section in SECTIONS {
                    let existing = section.map(&scratch.structure);
                    let added = section.map(&own.structure).keys().filter(|name| {
                        !existing.contains_key(*name) && !body_labels.contains(*name)
                    });
                    scratch
                        .late_fields
                        .extend(added.cloned().collect::<Vec<_>>());
                }
                merge_generated(&mut self.eval.arena, &mut scratch.structure, own.structure);
                if !final_pass
                    && scratch
                        .constraint()
                        .is_some_and(|value| self.eval.arena.is_unresolved(value))
                {
                    return Ok(false);
                }
                *target = scratch;
                // Loop-local bindings have been popped; subsequent references
                // must resolve to the final generated field values.
                self.eval.bind_struct_fields(&target.structure);
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// A literal owns a frame distinct from comprehension variables and from
    /// the field whose value contains it. Restore that context even on error.
    pub(crate) fn eval_nested_decls(
        &mut self,
        decls: &[Decl],
    ) -> Result<DeclarationValue, EvalError> {
        self.eval.push_scope();
        let enclosing = std::mem::replace(&mut self.eval.at_file_root, false);
        let saved_field = self.eval.current_field.take();
        let result = self.eval_decls(decls);
        self.eval.current_field = saved_field;
        self.eval.at_file_root = enclosing;
        self.eval.pop_scope();
        result
    }

    /// Derive again the fields of a struct the evaluator has just merged.
    ///
    /// A field beside the one the merge changed was evaluated against the value
    /// that field had before, so its recipe runs again here, against the merged
    /// struct, and replaces what it produced the first time. Replacement rather
    /// than unification: the cached value came from the same recipe, and
    /// `"v5" & "v9"` is bottom.
    pub(crate) fn rederive(&mut self, id: ValueId) -> Result<ValueId, EvalError> {
        let mut visiting = HashSet::new();
        self.rederive_value(id, &mut visiting)
    }

    /// Unify two values in this evaluator's arena and re-derive any reader fields affected by the merge.
    pub(crate) fn unify_and_rederive(
        &mut self,
        v1: ValueId,
        v2: ValueId,
    ) -> Result<ValueId, EvalError> {
        let merged = unify(&mut self.eval.arena, v1, v2);
        self.rederive(merged)
    }

    fn rederive_value(
        &mut self,
        id: ValueId,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<ValueId, EvalError> {
        if let Some(Value::Disjunction { branches }) = self.eval.arena.get(id)
            && !self.eval.arena.has_embedded_recipe(id)
            && (self
                .eval
                .arena
                .metadata(id)
                .is_some_and(|metadata| metadata.is_choice_view())
                || branches.iter().any(|branch| {
                    self.eval.arena.metadata(branch.val).is_some()
                        // A branch holding a field on credit settles it as a
                        // merged struct does (`#A & ({b: 3} | {})`).
                        || matches!(
                            self.eval.arena.get(branch.val),
                            Some(Value::Struct(s)) if !s.provisional.is_empty()
                        )
                }))
        {
            if !visiting.insert(id) {
                return Ok(id);
            }
            let result = self.rederive_metadata_choices(id, branches.clone(), visiting);
            visiting.remove(&id);
            return result;
        }
        if let Some(metadata) = self.eval.arena.embedded_metadata(id).cloned() {
            if !visiting.insert(id) {
                return Ok(id);
            }
            let result = self.rederive_metadata(id, metadata, visiting);
            visiting.remove(&id);
            return result;
        }
        // A list's elements are merged one by one (`[...#Svc] & [{port: 9}]`),
        // and each settles as a merged struct does.
        if let Some(Value::List { elements, ellipsis }) = self.eval.arena.get(id).cloned() {
            if !visiting.insert(id) {
                return Ok(id);
            }
            let mut derived = Vec::with_capacity(elements.len());
            for &element in &elements {
                match self.rederive_value(element, visiting) {
                    // An element derived again to the same value keeps its id,
                    // or the list would count as moved on every pass.
                    Ok(new)
                        if new != element
                            && crate::unify::compare_values(&self.eval.arena, element, new)
                                == crate::unify::Equivalence::Equal =>
                    {
                        derived.push(element)
                    }
                    Ok(element) => derived.push(element),
                    Err(error) => {
                        visiting.remove(&id);
                        return Err(error);
                    }
                }
            }
            visiting.remove(&id);
            return Ok(if derived == elements {
                id
            } else {
                self.eval.arena.alloc(Value::List {
                    elements: derived,
                    ellipsis,
                })
            });
        }
        let Some(Value::Struct(s)) = self.eval.arena.get(id) else {
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
            self.eval.arena.alloc(Value::Struct(s))
        } else {
            id
        })
    }

    fn rederive_metadata_choices(
        &mut self,
        id: ValueId,
        branches: Vec<ValueBranch>,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<ValueId, EvalError> {
        let mut kept = Vec::new();
        let mut errors = Vec::new();
        let mut changed = false;
        for branch in branches {
            let value = self.rederive_value(branch.val, visiting)?;
            changed |= value != branch.val;
            if let Some(Value::Bottom(reason)) = self.eval.arena.get(value)
                && !reason.kind.may_resolve_later()
                && reason.kind != BottomKind::Incomplete
            {
                errors.push(reason.clone());
                changed = true;
                continue;
            }
            push_branch(
                &self.eval.arena,
                &mut kept,
                ValueBranch {
                    val: value,
                    ..branch
                },
            );
        }
        let mut view = self
            .eval
            .arena
            .metadata(id)
            .filter(|metadata| metadata.is_choice_view())
            .cloned();
        if let Some(metadata) = &mut view {
            let fields = self.rederive_value(metadata.fields, visiting)?;
            changed |= fields != metadata.fields;
            metadata.fields = fields;
            if !matches!(self.eval.arena.get(fields), Some(Value::Struct(_))) {
                return Ok(fields);
            }
        }
        Ok(if changed {
            let result = crate::unify::settle_disjunction(&mut self.eval.arena, kept, &errors);
            if let Some(metadata) = view
                && let Some(value @ Value::Disjunction { .. }) =
                    self.eval.arena.get(result).cloned()
            {
                self.eval.arena.alloc_with_metadata(value, metadata)
            } else {
                result
            }
        } else {
            id
        })
    }

    fn rederive_metadata(
        &mut self,
        id: ValueId,
        metadata: crate::value::ValueMetadata,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<ValueId, EvalError> {
        let fields = self.rederive_value(metadata.fields, visiting)?;
        let Some(Value::Struct(body)) = self.eval.arena.get(fields).cloned() else {
            return Ok(fields);
        };
        let Some(value) = self.derive_metadata_payload(&metadata, fields, &body, visiting)? else {
            return Ok(id);
        };
        let view = self
            .eval
            .arena
            .metadata(value)
            .map(|m| m.view.unwrap_or(m.fields));
        let same_view = view.is_none_or(|view| {
            compare_values(
                &self.eval.arena,
                metadata.view.unwrap_or(metadata.fields),
                view,
            ) == Equivalence::Equal
        });
        if fields == metadata.fields
            && same_view
            && crate::unify::same_payload(&self.eval.arena, id, value)
        {
            return Ok(id);
        }
        let value = self
            .eval
            .arena
            .get(value)
            .expect("derived payload exists")
            .clone();
        Ok(self.eval.arena.alloc_with_metadata(
            value,
            crate::value::ValueMetadata {
                fields,
                view,
                ..metadata
            },
        ))
    }

    fn derive_metadata_payload(
        &mut self,
        metadata: &crate::value::ValueMetadata,
        fields: ValueId,
        body: &StructValue,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<Option<ValueId>, EvalError> {
        let mut value = None;
        for conjunct in metadata
            .conjuncts()
            .expect("embedded metadata has conjuncts")
        {
            let next = match conjunct {
                Conjunct::Value(id) | Conjunct::Closed(id) => {
                    if let Some(group) = self.eval.arena.metadata(*id).cloned()
                        && matches!(group.source, crate::value::MetadataSource::Closed { .. })
                    {
                        match self.derive_closed_group(*id, &group, body, visiting)? {
                            Some(value) => value,
                            None => return Ok(None),
                        }
                    } else {
                        *id
                    }
                }
                Conjunct::Thunk(thunk) => match self.derive_thunk(thunk, body)? {
                    // This thunk came from an embedding declaration. Reapply
                    // the same outer opening used on its first evaluation;
                    // the receiving literal closes after adding its fields.
                    Some(value) => open_for_embedding(&mut self.eval.arena, value).0,
                    None => return Ok(None),
                },
            };
            value = Some(match value {
                Some(previous) => unify(&mut self.eval.arena, previous, next),
                None => next,
            });
        }
        let Some(value) = value else { return Ok(None) };
        let value = crate::metadata::materialize(
            &mut self.eval.arena,
            value,
            fields,
            &mut crate::unify::UnifyContext::new(),
        );
        Ok(Some(match metadata.source {
            crate::value::MetadataSource::Closed { closure, .. } => {
                closure.apply(&mut self.eval.arena, value)
            }
            _ => value,
        }))
    }

    fn derive_closed_group(
        &mut self,
        id: ValueId,
        group: &crate::value::ValueMetadata,
        body: &StructValue,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<Option<ValueId>, EvalError> {
        if visiting.len() >= crate::unify::MAX_TOTAL_DEPTH || !visiting.insert(id) {
            return Ok(Some(
                self.eval
                    .arena
                    .bottom("cycle error: embedded recipe recursion limit exceeded"),
            ));
        }
        let result = (|| {
            let inputs = crate::metadata::project_inputs(&mut self.eval.arena, group.fields, body);
            let inputs = self.rederive_value(inputs, visiting)?;
            let Some(Value::Struct(inputs_body)) = self.eval.arena.get(inputs).cloned() else {
                return Ok(Some(inputs));
            };
            self.derive_metadata_payload(group, inputs, &inputs_body, visiting)
        })();
        visiting.remove(&id);
        result
    }

    pub(crate) fn rederive_struct(
        &mut self,
        s: &mut StructValue,
        visiting: &mut HashSet<ValueId>,
    ) -> Result<bool, EvalError> {
        // Nothing merged into this struct, so no recipe's inputs moved: a name
        // the literal does not declare resolves in the scope it was written in,
        // which no merge can change.
        // A field the merge gave a second recipe to is one whose value it may
        // have moved; everything else in the struct is exactly what it was.
        // Pattern-matched fields join them (a pattern applies without
        // appearing among conjuncts); unmatched fields cannot have changed.
        let mut moved: HashSet<String> = seed_moved(&self.eval.arena, s);
        if moved.is_empty() {
            return Ok(false);
        }

        let mut order = derivation_order(s);
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
            // A declaration the struct could not decide before reads what the
            // merge moved too. What it generates now moves in turn.
            let mut next = sweep.moved;
            if !s.recipes.is_empty() {
                let mut read = moved.clone();
                read.extend(next.iter().cloned());
                let regenerated = self.derive_recipes(s, &read)?;
                if let Some(regenerated) = regenerated {
                    order = derivation_order(s);
                    changed = true;
                    next.extend(regenerated);
                }
            }
            moved = next;
            if moved.is_empty() {
                settled = true;
                break;
            }
        }
        if !settled {
            self.eval.unsettled += 1;
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
                // A pattern meets the fields it matches without leaving a
                // conjunct. What it merged holds a field on credit only if a
                // closed value met one it does not declare, and only a
                // comprehension the pattern brought can settle it, so only
                // then does it settle below as any merge does. Descending into
                // every matched field would derive nested patterns again on
                // every pass.
                if entry.conjuncts.len() < 2
                    && (!s.pattern_constraints.iter().any(|pc| {
                        crate::closedness::holds_recipe(&self.eval.arena, pc.target_val)
                            && crate::unify::field_matches_pattern(
                                &self.eval.arena,
                                pc.pattern_val,
                                &name,
                            )
                    }) || crate::closedness::unvouched_field(&self.eval.arena, entry.val)
                        .is_none())
                {
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
            let verdict = compare_values(&self.eval.arena, val, entry.val);
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
        self.eval.derivations += 1;
        let saved_field = self.eval.current_field.replace(name.to_string());
        let result = self.derive_field_value(entry, s, name);
        self.eval.current_field = saved_field;
        result
    }

    fn derive_field_value(
        &mut self,
        entry: &FieldEntry,
        s: &StructValue,
        name: &str,
    ) -> Result<Option<ValueId>, EvalError> {
        // A definition's contributions derive together and close once: it is
        // closed over all of its declarations of the field. What the merge
        // added meets that closed value after.
        let mut definition: Option<ValueId> = None;
        let mut rest: Option<ValueId> = None;
        for conjunct in entry.conjuncts.iter() {
            let conjunct_val = match conjunct {
                Conjunct::Value(val) | Conjunct::Closed(val) => *val,
                Conjunct::Thunk(thunk) => match self.derive_thunk(thunk, s)? {
                    Some(derived) => derived,
                    None => return Ok(None),
                },
            };
            let group = if conjunct.closes() {
                &mut definition
            } else {
                &mut rest
            };
            *group = Some(match *group {
                None => conjunct_val,
                Some(previous) => unify(&mut self.eval.arena, previous, conjunct_val),
            });
        }
        let definition =
            definition.map(|definition| Closure::Recursive.apply(&mut self.eval.arena, definition));
        let mut val = match (definition, rest) {
            (Some(definition), Some(rest)) => unify(&mut self.eval.arena, definition, rest),
            (Some(val), None) | (None, Some(val)) => val,
            (None, None) => return Ok(None),
        };

        // Pattern constraints are part of the field's value, not of its recipe.
        for pc in s.pattern_constraints.clone() {
            if crate::unify::field_matches_pattern(&self.eval.arena, pc.pattern_val, name) {
                val = unify(&mut self.eval.arena, val, pc.target_val);
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
        let saved_scopes = std::mem::replace(&mut self.eval.scopes, thunk.env.scopes.clone());
        let saved_env = self.eval.current_env.replace(thunk.env.clone());
        // The recipe may have been written in another file, and `pkg.Name` means
        // what `pkg` names there.
        let saved_imports = std::mem::replace(&mut self.eval.imports, thunk.env.imports.clone());
        let saved_depth = self.eval.struct_scope_depth;

        let mut result = self.rebind_enclosing(&thunk.env, s).map(|()| None);
        self.eval.push_scope();
        self.eval.struct_scope_depth = self.eval.scopes.len().saturating_sub(1);
        for section in SECTIONS {
            for (name, entry) in section.map(s) {
                if thunk.env.owns_field(name) {
                    self.eval.insert_binding(name, entry.val);
                }
            }
        }

        // A binding is a recipe too, and reads the merged values beside it.
        for (name, expr) in thunk.env.lets.clone() {
            if result.is_err() {
                break;
            }
            match self.eval.eval_expr(&expr) {
                // A binding that cannot be derived here keeps the value it was
                // captured with; deriving again may only improve it.
                Ok(val) if !self.eval.arena.is_unresolved(val) => {
                    self.eval.insert_binding(&name, val)
                }
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
            result = self.eval.eval_expr(&thunk.expr).map(|val| {
                (self.eval.deferring || !self.eval.arena.is_unresolved(val)).then_some(val)
            });
        }

        self.eval.scopes = saved_scopes;
        self.eval.current_env = saved_env;
        self.eval.imports = saved_imports;
        self.eval.struct_scope_depth = saved_depth;
        result
    }

    /// In a comprehension body's scope stack, bind the names each literal
    /// around the body declares to their values in the merged struct, and
    /// derive those literals' `let` bindings again, each in its own frame.
    ///
    /// The body declares into the struct its comprehension is written in, so
    /// a name it reads from there - `x: a` in `{a: int | *0, if c {x: a}}` -
    /// means the merged `a`, not the one captured when the body first ran. A
    /// guard or `for` source that changes reruns the comprehension instead;
    /// this is what keeps a body right when the merge leaves those alone.
    fn rebind_enclosing(&mut self, env: &ThunkEnv, s: &StructValue) -> Result<(), EvalError> {
        if env.enclosing.is_none() {
            return Ok(());
        }
        let mut chain: Vec<_> = env.enclosing_literals().cloned().collect();
        // Outermost first: a literal's `let` may read the literal around it.
        chain.reverse();
        let saved_env = self.eval.current_env.clone();
        let saved_depth = self.eval.struct_scope_depth;
        let mut result = Ok(());
        for outer in chain {
            if outer.frame >= self.eval.scopes.len() {
                continue;
            }
            // The frames above this literal's own (comprehension clauses, the
            // bodies inside it) are out of reach of its names and `let`s.
            let inner = self.eval.scopes.split_off(outer.frame + 1);
            self.eval.current_env = Some(outer.env.clone());
            self.eval.struct_scope_depth = outer.frame;
            for name in outer.env.own_fields.borrow().iter() {
                if let Some(entry) = SECTIONS.iter().find_map(|section| section.map(s).get(name)) {
                    self.eval.insert_binding(name, entry.val);
                }
            }
            for (name, expr) in &outer.env.lets {
                match self.eval.eval_expr(expr) {
                    // As in `derive_thunk`: one that cannot be derived here
                    // keeps the value it was captured with.
                    Ok(val) if !self.eval.arena.is_unresolved(val) => {
                        self.eval.insert_binding(name, val)
                    }
                    Ok(_) => {}
                    Err(error) => result = Err(error),
                }
                if result.is_err() {
                    break;
                }
            }
            self.eval.scopes.extend(inner);
            if result.is_err() {
                break;
            }
        }
        self.eval.current_env = saved_env;
        self.eval.struct_scope_depth = saved_depth;
        result
    }

    pub(crate) fn eval_comprehension(
        &mut self,
        comp: &ComprehensionDecl,
        target: &mut DeclarationValue,
    ) -> Result<(), EvalError> {
        if comp.clauses.is_empty() {
            return Ok(());
        }

        self.eval_comprehension_clause(0, comp, target)
    }

    fn eval_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &ComprehensionDecl,
        target: &mut DeclarationValue,
    ) -> Result<(), EvalError> {
        if clause_idx >= comp.clauses.len() {
            // Reached the body. Its declarations are evaluated separately,
            // then merged into the target: evaluated in place, every iteration would clone and
            // re-derive everything the iterations before it generated, which is
            // quadratic in the size of the source.
            // The body declares into the literal the comprehension is written
            // in, whose names are bound in the frame of that literal's scope.
            self.eval.comprehension_body =
                self.eval
                    .current_env
                    .clone()
                    .map(|env| crate::value::Enclosing {
                        env,
                        frame: self.eval.struct_scope_depth,
                    });
            let generated = self.eval_nested_decls(&comp.struct_lit.decls);
            self.eval.comprehension_body = None;
            let generated = generated?;
            target.merge_constraints(&mut self.eval.arena, &generated, self.eval.at_file_root);
            merge_generated(
                &mut self.eval.arena,
                &mut target.structure,
                generated.structure,
            );
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => {
                match self.eval.comprehension_guard(condition)? {
                    Clause::Ready(true) => {
                        self.eval_comprehension_clause(clause_idx + 1, comp, target)?;
                    }
                    Clause::Ready(false) => {}
                    Clause::Incomplete(reason) => {
                        self.incomplete.get_or_insert(reason);
                    }
                }
            }
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval.eval_expr(expr)?;
                self.eval.push_scope();
                self.eval.insert_binding(ident, val_id);
                let result = self.eval_comprehension_clause(clause_idx + 1, comp, target);
                self.eval.pop_scope();
                result?;
            }
            ComprehensionClause::For { key, value, source } => {
                let src_val = match self.eval.comprehension_source(source)? {
                    Clause::Ready(value) => value,
                    Clause::Incomplete(reason) => {
                        self.incomplete.get_or_insert(reason);
                        return Ok(());
                    }
                };

                match src_val {
                    Value::List { elements, .. } => {
                        for (idx, &elem_id) in elements.iter().enumerate() {
                            self.eval.push_scope();
                            self.eval.insert_binding(value, elem_id);
                            if let Some(k_name) = key {
                                let k_id = self.eval.arena.int(idx as i64);
                                self.eval.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_comprehension_clause(clause_idx + 1, comp, target);
                            self.eval.pop_scope();
                            result?;
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in s.fields.iter().filter(|(_, e)| !e.optional) {
                            self.eval.push_scope();
                            self.eval.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.eval.arena.string(k.as_str());
                                self.eval.insert_binding(k_name, k_id);
                            }
                            let result =
                                self.eval_comprehension_clause(clause_idx + 1, comp, target);
                            self.eval.pop_scope();
                            result?;
                        }
                    }
                    _ => unreachable!("comprehension_source yields a list or a struct"),
                }
            }
        }

        Ok(())
    }

    /// Run a comprehension into a struct of its own, and keep its recipe there:
    /// what it generated, and whether a clause was left undecided.
    fn run_comprehension(
        &mut self,
        comp: &Rc<ComprehensionDecl>,
        deps: Rc<HashSet<String>>,
    ) -> Result<DeclarationValue, EvalError> {
        let mut own = DeclarationValue::default();
        let enclosing = self.incomplete.take();
        let result = self.eval_comprehension(comp, &mut own);
        let incomplete = std::mem::replace(&mut self.incomplete, enclosing);
        result?;
        self.keep_recipe(
            &mut own,
            DeclSource::Comprehension(comp.clone()),
            deps,
            incomplete,
        )?;
        Ok(own)
    }

    /// Evaluate a dynamic field whose label was not concrete when its literal
    /// was, into a struct of its own that keeps its recipe.
    fn run_field(&mut self, field: &Rc<FieldDecl>) -> Result<DeclarationValue, EvalError> {
        let Label::Dynamic(label) = &field.label else {
            return Ok(DeclarationValue::default());
        };
        let Some(env) = self.eval.current_env.clone() else {
            return Err(EvalError::Evaluation(
                "dynamic field outside a literal".to_string(),
            ));
        };
        let mut own = DeclarationValue::default();
        let incomplete = match self.eval.dynamic_label(label)? {
            Clause::Ready(name) => {
                let saved_field = self.eval.current_field.replace(name.clone());
                let res = self.eval.eval_field_value(&field.value, &env);
                self.eval.current_field = saved_field;
                let (val_id, conjunct) = res?;
                self.eval.unify_decl_field(
                    &mut own.structure.fields,
                    &name,
                    val_id,
                    field.optional,
                    conjunct,
                )?;
                None
            }
            Clause::Incomplete(reason) => Some(reason),
        };
        let deps = Rc::new(crate::deps::recipe_deps(label, &[]));
        self.keep_recipe(&mut own, DeclSource::Field(field.clone()), deps, incomplete)?;
        Ok(own)
    }

    fn keep_recipe(
        &mut self,
        own: &mut DeclarationValue,
        source: DeclSource,
        deps: Rc<HashSet<String>>,
        incomplete: Option<String>,
    ) -> Result<(), EvalError> {
        let Some(env) = self.eval.current_env.clone() else {
            return match incomplete {
                Some(reason) => Err(EvalError::Evaluation(reason)),
                None => Ok(()),
            };
        };
        let deps = {
            let lets = env.reachable_lets();
            if lets.iter().any(|(name, _)| deps.contains(name)) {
                Rc::new(crate::deps::expand_lets((*deps).clone(), &lets))
            } else {
                deps
            }
        };
        let fields = SECTIONS
            .iter()
            .flat_map(|section| {
                section
                    .map(&own.structure)
                    .iter()
                    .map(|(name, entry)| (*section, name.clone(), entry.conjuncts.clone()))
            })
            .collect();
        let outcome = DeclOutcome {
            fields,
            recipes: own.structure.recipes.clone(),
            incomplete,
        };
        own.structure.add_recipe(DeclRecipe {
            source,
            env,
            deps,
            outcome: Rc::new(outcome),
            closes: false,
        });
        Ok(())
    }

    /// Run again the declarations of a merged struct that read a name the
    /// merge moved. One that decides differently now gives back the fields it
    /// generated and contributes what it generates instead. Returns the names
    /// of the fields that changed that way, or `None` when every one decided
    /// as it had.
    fn derive_recipes(
        &mut self,
        s: &mut StructValue,
        moved: &HashSet<String>,
    ) -> Result<Option<HashSet<String>>, EvalError> {
        let mut changed = HashSet::new();
        let mut wrote = false;
        for recipe in s.recipes.clone() {
            if !recipe.deps.iter().any(|name| moved.contains(name)) || !s.recipes.contains(&recipe)
            {
                continue;
            }
            let Some(mut fresh) = self.rerun_recipe(&recipe, s)? else {
                continue;
            };
            if recipe.closes {
                definition_generation(&mut self.eval.arena, &mut fresh.structure, s);
            }
            let decided_alike = fresh
                .structure
                .recipes
                .last()
                .is_some_and(|rerun| rerun.same_decision(&recipe));
            if decided_alike {
                // The same fields, but what a clause bound for them - `v` of
                // `for v in l`, `y` of `let y = a` - is the value from before
                // the merge in the fields generated the last time. The run
                // just made bound the merged one.
                if recipe.binds_clause_names() {
                    wrote |= self.adopt_rerun(s, &recipe, fresh.structure, &mut changed)?;
                }
                continue;
            }
            wrote = true;
            self.retract(s, &recipe, &mut changed);
            changed.extend(
                SECTIONS
                    .iter()
                    .flat_map(|section| section.map(&fresh.structure).keys().cloned()),
            );
            // What it generates now is part of the struct it was declared in,
            // so the fields a merge admitted on its credit are settled.
            let generated = self
                .eval
                .arena
                .alloc(Value::Struct(Box::new(fresh.structure.clone())));
            merge_generated(&mut self.eval.arena, s, fresh.structure);
            crate::closedness::vouch_struct(
                &mut self.eval.arena,
                s,
                generated,
                recipe.source_ptr(),
            );
        }
        Ok(wrote.then_some(changed))
    }

    /// Put what a rerun that decided as `old` did generate in place of what
    /// `old` generated: each field's conjuncts from `old` give way to the
    /// rerun's, where they stood, and the field is derived from them. Reports
    /// whether any field changed; the names whose value moved join `changed`.
    fn adopt_rerun(
        &mut self,
        s: &mut StructValue,
        old: &DeclRecipe,
        fresh: StructValue,
        changed: &mut HashSet<String>,
    ) -> Result<bool, EvalError> {
        let mut updates = Vec::new();
        for (section, name, before) in &old.outcome.fields {
            let (Some(entry), Some(after)) =
                (section.map(s).get(name), section.map(&fresh).get(name))
            else {
                continue;
            };
            let mut conjuncts = Vec::with_capacity(entry.conjuncts.len() + after.conjuncts.len());
            let mut placed = false;
            for conjunct in entry.conjuncts.iter() {
                if !before.iter().any(|c| c.same(conjunct)) {
                    conjuncts.push(conjunct.clone());
                } else if !placed {
                    conjuncts.extend(after.conjuncts.iter().cloned());
                    placed = true;
                }
            }
            // The field no longer holds what `old` generated there.
            if !placed {
                continue;
            }
            let candidate = FieldEntry::with_conjuncts(entry.val, entry.optional, conjuncts);
            let val = self.derive_field(&candidate, s, name)?.unwrap_or(entry.val);
            let moved = val != entry.val
                && compare_values(&self.eval.arena, val, entry.val) != Equivalence::Equal;
            updates.push((
                *section,
                name.clone(),
                FieldEntry { val, ..candidate },
                moved,
            ));
        }
        let wrote = !updates.is_empty();
        for (section, name, entry, moved) in updates {
            if moved {
                changed.insert(name.clone());
            }
            section.map_mut(s).insert(name, entry);
        }
        // The rerun's recipes - its own, last, and those of the literals it
        // generated - stand where `old` and its literals' stood.
        let at = s.recipes.iter().position(|kept| kept == old);
        s.recipes
            .retain(|kept| kept != old && !old.outcome.recipes.contains(kept));
        let at = at.map_or(s.recipes.len(), |at| at.min(s.recipes.len()));
        s.recipes.splice(at..at, fresh.recipes);
        Ok(wrote)
    }

    /// Run one recipe in the scope its literal was written in, under a frame
    /// holding the merged values of the names that literal declares - as a
    /// field's recipe runs. `None` while it still waits on a reference.
    fn rerun_recipe(
        &mut self,
        recipe: &DeclRecipe,
        s: &StructValue,
    ) -> Result<Option<DeclarationValue>, EvalError> {
        let env = &recipe.env;
        let saved_scopes = std::mem::replace(&mut self.eval.scopes, env.scopes.clone());
        let saved_env = self.eval.current_env.replace(env.clone());
        let saved_imports = std::mem::replace(&mut self.eval.imports, env.imports.clone());
        let saved_depth = self.eval.struct_scope_depth;
        let saved_field = self.eval.current_field.take();

        let mut result = self.rebind_enclosing(env, s);
        self.eval.push_scope();
        self.eval.struct_scope_depth = self.eval.scopes.len().saturating_sub(1);
        for section in SECTIONS {
            for (name, entry) in section.map(s) {
                if env.owns_field(name) {
                    self.eval.insert_binding(name, entry.val);
                }
            }
        }
        for (name, expr) in env.lets.clone() {
            if result.is_err() {
                break;
            }
            match self.eval.eval_expr(&expr) {
                Ok(val) => self.eval.insert_binding(&name, val),
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        let result = result.and_then(|()| match &recipe.source {
            DeclSource::Comprehension(comp) => self.run_comprehension(comp, recipe.deps.clone()),
            DeclSource::Field(field) => self.run_field(field),
        });

        self.eval.scopes = saved_scopes;
        self.eval.current_env = saved_env;
        self.eval.imports = saved_imports;
        self.eval.struct_scope_depth = saved_depth;
        self.eval.current_field = saved_field;
        match result {
            Ok(fresh) => Ok(Some(fresh)),
            Err(EvalError::Unresolved(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Take back what a recipe generated: its recipes from each field, the
    /// field itself where nothing else declares it, and the recipes of the
    /// literals it generated.
    fn retract(&mut self, s: &mut StructValue, recipe: &DeclRecipe, changed: &mut HashSet<String>) {
        for (section, name, contributed) in &recipe.outcome.fields {
            let Some(entry) = section.map(s).get(name).cloned() else {
                continue;
            };
            let remaining: Vec<Conjunct> = entry
                .conjuncts
                .iter()
                .filter(|conjunct| !contributed.iter().any(|c| c.same(conjunct)))
                .cloned()
                .collect();
            if remaining.len() == entry.conjuncts.len() {
                continue;
            }
            changed.insert(name.clone());
            if remaining.is_empty() {
                section.map_mut(s).shift_remove(name);
                continue;
            }
            let rest = FieldEntry::with_conjuncts(entry.val, entry.optional, remaining);
            let snapshot = s.clone();
            let val = match self.derive_field(&rest, &snapshot, name) {
                Ok(Some(val)) => val,
                _ => entry.val,
            };
            section
                .map_mut(s)
                .insert(name.clone(), FieldEntry { val, ..rest });
        }
        s.recipes
            .retain(|kept| kept != recipe && !recipe.outcome.recipes.contains(kept));
    }
}

/// Mark what a definition's declaration generated as the definition's: its
/// recipes, and each field's conjuncts, so deriving them again closes them.
/// A field the definition declares nowhere else - absent from `target`, or
/// there only on credit - is the definition's whole declaration of it, so it
/// is closed now and what the merge put there meets it closed. A field it
/// declares elsewhere too merges open: closing one declaration on its own
/// would refuse what the others declare.
fn definition_generation(
    arena: &mut crate::value::ValueArena,
    generated: &mut StructValue,
    target: &StructValue,
) {
    for recipe in &mut generated.recipes {
        recipe.closes = true;
    }
    for (name, entry) in generated.fields.iter_mut() {
        if let Some(conjuncts) = crate::closedness::definition_conjuncts(&entry.conjuncts) {
            entry.conjuncts = conjuncts;
        }
        let declared_elsewhere = target.fields.contains_key(name)
            && !target.provisional.iter().any(|credit| credit.name == *name);
        if !declared_elsewhere {
            entry.val = Closure::Recursive.apply(arena, entry.val);
        }
    }
}

/// Arc order for a literal: its own fields where it declares them, then what
/// embeddings, comprehensions and dynamic labels added, as they were added.
/// Passes resolve fields in dependency order, which is not the order upstream
/// creates them in.
fn put_literal_fields_first(target: &mut DeclarationValue, decls: &[Decl]) {
    let mut labels = Vec::new();
    crate::arc_order::decl_literal_labels(decls, &mut labels);
    crate::arc_order::put_literal_fields_first(&mut target.structure, &labels, &target.late_fields);
}
