//! Retained fields and embedded recipes belong to a value ID, not its kind.

use crate::unify::{UnifyContext, unify_with_context};
use crate::value::{
    Conjunct, MetadataSource, StructValue, Value, ValueArena, ValueId, ValueMetadata,
};

/// Refresh values of declarations belonging to a group without admitting
/// declarations that were contributed by an outside conjunct.
pub(crate) fn project_inputs(
    arena: &mut ValueArena,
    fields: ValueId,
    inputs: &StructValue,
) -> ValueId {
    let Some(Value::Struct(mut declared)) = arena.get(fields).cloned() else {
        unreachable!("recipe inputs are a struct")
    };
    for (own, merged) in [
        (&mut declared.fields, &inputs.fields),
        (&mut declared.definitions, &inputs.definitions),
        (&mut declared.hidden, &inputs.hidden),
    ] {
        for (name, field) in own {
            if let Some(input) = merged.get(name) {
                let optional = field.optional;
                *field = input.clone();
                field.optional = optional;
            }
        }
    }
    arena.alloc(Value::Struct(declared))
}

/// A fresh payload without its owner's fields, for a kind-only constraint meet.
pub(crate) fn payload(arena: &mut ValueArena, id: ValueId) -> ValueId {
    if arena.metadata(id).is_none() {
        return id;
    }
    let value = arena.get(id).expect("metadata owner exists").clone();
    arena.alloc(value)
}

/// A type name still appears in the dependency set. It is fixed when it reads
/// the predeclared frame, with no local declaration or captured shadowing.
fn fixed_conjunct(arena: &ValueArena, conjunct: &Conjunct) -> bool {
    match conjunct {
        Conjunct::Value(id) => !arena
            .metadata(*id)
            .is_some_and(|m| matches!(m.source, MetadataSource::Closed { .. })),
        Conjunct::Thunk(thunk) => thunk.deps.iter().all(|name| {
            !thunk.env.own_fields.borrow().contains(name)
                && !thunk.env.lets.iter().any(|(local, _)| local == name)
                && !thunk
                    .env
                    .scopes
                    .iter()
                    .skip(1)
                    .any(|scope| scope.contains_key(name))
                && thunk
                    .env
                    .scopes
                    .first()
                    .and_then(|scope| scope.get(name))
                    .is_some_and(|id| matches!(arena.get(*id), Some(Value::Type(_))))
        }),
    }
}

pub(crate) fn needs_recipe(arena: &ValueArena, conjuncts: &[Conjunct]) -> bool {
    !conjuncts
        .iter()
        .all(|conjunct| fixed_conjunct(arena, conjunct))
}

pub(crate) fn finish(
    arena: &mut ValueArena,
    value: ValueId,
    fields: ValueId,
    conjuncts: Vec<Conjunct>,
) -> ValueId {
    finish_with_context(arena, value, fields, conjuncts, &mut UnifyContext::new())
}

pub(crate) fn finish_with_context(
    arena: &mut ValueArena,
    value: ValueId,
    fields: ValueId,
    conjuncts: Vec<Conjunct>,
    context: &mut UnifyContext,
) -> ValueId {
    if needs_recipe(arena, &conjuncts) {
        let materialized = materialize(arena, value, fields, context);
        let view = arena
            .metadata(materialized)
            .map(|m| m.view.unwrap_or(m.fields));
        let value = arena
            .get(materialized)
            .expect("materialized value exists")
            .clone();
        return arena.alloc_with_metadata(
            value,
            ValueMetadata {
                fields,
                view,
                source: MetadataSource::Embedded(conjuncts),
            },
        );
    }
    finish_fixed(arena, value, fields, conjuncts, context)
}

/// Join the current result with its common declarations. Recipes belong to
/// the enclosing owner, not to copies of each currently viable alternative.
pub(crate) fn materialize(
    arena: &mut ValueArena,
    value: ValueId,
    fields: ValueId,
    context: &mut UnifyContext,
) -> ValueId {
    finish_fixed(arena, value, fields, vec![Conjunct::Value(value)], context)
}

fn finish_fixed(
    arena: &mut ValueArena,
    value: ValueId,
    fields: ValueId,
    conjuncts: Vec<Conjunct>,
    context: &mut UnifyContext,
) -> ValueId {
    let body = arena.fields(fields).expect("metadata fields are a struct");
    if let Some(error) = body
        .fields
        .values()
        .filter(|entry| !entry.optional)
        .chain(body.definitions.values())
        .chain(body.hidden.values())
        .find(|entry| crate::unify::collapses_struct(arena, entry.val))
        .map(|entry| entry.val)
    {
        return error;
    }
    // Once constrained to struct kind, its normal field storage is sufficient.
    if matches!(arena.get(value), Some(Value::Struct(_))) {
        return unify_with_context(arena, value, fields, context);
    }
    // Fixed choices can retain an individual constant recipe in each
    // alternative. A thunk for the whole choice cannot stand in for a branch.
    // Common fields belong to each branch before a definition closes it.
    if let Some(Value::Disjunction { branches }) = arena.get(value) {
        let pair = (value, fields);
        if context.active_metadata_choices.len() >= crate::unify::MAX_DISJUNCTION_DEPTH
            || !context.active_metadata_choices.insert(pair)
        {
            return arena.bottom("cycle error: metadata choice normalization limit exceeded");
        }
        let mut kept = Vec::new();
        let mut errors = Vec::new();
        for branch in branches.clone() {
            let merged = attach_branch_fields(arena, branch.val, fields, context);
            if let Some(Value::Bottom(reason)) = arena.get(merged) {
                errors.push(reason.clone());
            } else {
                crate::unify::push_branch(
                    arena,
                    &mut kept,
                    crate::value::DisjunctionBranch {
                        val: merged,
                        ..branch
                    },
                );
            }
        }
        context.active_metadata_choices.remove(&pair);
        let result = crate::unify::settle_disjunction(arena, kept, &errors);
        if let Some(value @ Value::Disjunction { .. }) = arena.get(result).cloned() {
            return arena.alloc_with_metadata(
                value,
                ValueMetadata {
                    fields,
                    view: None,
                    source: MetadataSource::ChoiceFields,
                },
            );
        }
        return result;
    }
    if arena.metadata(value).is_some() {
        return attach_branch_fields(arena, value, fields, context);
    }
    let value = arena.get(value).expect("evaluated payload exists").clone();
    arena.alloc_with_metadata(
        value,
        ValueMetadata {
            fields,
            view: None,
            source: MetadataSource::Embedded(conjuncts),
        },
    )
}

/// Rebuild a selector view after branch unification has already checked all
/// actual constraints. Closing this view must not introduce another conjunct.
pub(crate) fn merged_choice_view(
    arena: &mut ValueArena,
    value: ValueId,
    left: ValueId,
    right: ValueId,
    context: &mut UnifyContext,
) -> ValueId {
    if !matches!(arena.get(value), Some(Value::Disjunction { .. }))
        || ![left, right]
            .into_iter()
            .any(|id| arena.metadata(id).is_some_and(|m| m.is_choice_view()))
    {
        return value;
    }
    let mut fields = None;
    for id in [left, right] {
        let Some(mut body) = arena.fields(id).cloned() else {
            continue;
        };
        body.is_closed = false;
        let next = arena.alloc(Value::Struct(body));
        fields = Some(match fields {
            Some(previous) => unify_with_context(arena, previous, next, context),
            None => next,
        });
    }
    let fields = fields.expect("a choice view supplies common fields");
    if !matches!(arena.get(fields), Some(Value::Struct(_))) {
        return fields;
    }
    let value = arena.get(value).expect("unified choice exists").clone();
    arena.alloc_with_metadata(
        value,
        ValueMetadata {
            fields,
            view: None,
            source: MetadataSource::ChoiceFields,
        },
    )
}

/// Keep fields already owned by a branch, including its private definitions.
fn attach_branch_fields(
    arena: &mut ValueArena,
    branch: ValueId,
    fields: ValueId,
    context: &mut UnifyContext,
) -> ValueId {
    if arena.metadata(branch).is_none() {
        return finish_with_context(
            arena,
            branch,
            fields,
            vec![Conjunct::Value(branch)],
            context,
        );
    }
    let top = arena.top();
    let common = arena.alloc_with_metadata(
        Value::Top,
        ValueMetadata {
            fields,
            view: None,
            source: MetadataSource::Embedded(vec![Conjunct::Value(top)]),
        },
    );
    unify_with_context(arena, branch, common, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{DisjunctionBranch, StructValue};

    #[test]
    fn normalization_bounds_cycles_and_deep_acyclic_choices() {
        for cyclic in [true, false] {
            let mut arena = ValueArena::new();
            let mut choice = arena.top();
            if cyclic {
                *arena.get_mut(choice).unwrap() = Value::Disjunction {
                    branches: vec![
                        DisjunctionBranch {
                            val: choice,
                            default: false
                        };
                        2
                    ],
                };
            } else {
                for _ in 0..=crate::unify::MAX_DISJUNCTION_DEPTH {
                    choice = arena.alloc(Value::Disjunction {
                        branches: vec![DisjunctionBranch {
                            val: choice,
                            default: false,
                        }],
                    });
                }
            }
            let fields = arena.alloc(Value::Struct(StructValue::default()));
            let mut context = UnifyContext::new();
            let result = finish_with_context(
                &mut arena,
                choice,
                fields,
                vec![Conjunct::Value(choice)],
                &mut context,
            );
            assert!(matches!(arena.get(result), Some(Value::Bottom(_))));
            assert!(context.active_metadata_choices.is_empty());
        }
    }
}
