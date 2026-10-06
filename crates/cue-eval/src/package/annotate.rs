use super::model::OriginAnnotation;
use crate::eval::Evaluator;
use crate::value::{DisjunctionBranch, StructValue, Value, ValueId};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn annotate_origin(evaluator: &mut Evaluator, root_id: ValueId, origin_dir: &Path) {
    let Some(annotation) = evaluator.origin.clone() else {
        return;
    };
    let Some(Value::Struct(mut root)) = evaluator.arena.get(root_id).cloned() else {
        return;
    };
    let origin = evaluator
        .arena
        .string(origin_dir.to_string_lossy().to_string());
    let mut done = HashMap::new();
    for entry in root.fields.values_mut() {
        entry.val = annotate(evaluator, entry.val, &annotation, origin, &mut done);
    }
    if let Some(value) = evaluator.arena.get_mut(root_id) {
        *value = Value::Struct(root);
    }
}

/// `id` with every struct below it that carries the marker annotated, copied on write.
fn annotate(
    evaluator: &mut Evaluator,
    id: ValueId,
    annotation: &OriginAnnotation,
    origin: ValueId,
    done: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    if let Some(&copy) = done.get(&id) {
        return copy;
    }
    done.insert(id, id);

    let annotated = match evaluator.arena.get(id).cloned() {
        Some(Value::Struct(s)) => annotate_struct(evaluator, id, *s, annotation, origin, done),
        Some(Value::Disjunction { branches }) => {
            annotate_disjunction(evaluator, id, branches, annotation, origin, done)
        }
        Some(Value::List { elements, ellipsis }) => {
            annotate_list(evaluator, id, elements, ellipsis, annotation, origin, done)
        }
        _ => id,
    };
    done.insert(id, annotated);
    annotated
}

fn annotate_struct(
    evaluator: &mut Evaluator,
    id: ValueId,
    mut s: StructValue,
    annotation: &OriginAnnotation,
    origin: ValueId,
    done: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    let mut changed = false;
    for entry in s.fields.values_mut() {
        let val = annotate(evaluator, entry.val, annotation, origin, done);
        changed |= val != entry.val;
        entry.val = val;
    }

    let set = s.fields.get(&annotation.field).is_some_and(|f| !f.optional);
    if s.hidden.contains_key(&annotation.marker) && !set {
        s.insert_field(annotation.field.clone(), origin, false);
        changed = true;
    }

    if changed {
        evaluator.arena.alloc_like(id, Value::Struct(Box::new(s)))
    } else {
        id
    }
}

fn annotate_disjunction(
    evaluator: &mut Evaluator,
    id: ValueId,
    branches: Vec<DisjunctionBranch>,
    annotation: &OriginAnnotation,
    origin: ValueId,
    done: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    let annotated: Vec<DisjunctionBranch> = branches
        .iter()
        .map(|b| DisjunctionBranch {
            default: b.default,
            val: annotate(evaluator, b.val, annotation, origin, done),
        })
        .collect();

    if annotated == branches {
        id
    } else {
        evaluator.arena.alloc_like(
            id,
            Value::Disjunction {
                branches: annotated,
            },
        )
    }
}

fn annotate_list(
    evaluator: &mut Evaluator,
    id: ValueId,
    elements: Vec<ValueId>,
    ellipsis: Option<ValueId>,
    annotation: &OriginAnnotation,
    origin: ValueId,
    done: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    let annotated: Vec<ValueId> = elements
        .iter()
        .map(|&e| annotate(evaluator, e, annotation, origin, done))
        .collect();

    if annotated == elements {
        id
    } else {
        evaluator.arena.alloc_like(
            id,
            Value::List {
                elements: annotated,
                ellipsis,
            },
        )
    }
}
