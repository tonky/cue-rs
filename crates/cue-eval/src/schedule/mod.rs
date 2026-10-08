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
/// same conditions as the binding itself: a definition is read through its
/// placeholder instead.
pub(crate) fn pending_binding_name(decl: &Decl) -> Option<&str> {
    match decl {
        Decl::Field(f) if !f.label.is_definition() => f.label.ident_name().or_else(|| {
            bound_alias(f)
                .and_then(|_| f.label.name())
                .filter(|_| matches!(f.label, Label::String(_)))
        }),
        _ => None,
    }
}

/// Marks a field whose aliases [`expand_field_aliases`] has already turned
/// into bindings: its `alias` holds this prefix and the name it bound, so a
/// literal evaluated again does not bind it twice, and a dynamic label that
/// resolves can point its binding at the field it named.
pub(crate) const BOUND_ALIAS: char = '\u{0}';

/// The binding a dynamic label's field alias reads until the label resolves:
/// no field has this name, so a reader waits.
const UNRESOLVED_LABEL: &str = "\u{0}unresolved label";

/// The field alias an expanded field bound, if any.
pub(crate) fn bound_alias(field: &FieldDecl) -> Option<&str> {
    field.alias.as_deref()?.strip_prefix(BOUND_ALIAS)
}

/// Field names one literal declares with an identifier label. A reference
/// inside it resolves to these at whatever value the merged struct gives them;
/// anything else it names - a quoted or dynamic label, a field an embedding or
/// a comprehension contributed - belongs to an enclosing scope and keeps
/// resolving there, as upstream scopes references.
///
/// A quoted or resolved dynamic label with a field alias is named here too:
/// the alias reads the field under its label. No identifier can spell most
/// such labels; one that can (`X="a": 1`) is read by `a` in this literal,
/// where upstream reads an enclosing `a`.
pub(crate) fn collect_field_names(decls: &[Decl]) -> HashSet<String> {
    decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Field(field) => field.label.ident_name().or_else(|| {
                bound_alias(field)
                    .and_then(|_| field.label.name())
                    .filter(|_| matches!(field.label, Label::String(_)))
            }),
            _ => None,
        })
        .map(str::to_string)
        .collect()
}

/// Turn the aliases of one literal's fields into the bindings they mean.
///
/// `X=a: v`, `a~X: v` and `a: X=v` name the field `a` within its literal: the
/// alias reads what `a` reads, the field's merged value, so it is the binding
/// `let X = a`. On a quoted label the field is bound under its label (see
/// [`collect_field_names`]); on a dynamic one, under the label it resolves
/// to, once it does (see [`resolve_bound_alias`]). The label alias of
/// `a~(K,V)` is the label itself: `let K = "a"`, or the label's expression
/// for a dynamic one.
///
/// A pattern's aliases name a different field for every label it matches,
/// so they stay on the pattern, which binds them as it meets each field.
pub(crate) fn expand_field_aliases(decls: &[Decl]) -> Result<Cow<'_, [Decl]>, String> {
    if !decls.iter().any(|decl| {
        matches!(decl, Decl::Field(field) if !matches!(field.label, Label::Pattern(_))
            && (field.alias.as_deref().is_some_and(|a| !a.starts_with(BOUND_ALIAS))
                || field.label_alias.is_some()
                || field.value_alias.is_some()))
    }) {
        return Ok(Cow::Borrowed(decls));
    }
    let mut expanded = Vec::with_capacity(decls.len() + 1);
    for decl in decls {
        let Decl::Field(field) = decl else {
            expanded.push(decl.clone());
            continue;
        };
        if matches!(field.label, Label::Pattern(_)) {
            expanded.push(decl.clone());
            continue;
        }
        let mut field = field.clone();
        let mut lets = Vec::new();
        if let Some(label_alias) = field.label_alias.take() {
            let label = match &field.label {
                Label::Ident(name) | Label::String(name) => {
                    Expr::String(StringLit::quoted(name.clone()))
                }
                Label::DefIdent(_) | Label::HiddenIdent(_) | Label::HiddenDefIdent(_) => {
                    return Err("label alias cannot reference definition or hidden field".into());
                }
                Label::Dynamic(expr) => expr.clone(),
                Label::Pattern(_) => unreachable!(),
            };
            lets.push(Decl::Let {
                ident: label_alias,
                expr: label,
            });
        }
        let aliases: Vec<String> = [field.alias.take(), field.value_alias.take()]
            .into_iter()
            .flatten()
            .collect();
        let mut bound = None;
        for alias in aliases {
            if let Some(name) = alias.strip_prefix(BOUND_ALIAS) {
                bound = Some(name.to_string());
                continue;
            }
            let reference = match &field.label {
                Label::Ident(name) => Expr::Ident(name.clone()),
                Label::DefIdent(name) => Expr::DefIdent(name.clone()),
                Label::HiddenIdent(name) => Expr::HiddenIdent(name.clone()),
                Label::HiddenDefIdent(name) => Expr::HiddenDefIdent(name.clone()),
                Label::String(name) => {
                    bound = Some(alias.clone());
                    Expr::Ident(name.clone())
                }
                Label::Dynamic(_) => {
                    bound = Some(alias.clone());
                    Expr::Ident(UNRESOLVED_LABEL.to_string())
                }
                Label::Pattern(_) => unreachable!(),
            };
            lets.push(Decl::Let {
                ident: alias,
                expr: reference,
            });
        }
        field.alias = bound.map(|name| format!("{BOUND_ALIAS}{name}"));
        expanded.push(Decl::Field(field));
        expanded.extend(lets);
    }
    Ok(Cow::Owned(expanded))
}

/// A dynamic label with a field alias resolved to `name`: point the alias's
/// binding at the field it names.
pub(crate) fn resolve_bound_alias(decls: &mut [Decl], alias: &str, name: &str) {
    for decl in decls {
        if let Decl::Let { ident, expr } = decl
            && ident == alias
            && matches!(expr, Expr::Ident(pending) if pending == UNRESOLVED_LABEL)
        {
            *expr = Expr::Ident(name.to_string());
        }
    }
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
    // The later declarations' values of each repeated field, by the index of
    // its first declaration. The conjunction is built once at the end: built
    // as it grows, every declaration cloned the whole chain so far.
    let mut repeats: HashMap<usize, Vec<&Expr>> = HashMap::new();
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
                repeats.entry(index).or_default().push(&field.value);
                continue;
            }
            positions.insert(key, collected.len());
        }
        collected.push(decl.clone());
    }
    for (index, mut values) in repeats {
        let Decl::Field(field) = &mut collected[index] else {
            unreachable!();
        };
        let first = std::mem::replace(&mut field.value, Expr::Top);
        values.insert(0, &first);
        field.value = balanced_conjunction(&values);
    }
    Cow::Owned(collected)
}

/// `a: x, a: y, a: z` is `a: x & y & z`. Unification is associative, so the
/// conjunction is nested as a balanced tree, in declaration order: nested to
/// the left as the parser nests a chain, each merge copies everything the
/// declarations so far merged, and n declarations cost n² (3000 of
/// `svcs: sN: {...}` beside a `[string]: #Svc` pattern took 9 s). Up to three
/// declarations the two shapes are the same.
fn balanced_conjunction(values: &[&Expr]) -> Expr {
    match values {
        [] => Expr::Top,
        [value] => (*value).clone(),
        _ => {
            let (left, right) = values.split_at(values.len().div_ceil(2));
            Expr::Binary {
                op: BinaryOp::Unify,
                left: Box::new(balanced_conjunction(left)),
                right: Box::new(balanced_conjunction(right)),
            }
        }
    }
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
