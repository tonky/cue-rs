//! Builtin registry seed: the top-level type bindings.
//!
//! The 19 `(name, type)` pairs are data, declared once as a table; building
//! them into arena values is a pure function of the arena. The evaluator only
//! inserts the result into scope. Adding a type means adding a table row.

use crate::value::{
    BottomKind, DisjunctionBranch, Imports, NumberKind, TypeKind, Value, ValueArena, ValueId,
};
use cue_syntax::ast::Expr;

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

/// Apply a call to already-evaluated arguments: total function of arena,
/// imports and operands. Failures are bottom values, never errors — an
/// unknown function is a conflict, not a crash.
///
/// The caller evaluates and default-resolves the arguments; see
/// [`Evaluator::eval_call`](crate::eval::Evaluator::eval_call).
pub(crate) fn call(
    arena: &mut ValueArena,
    imports: &Imports,
    func_expr: &Expr,
    evaluated_args: &[ValueId],
) -> ValueId {
    use cue_syntax::ast::Expr as E;

    // Top-level builtins: len(x), or(list), close(x)
    if let E::Ident(name) = func_expr {
        match name.as_str() {
            "div" | "mod" | "quo" | "rem" => {
                return crate::operators::integer_division(arena, name, evaluated_args);
            }
            "len" => {
                if let Some(&arg0) = evaluated_args.first() {
                    match arena.get(arg0) {
                        Some(Value::String(s)) => {
                            return arena.int(s.chars().count() as i64);
                        }
                        Some(Value::Bytes(b)) => return arena.int(b.len() as i64),
                        Some(Value::List { elements, .. }) => {
                            return arena.int(elements.len() as i64);
                        }
                        // An optional field that nothing set is not there.
                        Some(Value::Struct(s)) => {
                            let set = s.fields.values().filter(|e| !e.optional).count();
                            return arena.int(set as i64);
                        }
                        _ => return arena.bottom("len: unsupported type"),
                    }
                }
                return arena.bottom("len requires 1 argument");
            }
            "or" => {
                if let Some(&arg0) = evaluated_args.first()
                    && let Some(Value::Bottom(_)) = arena.get(arg0)
                {
                    return arg0;
                }
                let Some(Value::List { elements, .. }) =
                    evaluated_args.first().and_then(|&a| arena.get(a)).cloned()
                else {
                    return arena.bottom("or requires 1 list argument");
                };
                // Upstream reports this as incomplete, not as a conflict: the
                // list is usually built by a comprehension over fields that a
                // later conjunct has yet to add. Not a reference to wait
                // for either: the definition holding it would never count
                // as evaluated, and neither would anything reading it. The
                // merge that adds the fields re-derives it.
                if elements.is_empty() {
                    return arena.bottom_of(BottomKind::Incomplete, "empty list in call to or");
                }
                let branches = elements
                    .into_iter()
                    .map(|val| DisjunctionBranch {
                        default: false,
                        val,
                    })
                    .collect();
                return arena.alloc(Value::Disjunction { branches });
            }
            "close" => {
                if let Some(&arg0) = evaluated_args.first()
                    && let Some(Value::Struct(s)) = arena.get(arg0)
                {
                    let mut closed = s.clone();
                    closed.is_closed = true;
                    return arena.alloc(Value::Struct(closed));
                }
                return arena.bottom("close requires 1 struct argument");
            }
            _ => {}
        }
    }

    // Builtin functions dispatch: strings.*, math.*, list.*, regexp.*
    if let E::Selector { expr, field } = func_expr
        && let E::Ident(pkg) = &**expr
    {
        match crate::stdlib::call_stdlib_func(arena, imports.path_of(pkg), field, evaluated_args) {
            Ok(res_id) => return res_id,
            Err(err) => return arena.bottom(err),
        }
    }

    arena.bottom("unsupported function call")
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
    fn len_counts_runes_elements_and_set_fields() {
        use cue_syntax::ast::Expr as E;
        let mut arena = ValueArena::new();
        let imports = Imports::default();
        let len = E::Ident("len".to_string());

        let s = arena.string("héllo");
        let id = call(&mut arena, &imports, &len, &[s]);
        assert_eq!(arena.get(id), Some(&Value::Int(5.into())));

        let empty = Vec::new();
        let list = arena.alloc(Value::List {
            elements: empty,
            ellipsis: None,
        });
        let id = call(&mut arena, &imports, &len, &[list]);
        assert_eq!(arena.get(id), Some(&Value::Int(0.into())));

        let id = call(&mut arena, &imports, &len, &[]);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }

    #[test]
    fn or_builds_a_disjunction_and_rejects_non_lists() {
        use cue_syntax::ast::Expr as E;
        let mut arena = ValueArena::new();
        let imports = Imports::default();
        let or = E::Ident("or".to_string());

        let a = arena.int(1);
        let b = arena.int(2);
        let list = arena.alloc(Value::List {
            elements: vec![a, b],
            ellipsis: None,
        });
        let id = call(&mut arena, &imports, &or, &[list]);
        assert!(matches!(arena.get(id), Some(Value::Disjunction { .. })));

        let id = call(&mut arena, &imports, &or, &[a]);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }

    #[test]
    fn unknown_calls_are_bottom_not_errors() {
        use cue_syntax::ast::Expr as E;
        let mut arena = ValueArena::new();
        let imports = Imports::default();
        let bogus = E::Ident("nope".to_string());
        let id = call(&mut arena, &imports, &bogus, &[]);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
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
