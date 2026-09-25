//! Closing a definition's value.
//!
//! A definition is closed recursively: referencing `#A` yields a value that
//! rejects any regular field `#A` does not declare, at every depth of struct
//! literal it holds. The definition as stored stays open, so its own body can
//! embed other values and name itself while it is being built; closing happens
//! where it is read.
//!
//! What is not closed: a struct written with `...`, whose nested structs still
//! are; a value reached through a recursive reference, which is closed when that
//! reference is itself read; hidden fields and definitions, which closedness
//! never constrains.

use crate::value::{DisjunctionBranch, PatternConstraint, Value, ValueArena, ValueId};
use std::collections::HashMap;

/// Closed copies already made, keyed by the value they close.
///
/// A definition is read at every use, so its closed copy is shared. A key whose
/// node a disjunction rollback removed no longer resolves in the arena and is
/// made again.
#[derive(Debug, Default)]
pub struct ClosedCopies {
    copies: HashMap<ValueId, ValueId>,
}

impl ClosedCopies {
    pub fn close(&mut self, arena: &mut ValueArena, id: ValueId) -> ValueId {
        if let Some(&copy) = self.copies.get(&id)
            && arena.get(copy).is_some()
            && arena.get(id).is_some()
        {
            return copy;
        }
        let mut in_progress = HashMap::new();
        let copy = close_deep(arena, id, &mut in_progress);
        self.copies.insert(id, copy);
        copy
    }
}

/// `id` closed recursively, or `id` itself when nothing in it needed closing.
fn close_deep(
    arena: &mut ValueArena,
    id: ValueId,
    in_progress: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    if let Some(&copy) = in_progress.get(&id) {
        return copy;
    }
    // A cyclic graph reaches a node again before its copy exists; answering
    // with the node itself ends the walk there.
    in_progress.insert(id, id);
    let closed = match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) => {
            let mut changed = s.is_closed != !s.is_open;
            s.is_closed = !s.is_open;
            for entry in s.fields.values_mut() {
                let val = close_deep(arena, entry.val, in_progress);
                changed |= val != entry.val;
                entry.val = val;
            }
            let constraints = std::mem::take(&mut s.pattern_constraints);
            s.pattern_constraints = constraints
                .into_iter()
                .map(|pc| {
                    let target_val = close_deep(arena, pc.target_val, in_progress);
                    changed |= target_val != pc.target_val;
                    PatternConstraint { target_val, ..pc }
                })
                .collect();
            if changed {
                arena.alloc(Value::Struct(s))
            } else {
                id
            }
        }
        Some(Value::Disjunction { branches }) => {
            let closed: Vec<DisjunctionBranch> = branches
                .iter()
                .map(|b| DisjunctionBranch {
                    default: b.default,
                    val: close_deep(arena, b.val, in_progress),
                })
                .collect();
            if closed == branches {
                id
            } else {
                arena.alloc(Value::Disjunction { branches: closed })
            }
        }
        Some(Value::List { elements, ellipsis }) => {
            let closed: Vec<ValueId> = elements
                .iter()
                .map(|&e| close_deep(arena, e, in_progress))
                .collect();
            let closed_ellipsis = ellipsis.map(|e| close_deep(arena, e, in_progress));
            if closed == elements && closed_ellipsis == ellipsis {
                id
            } else {
                arena.alloc(Value::List {
                    elements: closed,
                    ellipsis: closed_ellipsis,
                })
            }
        }
        _ => id,
    };
    in_progress.insert(id, closed);
    closed
}

/// Open the outermost struct of an embedded value for the merge into the
/// literal embedding it, reporting whether it was closed.
///
/// Embedding a closed value closes the literal, but the literal's own fields
/// are allowed: `#B: {#A, y: int}` declares `y`. So the literal meets an open
/// copy and is closed afterwards with [`reclose`].
pub fn open_for_embedding(arena: &mut ValueArena, id: ValueId) -> (ValueId, bool) {
    match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) if s.is_closed => {
            s.is_closed = false;
            (arena.alloc(Value::Struct(s)), true)
        }
        Some(Value::Disjunction { branches }) => {
            let mut any_closed = false;
            let opened = branches
                .into_iter()
                .map(|b| {
                    let (val, closed) = open_for_embedding(arena, b.val);
                    any_closed |= closed;
                    DisjunctionBranch { val, ..b }
                })
                .collect();
            if any_closed {
                (arena.alloc(Value::Disjunction { branches: opened }), true)
            } else {
                (id, false)
            }
        }
        // A definition read before its declaration resolves through a
        // placeholder, which by now points at the closed value.
        Some(Value::RecursiveRef {
            target: Some(target),
            ..
        }) => open_for_embedding(arena, target),
        _ => (id, false),
    }
}

/// Close the outermost struct of a merged embedding again, or every struct
/// branch when the merge left a disjunction.
pub fn reclose(arena: &mut ValueArena, id: ValueId) -> ValueId {
    match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) if !s.is_closed && !s.is_open => {
            s.is_closed = true;
            arena.alloc(Value::Struct(s))
        }
        Some(Value::Disjunction { branches }) => {
            let closed = branches
                .into_iter()
                .map(|b| DisjunctionBranch {
                    val: reclose(arena, b.val),
                    ..b
                })
                .collect();
            arena.alloc(Value::Disjunction { branches: closed })
        }
        _ => id,
    }
}
