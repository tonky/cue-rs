//! The relaxation loop: settling declarations order-independently.
//!
//! `RelaxationLoop` owns the declaration driver — scoped evaluation,
//! forward-reference passes, refinement allowance and nested literals.
//! It borrows the evaluator, so every value and binding operation stays on
//! `Evaluator`; what moved here is the scheduling of declarations.

use crate::closedness::{open_for_embedding, reclose};
use crate::declaration::DeclarationValue;
use crate::eval::{EvalError, Evaluator};
use crate::schedule::{
    SECTIONS, Section, Sweep, collect_field_declarations, collect_field_names, derivation_order,
    merge_generated, pending_binding_name, refined_any,
};
use crate::unify::{Equivalence, compare_values, push_branch, unify};
use crate::value::{
    BottomKind, Conjunct, DisjunctionBranch as ValueBranch, FieldEntry, StructValue, Thunk,
    ThunkEnv, Value, ValueId,
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
}

impl<'a> RelaxationLoop<'a> {
    pub(crate) fn new(eval: &'a mut Evaluator) -> Self {
        Self { eval }
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
        // A reference must see every declaration of a static field, including
        // declarations appearing after the reference in source order.
        let decls = collect_field_declarations(decls);
        // Captured once per literal and shared by its thunks: every field of this
        // struct was written in the same lexical scope.
        let env = Rc::new(ThunkEnv::new(
            self.eval.scopes.clone(),
            self.collect_let_declarations(&decls),
            collect_field_names(&decls),
            self.eval.imports.clone(),
        ));
        self.eval.current_env = Some(env.clone());
        // Reserve names that shadow an existing outer binding. Nested literals
        // must not read that outer value while this field is still uncomputed.
        // Without an outer binding a missing lookup already waits, so it needs
        // no placeholder. All reservations can share one pending value.
        let mut pending = None;
        for decl in &decls {
            if let Decl::Field(field) = decl
                && !field.label.is_definition()
                && let Some(name) = field.label.name()
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
        for decl in &decls {
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
            let mut named_decls = decls.clone();
            for decl in &mut named_decls {
                if let Decl::Field(field) = decl
                    && let Label::Dynamic(expr) = &field.label
                {
                    let label = self.eval.eval_expr(expr)?;
                    match self.eval.arena.get(label) {
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
            *target = base_struct;
            self.eval.scopes = base_scopes;
            return self.eval_decls_into(&named_decls, target);
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
                    target
                        .structure
                        .add_pattern_constraint(pattern_val, target_val);
                    Ok(!self.eval.arena.is_unresolved(target_val))
                }
                Label::Dynamic(dyn_expr) => {
                    let label_val_id = self.eval.eval_expr(dyn_expr)?;
                    if let Some(Value::String(name)) = self.eval.arena.get(label_val_id) {
                        let name = name.clone();
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
                        self.eval.note_field_name(&name);
                        self.eval.insert_binding(&name, val_id);
                        Ok(!is_unresolved)
                    } else if self.eval.arena.is_unresolved(label_val_id) {
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
                    let saved_field = self.eval.current_field.clone();
                    if let Some(name) = f.label.name() {
                        self.eval.current_field = Some(name.to_string());
                    }
                    let res = self.eval.eval_field_value(&f.value, env);
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
                        self.eval.insert_binding(name, val_id);
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
                    let current_id = self
                        .eval
                        .arena
                        .alloc(Value::Struct(target.structure.clone()));
                    let unified_id = unify(&mut self.eval.arena, current_id, embedded_id);
                    let unified_id = if was_closed && !self.eval.at_file_root {
                        reclose(&mut self.eval.arena, unified_id)
                    } else {
                        unified_id
                    };
                    if let Some(Value::Struct(s)) = self.eval.arena.get(unified_id) {
                        target.structure = s.clone();
                        self.eval.bind_struct_fields(&target.structure);
                    } else {
                        target.constrain(&mut self.eval.arena, unified_id);
                    }
                } else {
                    if let Some(metadata) = self.eval.arena.metadata(embedded_id).cloned() {
                        let current = self
                            .eval
                            .arena
                            .alloc(Value::Struct(target.structure.clone()));
                        let fields = unify(&mut self.eval.arena, current, metadata.fields);
                        if let Some(Value::Struct(fields)) = self.eval.arena.get(fields) {
                            target.structure = fields.clone();
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
                match self.eval_comprehension(comp, &mut scratch) {
                    Err(EvalError::Unresolved(_)) if !final_pass => return Ok(false),
                    other => other?,
                }
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
                || branches
                    .iter()
                    .any(|branch| self.eval.arena.metadata(branch.val).is_some()))
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
                Conjunct::Value(id) => {
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

        let order = derivation_order(s);
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
        let mut val: Option<ValueId> = None;
        for conjunct in entry.conjuncts.iter() {
            let conjunct_val = match conjunct {
                Conjunct::Value(val) => *val,
                Conjunct::Thunk(thunk) => match self.derive_thunk(thunk, s)? {
                    Some(derived) => derived,
                    None => return Ok(None),
                },
            };
            val = Some(match val {
                None => conjunct_val,
                Some(previous) => unify(&mut self.eval.arena, previous, conjunct_val),
            });
        }
        let Some(mut val) = val else {
            return Ok(None);
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
        let mut result = Ok(None);
        for (name, expr) in thunk.env.lets.clone() {
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
            let generated = self.eval_nested_decls(&comp.struct_lit.decls)?;
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
                let cond_val = self.eval.eval_expr(condition)?;
                match self.eval.arena.get(cond_val) {
                    Some(Value::Bool(true)) => {
                        self.eval_comprehension_clause(clause_idx + 1, comp, target)?;
                    }
                    Some(Value::Bottom(r)) if r.kind.may_resolve_later() && self.eval.deferring => {
                        return Err(EvalError::Unresolved(r.to_string()));
                    }
                    _ => {}
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
                let src_id = self.eval.eval_expr(source)?;
                // A source that is not resolved yet has nothing to yield *yet*;
                // yielding nothing would settle the comprehension as empty.
                let src_val = match self.eval.arena.get(src_id) {
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
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
