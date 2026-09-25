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

use crate::value::{
    Conjunct, DisjunctionBranch, MetadataSource, PatternConstraint, Value, ValueArena, ValueId,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Closure {
    Recursive,
    Outer,
    Contents,
}

impl Closure {
    pub(crate) fn apply(self, arena: &mut ValueArena, value: ValueId) -> ValueId {
        match self {
            Self::Recursive => ClosedCopies::default().close(arena, value),
            Self::Outer => reclose(arena, value),
            Self::Contents => {
                let closed = ClosedCopies::default().close(arena, value);
                open_for_embedding(arena, closed).0
            }
        }
    }
}

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
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            if matches!(
                metadata.source,
                MetadataSource::Closed {
                    closure: Closure::Recursive,
                    ..
                }
            ) {
                return id;
            }
            let conjuncts = match metadata.source {
                MetadataSource::Embedded(conjuncts) => conjuncts,
                _ => vec![Conjunct::Value(id)],
            };
            let payload = crate::metadata::payload(arena, id);
            let closed = close_deep(arena, payload, in_progress);
            metadata.view = Some(close_deep(
                arena,
                metadata.view.unwrap_or(metadata.fields),
                in_progress,
            ));
            metadata.source = MetadataSource::Closed {
                conjuncts,
                closure: Closure::Recursive,
            };
            let value = arena.get(closed).expect("closed payload exists").clone();
            let result = arena.alloc_with_metadata(value, metadata);
            in_progress.insert(id, result);
            return result;
        }
        let payload = crate::metadata::payload(arena, id);
        let closed = close_deep(arena, payload, in_progress);
        let fields = close_deep(arena, metadata.fields, in_progress);
        if closed == payload && fields == metadata.fields {
            return id;
        }
        metadata.fields = fields;
        let value = arena.get(closed).expect("closed payload exists").clone();
        let result = arena.alloc_with_metadata(value, metadata);
        in_progress.insert(id, result);
        return result;
    }
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
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            let was_closed = match &mut metadata.source {
                MetadataSource::Closed { closure, .. } => {
                    let was_closed = *closure != Closure::Contents;
                    *closure = Closure::Contents;
                    was_closed
                }
                _ => false,
            };
            let payload = crate::metadata::payload(arena, id);
            let (opened, closed) = open_for_embedding(arena, payload);
            let (view, view_closed) =
                open_for_embedding(arena, metadata.view.unwrap_or(metadata.fields));
            metadata.view = Some(view);
            if !was_closed && !closed && !view_closed {
                return (id, false);
            }
            let value = arena.get(opened).expect("opened payload exists").clone();
            return (
                arena.alloc_with_metadata(value, metadata),
                was_closed || closed || view_closed,
            );
        }
        let (fields, closed) = open_for_embedding(arena, metadata.fields);
        let (payload, branch_closed) = if metadata.is_choice_view() {
            let payload = crate::metadata::payload(arena, id);
            open_for_embedding(arena, payload)
        } else {
            (id, false)
        };
        if fields == metadata.fields && !branch_closed {
            return (id, closed);
        }
        metadata.fields = fields;
        let value = arena.get(payload).expect("metadata owner exists").clone();
        return (
            arena.alloc_with_metadata(value, metadata),
            closed || branch_closed,
        );
    }
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
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            if matches!(
                metadata.source,
                MetadataSource::Closed {
                    closure: Closure::Recursive | Closure::Outer,
                    ..
                }
            ) {
                return id;
            }
            let conjuncts = match metadata.source {
                MetadataSource::Embedded(conjuncts) => conjuncts,
                _ => vec![Conjunct::Value(id)],
            };
            let payload = crate::metadata::payload(arena, id);
            let closed = reclose(arena, payload);
            metadata.view = Some(reclose(arena, metadata.view.unwrap_or(metadata.fields)));
            metadata.source = MetadataSource::Closed {
                conjuncts,
                closure: Closure::Outer,
            };
            let value = arena.get(closed).expect("closed payload exists").clone();
            return arena.alloc_with_metadata(value, metadata);
        }
        let fields = reclose(arena, metadata.fields);
        let payload = if metadata.is_choice_view() {
            let payload = crate::metadata::payload(arena, id);
            reclose(arena, payload)
        } else {
            id
        };
        if fields == metadata.fields && payload == id {
            return id;
        }
        metadata.fields = fields;
        let value = arena.get(payload).expect("metadata owner exists").clone();
        return arena.alloc_with_metadata(value, metadata);
    }
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
