//! Declaration scheduling for the relaxation loop.
//!
//! Pure helpers with no evaluator state: naming pending bindings, collecting a
//! literal's fields, and tracking what one re-derivation sweep did. The loop
//! itself stays in `eval.rs`; this module owns the shapes it reasons about.

use crate::unify::{Equivalence, compare_values, unify};
use crate::value::{FieldEntry, StructValue, ValueArena, ValueId};
use cue_syntax::ast::*;
use std::collections::{BTreeMap, HashMap, HashSet};

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
        Decl::Field(f) if !f.label.is_definition() && !f.label.is_hidden() => f.label.name(),
        _ => None,
    }
}

/// Field names one literal declares outright. A reference inside it resolves
/// to these at whatever value the merged struct gives them; anything else it
/// names belongs to an enclosing scope and keeps resolving there.
pub(crate) fn collect_field_names(decls: &[Decl]) -> HashSet<String> {
    decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Field(field) => field.label.name().map(str::to_string),
            _ => None,
        })
        .collect()
}

/// Merge repeated fields of one literal into unification conjuncts.
///
/// A literal may declare one field twice; the pair means their unification,
/// folded here so the relaxation loop sees one declaration per name. The
/// namespace key includes the definition/hidden sigils: quoted labels share
/// the ordinary namespace even when their text starts with `#` or `_`.
pub(crate) fn collect_field_declarations(decls: &[Decl]) -> Vec<Decl> {
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

/// Which map of a struct a field lives in. Definitions and hidden fields keep
/// their sigil in the name, so the three never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Field,
    Definition,
    Hidden,
}

impl Section {
    pub(crate) fn map(self, s: &StructValue) -> &BTreeMap<String, FieldEntry> {
        match self {
            Section::Field => &s.fields,
            Section::Definition => &s.definitions,
            Section::Hidden => &s.hidden,
        }
    }

    pub(crate) fn map_mut(self, s: &mut StructValue) -> &mut BTreeMap<String, FieldEntry> {
        match self {
            Section::Field => &mut s.fields,
            Section::Definition => &mut s.definitions,
            Section::Hidden => &mut s.hidden,
        }
    }
}

pub(crate) const SECTIONS: [Section; 3] = [Section::Field, Section::Definition, Section::Hidden];

/// The order to derive a struct's fields in: a field after the fields it
/// reads, so one sweep carries a change the whole length of a chain.
///
/// Only names this struct holds are edges - anything else a recipe mentions
/// comes from an enclosing scope, which a merge here cannot change. Recipes
/// that read each other have no such order, and are left to the sweep loop.
pub(crate) fn derivation_order(s: &StructValue) -> Vec<(Section, String)> {
    let nodes: Vec<(Section, String)> = SECTIONS
        .iter()
        .flat_map(|section| section.map(s).keys().map(|name| (*section, name.clone())))
        .collect();
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
///
/// Pure over its inputs: the evaluator passes its arena and a scope lookup,
/// keeping the progress question testable without an evaluator.
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
            (None, Some(_)) => true,
            (Some(prev), Some(now)) => {
                prev != now && compare_values(arena, prev, now) != Equivalence::Equal
            }
            _ => false,
        }
    })
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
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    let existing = slot.get_mut();
                    existing.val = unify(&mut *arena, existing.val, entry.val);
                    existing.optional &= entry.optional;
                    existing.extend_conjuncts(entry.conjuncts.iter().cloned());
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
            }
        }
    }
    target.pattern_constraints.extend(pattern_constraints);
    target.is_open |= is_open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_fields_fold_into_unification() {
        let file = cue_syntax::parse_file("a: 1\na: 2\nb: 3\n").unwrap();
        let folded = collect_field_declarations(&file.decls);
        let fields: Vec<_> = folded
            .iter()
            .filter_map(|decl| match decl {
                Decl::Field(f) => Some(f),
                _ => None,
            })
            .collect();
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0].value, Expr::Binary { .. }));
        assert_eq!(collect_field_names(&file.decls).len(), 2);
    }

    #[test]
    fn derivation_orders_readers_after_what_they_read() {
        use crate::value::{Conjunct, FieldEntry, Imports, Thunk, ThunkEnv, ValueArena};
        use std::rc::Rc;

        let mut arena = ValueArena::new();
        let mut s = StructValue::new(false);
        for (name, deps) in [("c", &["b"][..]), ("b", &["a"][..]), ("a", &[][..])] {
            let thunk = Thunk {
                expr: Rc::new(Expr::Top),
                env: Rc::new(ThunkEnv::new(
                    Vec::new(),
                    Vec::new(),
                    HashSet::new(),
                    Rc::new(Imports::default()),
                )),
                deps: Rc::new(deps.iter().map(|s| s.to_string()).collect()),
            };
            let val = arena.top();
            let mut entry = FieldEntry::value(val, false);
            entry.extend_conjuncts([Conjunct::Thunk(thunk)]);
            s.fields.insert(name.to_string(), entry);
        }
        let order = derivation_order(&s);
        let names: Vec<&str> = order.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn refinement_detects_new_and_changed_bindings() {
        let mut arena = ValueArena::new();
        let one = arena.int(1);
        let two = arena.int(2);
        let file = cue_syntax::parse_file("a: 1\n#D: 2\n").unwrap();
        let pending: Vec<&Decl> = file.decls.iter().collect();
        let field = &pending[..1];
        // Newly bound.
        assert!(refined_any(&arena, &|_| Some(one), field, &[None]));
        // Same value id: not a change.
        assert!(!refined_any(&arena, &|_| Some(one), field, &[Some(one)]));
        // Different value: a change.
        assert!(refined_any(&arena, &|_| Some(two), field, &[Some(one)]));
        // Lost binding is not refinement.
        assert!(!refined_any(&arena, &|_| None, field, &[Some(one)]));
        // Definitions never count, bound or not.
        assert!(!refined_any(
            &arena,
            &|_| Some(one),
            &pending[1..2],
            &[None]
        ));
    }

    #[test]
    fn only_ordinary_fields_bind_scope_names() {
        let file = cue_syntax::parse_file("a: 1\n#D: 2\n_h: 3\n").unwrap();
        let names: Vec<_> = file.decls.iter().filter_map(pending_binding_name).collect();
        assert_eq!(names, ["a"]);
    }
}
