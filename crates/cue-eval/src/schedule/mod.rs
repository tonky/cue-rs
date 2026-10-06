//! Declaration scheduling for the relaxation loop.
//!
//! Pure helpers with no evaluator state: naming pending bindings, collecting a
//! literal's fields, and tracking what one re-derivation sweep did. The loop
//! itself stays in `eval.rs`; this module owns the shapes it reasons about.

mod order;
#[cfg(test)]
mod tests;

pub(crate) use order::{SECTIONS, Section, derivation_order};

use crate::deps::{direct_deps, recipe_deps};
use crate::unify::{Equivalence, compare_values, field_matches_pattern, unify};
use crate::value::{BottomKind, StructValue, Value, ValueArena, ValueId};
use cue_syntax::ast::*;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// What one re-derivation sweep did: which fields moved, so the next sweep knows
/// what to derive, and whether anything was written at all, which is what the
/// caller's copy on write turns on.
#[derive(Debug, Default)]
pub(crate) struct Sweep {
    pub moved: HashSet<String>,
    pub wrote: bool,
}

/// The name a pending declaration binds its partial value to, if any. The
/// same three conditions as the binding itself: a definition or a hidden
/// field is not read this way.
pub(crate) fn pending_binding_name(decl: &Decl) -> Option<&str> {
    match decl {
        Decl::Field(f) if !f.label.is_definition() && !f.label.is_hidden() => f.label.ident_name(),
        _ => None,
    }
}

/// Field names one literal declares with an identifier label. A reference
/// inside it resolves to these at whatever value the merged struct gives them;
/// anything else it names - a quoted or dynamic label, a field an embedding or
/// a comprehension contributed - belongs to an enclosing scope and keeps
/// resolving there, as upstream scopes references.
pub(crate) fn collect_field_names(decls: &[Decl]) -> HashSet<String> {
    decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Field(field) => field.label.ident_name().map(str::to_string),
            _ => None,
        })
        .collect()
}

/// `X=a: v` names the field `a` within its literal: the alias reads what `a`
/// reads, the field's merged value, so it is the binding `let X = a`. An alias
/// on a quoted or dynamic label names a field no identifier reaches, which the
/// evaluator does not model, so it is refused rather than left unbound - an
/// unbound `X` would silently read an enclosing `X`.
pub(crate) fn expand_field_aliases(decls: &[Decl]) -> Result<Cow<'_, [Decl]>, String> {
    if !decls
        .iter()
        .any(|decl| matches!(decl, Decl::Field(field) if field.alias.is_some()))
    {
        return Ok(Cow::Borrowed(decls));
    }
    let mut expanded = Vec::with_capacity(decls.len() + 1);
    for decl in decls {
        expanded.push(decl.clone());
        let Decl::Field(field) = decl else { continue };
        let Some(alias) = &field.alias else { continue };
        let reference = match &field.label {
            Label::Ident(name) => Expr::Ident(name.clone()),
            Label::DefIdent(name) => Expr::DefIdent(name.clone()),
            Label::HiddenIdent(name) => Expr::HiddenIdent(name.clone()),
            Label::HiddenDefIdent(name) => Expr::HiddenDefIdent(name.clone()),
            Label::String(name) => {
                return Err(format!(
                    "alias {alias} on the quoted label \"{name}\" is not supported"
                ));
            }
            Label::Pattern(_) | Label::Dynamic(_) => {
                return Err(format!(
                    "alias {alias} on a pattern or dynamic label is not supported"
                ));
            }
        };
        expanded.push(Decl::Let {
            ident: alias.clone(),
            expr: reference,
        });
    }
    Ok(Cow::Owned(expanded))
}

/// Merge repeated fields of one literal into unification conjuncts.
pub(crate) fn collect_field_declarations(decls: &[Decl]) -> Cow<'_, [Decl]> {
    let mut seen = HashSet::new();
    let repeated = decls.iter().any(|decl| match decl {
        Decl::Field(field) => match field.label.name() {
            Some(name) => {
                !seen.insert((name, field.label.is_definition(), field.label.is_hidden()))
            }
            None => false,
        },
        _ => false,
    });
    if !repeated {
        return Cow::Borrowed(decls);
    }

    let mut collected: Vec<Decl> = Vec::with_capacity(decls.len());
    let mut positions: HashMap<(&str, bool, bool), usize> = HashMap::new();
    for decl in decls {
        if let Decl::Field(field) = decl
            && let Some(name) = field.label.name()
        {
            let key = (name, field.label.is_definition(), field.label.is_hidden());
            if let Some(&index) = positions.get(&key) {
                let Decl::Field(previous) = &mut collected[index] else {
                    unreachable!();
                };
                // `"a": 1, a: int` is one field that `a` refers to: keep the
                // identifier spelling, which is the one that declares the name.
                if previous.label.ident_name().is_none() {
                    previous.label = field.label.clone();
                }
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
    Cow::Owned(collected)
}

/// Whether a pass that resolved no declaration nevertheless left one of them
/// bound to more than it was.
pub(crate) fn refined_any(
    arena: &ValueArena,
    lookup: &dyn Fn(&str) -> Option<ValueId>,
    pending: &[&Decl],
    before: &[Option<ValueId>],
) -> bool {
    pending.iter().zip(before).any(|(decl, &prev)| {
        let Some(name) = pending_binding_name(decl) else {
            return false;
        };
        match (prev, lookup(name)) {
            (None, Some(now)) => !is_cycle_bottom(arena, now),
            (Some(prev), Some(now)) => {
                prev != now
                    && compare_values(arena, prev, now) != Equivalence::Equal
                    && !is_cycle_bottom(arena, now)
            }
            _ => false,
        }
    })
}

/// Whether the binding already failed terminally.
fn is_cycle_bottom(arena: &ValueArena, id: ValueId) -> bool {
    matches!(arena.get(id), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Cycle)
}

/// Whether a stalled field waits only on names its own literal cannot settle.
pub(crate) fn waits_only_on_external(
    arena: &ValueArena,
    lookup: &dyn Fn(&str) -> Option<ValueId>,
    decl: &Decl,
    internal: &HashSet<String>,
    lets: &[(String, Rc<Expr>)],
) -> bool {
    let Decl::Field(field) = decl else {
        return false;
    };
    if field.label.is_definition() || field.label.is_hidden() {
        return false;
    }
    let mut deps = recipe_deps(&field.value, lets);
    match &field.label {
        Label::Pattern(expr) | Label::Dynamic(expr) => deps.extend(direct_deps(expr)),
        _ => {}
    }
    let mut external_wait = false;
    for dep in deps {
        match lookup(&dep) {
            None => return false,
            Some(id) => {
                if !binding_pending(arena, id) {
                    continue;
                }
                if internal.contains(&dep) {
                    return false;
                }
                external_wait = true;
            }
        }
    }
    external_wait
}

/// Fields a re-derivation sweep must revisit.
pub(crate) fn seed_moved(arena: &ValueArena, s: &StructValue) -> HashSet<String> {
    let mut moved: HashSet<String> = SECTIONS
        .iter()
        .flat_map(|section| section.map(s))
        .filter(|(_, entry)| entry.conjuncts.len() > 1)
        .map(|(name, _)| name.clone())
        .collect();
    if !s.pattern_constraints.is_empty() {
        for section in SECTIONS {
            for name in section.map(s).keys() {
                if s.pattern_constraints
                    .iter()
                    .any(|pc| field_matches_pattern(arena, pc.pattern_val, name))
                {
                    moved.insert(name.clone());
                }
            }
        }
    }
    moved
}

/// Whether a binding can still move on a later pass of its own literal.
fn binding_pending(arena: &ValueArena, id: ValueId) -> bool {
    matches!(
        arena.get(id),
        Some(Value::Bottom(reason)) if reason.kind == BottomKind::Unresolved
    ) || matches!(
        arena.get(id),
        Some(Value::RecursiveRef { target: None, .. })
    )
}

/// Merge what one iteration of a comprehension generated into the struct
/// it generates into, keeping each field's recipes for re-derivation.
pub(crate) fn merge_generated(
    arena: &mut ValueArena,
    target: &mut StructValue,
    generated: StructValue,
) {
    let StructValue {
        fields,
        definitions,
        hidden,
        pattern_constraints,
        is_open,
        recipes,
        ..
    } = generated;
    for (section, entries) in [
        (Section::Field, fields),
        (Section::Definition, definitions),
        (Section::Hidden, hidden),
    ] {
        let map = section.map_mut(target);
        for (name, entry) in entries {
            match map.entry(name) {
                indexmap::map::Entry::Occupied(mut slot) => {
                    let existing = slot.get_mut();
                    existing.val = unify(&mut *arena, existing.val, entry.val);
                    existing.optional &= entry.optional;
                    existing.extend_conjuncts(entry.conjuncts.iter().cloned());
                }
                indexmap::map::Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
            }
        }
    }
    target.pattern_constraints.extend(pattern_constraints);
    target.is_open |= is_open;
    for recipe in recipes {
        target.add_recipe(recipe);
    }
}
