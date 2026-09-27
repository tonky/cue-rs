//! Builtin registry seed: the top-level type bindings.
//!
//! The 19 `(name, type)` pairs are data, declared once as a table; building
//! them into arena values is a pure function of the arena. The evaluator only
//! inserts the result into scope. Adding a type means adding a table row.

use crate::value::{NumberKind, TypeKind, Value, ValueArena, ValueId};

/// Every top-level type name an evaluator starts with bound.
pub(crate) const TYPE_BUILTINS: &[(&str, TypeKind)] = &[
    ("_", TypeKind::Top),
    ("null", TypeKind::Null),
    ("bool", TypeKind::Bool),
    ("int", TypeKind::Number(NumberKind::Int)),
    ("uint", TypeKind::Number(NumberKind::Uint)),
    ("uint8", TypeKind::Number(NumberKind::Uint8)),
    ("uint16", TypeKind::Number(NumberKind::Uint16)),
    ("uint32", TypeKind::Number(NumberKind::Uint32)),
    ("uint64", TypeKind::Number(NumberKind::Uint64)),
    ("int8", TypeKind::Number(NumberKind::Int8)),
    ("int16", TypeKind::Number(NumberKind::Int16)),
    ("int32", TypeKind::Number(NumberKind::Int32)),
    ("int64", TypeKind::Number(NumberKind::Int64)),
    ("float", TypeKind::Number(NumberKind::Float)),
    ("float32", TypeKind::Number(NumberKind::Float32)),
    ("float64", TypeKind::Number(NumberKind::Float64)),
    ("number", TypeKind::Number(NumberKind::Number)),
    ("string", TypeKind::String),
    ("bytes", TypeKind::Bytes),
];

/// Allocate one value per table row, in table order.
pub(crate) fn type_builtins(arena: &mut ValueArena) -> Vec<(String, ValueId)> {
    TYPE_BUILTINS
        .iter()
        .map(|(name, kind)| (name.to_string(), arena.alloc(Value::Type(*kind))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn table_has_unique_names() {
        let names: HashSet<&&str> = TYPE_BUILTINS.iter().map(|(name, _)| name).collect();
        assert_eq!(names.len(), TYPE_BUILTINS.len());
    }

    #[test]
    fn builds_one_value_per_row() {
        let mut arena = ValueArena::new();
        let built = type_builtins(&mut arena);
        assert_eq!(built.len(), TYPE_BUILTINS.len());
        let int = built.iter().find(|(name, _)| name == "int").unwrap();
        assert_eq!(
            arena.get(int.1),
            Some(&Value::Type(TypeKind::Number(NumberKind::Int)))
        );
    }
}
