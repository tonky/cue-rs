//! Construction state for a declaration block, before its value kind is known.

use crate::closedness::reclose;
use crate::unify::unify;
use crate::value::{Conjunct, StructValue, Value, ValueArena, ValueId};

#[derive(Clone, Default)]
pub(crate) struct DeclarationValue {
    pub structure: StructValue,
    constraint: Option<ValueId>,
    conjuncts: Vec<Conjunct>,
    /// An explicit `{}` embedding constrains the kind even without fields.
    pub has_struct_embedding: bool,
    pub close_on_finish: bool,
}

impl DeclarationValue {
    pub fn constraint(&self) -> Option<ValueId> {
        self.constraint
    }

    pub fn constrain(&mut self, arena: &mut ValueArena, value: ValueId) {
        self.constrain_with(arena, value, vec![Conjunct::Value(value)]);
    }

    pub fn constrain_with(
        &mut self,
        arena: &mut ValueArena,
        value: ValueId,
        conjuncts: Vec<Conjunct>,
    ) {
        self.constraint = Some(match self.constraint {
            Some(previous) => unify(arena, previous, value),
            None => value,
        });
        self.conjuncts.extend(conjuncts);
    }

    pub fn merge_constraints(&mut self, arena: &mut ValueArena, other: &Self, at_file_root: bool) {
        self.has_struct_embedding |= other.has_struct_embedding;
        self.close_on_finish |= other.close_on_finish && !at_file_root;
        if let Some(value) = other.constraint {
            self.constrain_with(arena, value, other.conjuncts.clone());
        }
    }

    pub fn finish(self, arena: &mut ValueArena) -> ValueId {
        let requires_struct = self.has_struct_embedding
            || self.structure.fields.values().any(|field| !field.optional);
        let has_metadata = !self.structure.fields.is_empty()
            || !self.structure.definitions.is_empty()
            || !self.structure.hidden.is_empty()
            || !self.structure.pattern_constraints.is_empty();
        let value = match self.constraint {
            Some(mut value)
                if (requires_struct || has_metadata)
                    && crate::metadata::needs_recipe(arena, &self.conjuncts) =>
            {
                let fields = arena.alloc(Value::Struct(self.structure));
                let mut conjuncts = self.conjuncts;
                if requires_struct {
                    // The declaration's fields are refreshed separately. Only
                    // struct kind is immutable; caching their values here would
                    // freeze expressions that depend on a later override.
                    let kind = arena.alloc(Value::Struct(StructValue::default()));
                    value = unify(arena, value, kind);
                    conjuncts.push(Conjunct::Value(kind));
                }
                crate::metadata::finish(arena, value, fields, conjuncts)
            }
            Some(value) if !requires_struct && has_metadata => {
                let fields = arena.alloc(Value::Struct(self.structure));
                crate::metadata::finish(arena, value, fields, self.conjuncts)
            }
            Some(value) if !requires_struct => value,
            constraint => {
                let structure = arena.alloc(Value::Struct(self.structure));
                match constraint {
                    Some(value) => unify(arena, structure, value),
                    None => structure,
                }
            }
        };
        if self.close_on_finish {
            reclose(arena, value)
        } else {
            value
        }
    }
}
