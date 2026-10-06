//! The order of a struct's fields, which is the order a comprehension over the
//! struct yields them.
//!
//! Upstream iterates a struct in arc order: the order its evaluator creates the
//! fields. A struct literal inserts its static fields as soon as it is
//! scheduled, along with those of the literals it embeds or is unified with
//! (`{a} & {b}`, `{{a}, b}`) and the static labels of its comprehension bodies.
//! Everything else waits on a queue and adds its fields after those, in the
//! order it resolves: references and selectors, dynamic labels, the
//! references in comprehension bodies, and, last of all, disjunctions. So
//! `#D & {y: 1, z: 2}` iterates `y, z` whatever order `#D` declares them in,
//! and `{S, c: 1}` iterates `c` before the fields of `S`.
//!
//! The evaluator inserts fields in its own order, the left operand of a merge
//! first. These helpers move the literal fields of an expression or a
//! declaration list to the front, keeping everything else in insertion order.

use crate::schedule::SECTIONS;
use crate::value::{FieldMap, StructValue, Value, ValueArena, ValueId};
use cue_syntax::ast::{BinaryOp, Decl, Expr};

/// The static labels `expr` declares without resolving anything, in
/// declaration order.
pub(crate) fn expr_literal_labels(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Struct(lit) => decl_literal_labels(&lit.decls, out),
        Expr::Binary {
            op: BinaryOp::Unify,
            left,
            right,
        } => {
            expr_literal_labels(left, out);
            expr_literal_labels(right, out);
        }
        Expr::Spread { expr } => expr_literal_labels(expr, out),
        _ => {}
    }
}

/// The static labels a literal with these declarations inserts on sight.
pub(crate) fn decl_literal_labels(decls: &[Decl], out: &mut Vec<String>) {
    for decl in decls {
        match decl {
            Decl::Field(field) => {
                if let Some(name) = field.label.name() {
                    out.push(name.to_string());
                }
            }
            Decl::Embedding(expr) => expr_literal_labels(expr, out),
            // A comprehension reserves the static fields of its body when it
            // is scheduled; a yield that never happens leaves no field.
            Decl::Comprehension(comp) => decl_literal_labels(&comp.struct_lit.decls, out),
            _ => {}
        }
    }
}

/// Puts the fields named in `labels` first, in that order, then the rest in
/// their current order, and the fields named in `late` last. Returns whether
/// anything moved.
pub(crate) fn put_literal_fields_first(
    s: &mut StructValue,
    labels: &[String],
    late: &[String],
) -> bool {
    let plans: Vec<_> = SECTIONS
        .iter()
        .map(|section| plan(section.map(s), labels.iter().map(String::as_str), late))
        .collect();
    let moved = plans.iter().any(Option::is_some);
    apply(s, plans);
    moved
}

/// A copy of `s` with the fields named in `labels` first, or `None` when the
/// order already holds, so a caller can skip copying a struct that would not
/// change.
pub(crate) fn literal_fields_first(s: &StructValue, labels: &[String]) -> Option<StructValue> {
    if labels.is_empty() {
        return None;
    }
    let plans: Vec<_> = SECTIONS
        .iter()
        .map(|section| plan(section.map(s), labels.iter().map(String::as_str), &[]))
        .collect();
    reordered(s, plans)
}

/// `id` with the fields named in `labels` first: a struct directly, a
/// disjunction branch by branch. Metadata moves to the reordered copy with
/// the value it describes.
pub(crate) fn with_literal_fields_first(
    arena: &mut ValueArena,
    id: ValueId,
    labels: &[String],
) -> ValueId {
    if labels.is_empty() {
        return id;
    }
    let value = match arena.get(id) {
        Some(Value::Struct(s)) => match literal_fields_first(s, labels) {
            Some(s) => Value::Struct(Box::new(s)),
            None => return id,
        },
        Some(Value::Disjunction { branches }) => {
            let mut branches = branches.clone();
            let mut moved = false;
            for branch in &mut branches {
                let val = with_literal_fields_first(arena, branch.val, labels);
                moved |= val != branch.val;
                branch.val = val;
            }
            if !moved {
                return id;
            }
            Value::Disjunction { branches }
        }
        _ => return id,
    };
    match arena.metadata(id).cloned() {
        Some(metadata) => arena.alloc_with_metadata(value, metadata),
        None => arena.alloc(value),
    }
}

/// `merged`, a merge with `first`, with the fields of `first` in front. A
/// disjunction resolves after everything beside it, so a branch merged with a
/// value keeps that value's fields first; so does a literal merged with what
/// its embedded references resolved to.
pub(crate) fn keep_fields_first(
    arena: &mut ValueArena,
    merged: ValueId,
    other: ValueId,
) -> ValueId {
    if merged == other || arena.metadata(merged).is_some() || arena.has_embedded_recipe(merged) {
        return merged;
    }
    let (Some(Value::Struct(m)), Some(Value::Struct(o))) = (arena.get(merged), arena.get(other))
    else {
        return merged;
    };
    let plans: Vec<_> = SECTIONS
        .iter()
        .map(|section| {
            let front = section.map(o).keys().map(String::as_str);
            plan(section.map(m), front, &[])
        })
        .collect();
    match reordered(m, plans) {
        Some(s) => arena.alloc(Value::Struct(Box::new(s))),
        None => merged,
    }
}

fn reordered(s: &StructValue, plans: Vec<Option<Vec<usize>>>) -> Option<StructValue> {
    if plans.iter().all(Option::is_none) {
        return None;
    }
    let mut copy = s.clone();
    apply(&mut copy, plans);
    Some(copy)
}

fn apply(s: &mut StructValue, plans: Vec<Option<Vec<usize>>>) {
    for (section, plan) in SECTIONS.iter().zip(plans) {
        let Some(order) = plan else { continue };
        let map = section.map_mut(s);
        let mut entries: Vec<Option<_>> = std::mem::take(map).into_iter().map(Some).collect();
        for index in order {
            let (name, entry) = entries[index].take().expect("each index placed once");
            map.insert(name, entry);
        }
    }
}

/// The new order of `map` by index (`front` first, `late` last), or `None`
/// when nothing moves.
fn plan<'a>(
    map: &FieldMap,
    front: impl Iterator<Item = &'a str>,
    late: &[String],
) -> Option<Vec<usize>> {
    if map.len() < 2 {
        return None;
    }
    let front: Vec<usize> = front.filter_map(|label| map.get_index_of(label)).collect();
    let late: Vec<usize> = late
        .iter()
        .filter_map(|label| map.get_index_of(label))
        .filter(|index| !front.contains(index))
        .collect();
    let rest = (0..map.len()).filter(|index| !late.contains(index));
    let mut order = Vec::with_capacity(map.len());
    let mut placed = vec![false; map.len()];
    for index in front
        .iter()
        .copied()
        .chain(rest)
        .chain(late.iter().copied())
    {
        if !std::mem::replace(&mut placed[index], true) {
            order.push(index);
        }
    }
    let moved = order.iter().enumerate().any(|(at, index)| at != *index);
    moved.then_some(order)
}
