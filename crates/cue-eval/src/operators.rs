//! Concrete operand selection and scalar operations, independent of lexical evaluation.
use crate::value::{BottomKind, NumberKind, Value, ValueArena, ValueId};
use cue_syntax::ast::{BinaryOp, UnaryOp};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
use std::collections::HashSet;

pub(crate) fn binary(
    arena: &mut ValueArena,
    op: BinaryOp,
    left: ValueId,
    right: ValueId,
) -> ValueId {
    let left = operand(arena, left);
    let right = operand(arena, right);
    let l_val = match arena.get(left) {
        Some(Value::Bottom(_)) => return left,
        Some(v) => v.clone(),
        None => return arena.bottom("invalid left operand"),
    };
    let r_val = match arena.get(right) {
        Some(Value::Bottom(_)) => return right,
        Some(v) => v.clone(),
        None => return arena.bottom("invalid right operand"),
    };

    if (abstract_value(&l_val) || abstract_value(&r_val)) && accepts(op, kind(&l_val), kind(&r_val))
    {
        return arena.bottom_of(BottomKind::Incomplete, "binary operand is not concrete");
    }
    match (op, l_val, r_val) {
        (op, Value::Int(a), Value::Int(b)) => integer_binary(arena, op, a, b),
        (op, Value::Float(a), Value::Float(b)) => float_binary(arena, op, a, b),
        (op, Value::Int(a), Value::Float(b)) => mixed_binary(arena, op, a, b, false),
        (op, Value::Float(a), Value::Int(b)) => mixed_binary(arena, op, b, a, true),

        (BinaryOp::Add, Value::String(a), Value::String(b)) => arena.string(format!("{a}{b}")),
        (BinaryOp::Mul, Value::String(a), Value::Int(b)) => {
            if let Some(count) = b.to_usize() {
                arena.string(a.repeat(count))
            } else {
                arena.bottom("invalid string repetition factor")
            }
        }
        (BinaryOp::Mul, Value::Int(a), Value::String(b)) => {
            if let Some(count) = a.to_usize() {
                arena.string(b.repeat(count))
            } else {
                arena.bottom("invalid string repetition factor")
            }
        }

        // List Concatenation and Repetition
        (
            BinaryOp::Add,
            Value::List {
                elements: mut e1,
                ellipsis: _,
            },
            Value::List {
                elements: e2,
                ellipsis,
            },
        ) => {
            e1.extend(e2);
            arena.alloc(Value::List {
                elements: e1,
                ellipsis,
            })
        }
        (
            BinaryOp::Mul,
            Value::List {
                elements: e1,
                ellipsis,
            },
            Value::Int(b),
        ) => {
            if let Some(count) = b.to_usize() {
                let mut repeated = Vec::new();
                for _ in 0..count {
                    repeated.extend(e1.clone());
                }
                arena.alloc(Value::List {
                    elements: repeated,
                    ellipsis,
                })
            } else {
                arena.bottom("invalid list repetition factor")
            }
        }

        (BinaryOp::Equal, Value::String(a), Value::String(b)) => arena.bool(a == b),
        (BinaryOp::NotEqual, Value::String(a), Value::String(b)) => arena.bool(a != b),
        (BinaryOp::Equal, Value::Bool(a), Value::Bool(b)) => arena.bool(a == b),
        (BinaryOp::NotEqual, Value::Bool(a), Value::Bool(b)) => arena.bool(a != b),
        (BinaryOp::LogicalAnd, Value::Bool(a), Value::Bool(b)) => arena.bool(a && b),
        (BinaryOp::LogicalOr, Value::Bool(a), Value::Bool(b)) => arena.bool(a || b),

        (op, left, right) if accepts(op, kind(&left), kind(&right)) => {
            arena.bottom("binary operation is not implemented for these operand kinds")
        }
        _ => arena.bottom_of(
            BottomKind::Conflict,
            "invalid operands for binary operation",
        ),
    }
}

/// Select a unique default at a concrete operation boundary. Constraints passed
/// to unification do not go through this function.
pub(crate) fn operand(arena: &mut ValueArena, mut id: ValueId) -> ValueId {
    let mut seen = HashSet::new();
    while let Some(Value::Disjunction { branches }) = arena.get(id) {
        if !seen.insert(id) {
            return arena.bottom_of(BottomKind::Cycle, "cycle while selecting a default operand");
        }
        let mut defaults = branches.iter().filter(|branch| branch.default);
        id = match (defaults.next(), defaults.next()) {
            (Some(branch), None) => branch.val,
            (None, None) if branches.len() == 1 => branches[0].val,
            _ => {
                return arena.bottom_of(
                    BottomKind::Incomplete,
                    "operand has no unique concrete default",
                );
            }
        };
    }
    id
}

pub(crate) fn unary(arena: &mut ValueArena, op: UnaryOp, id: ValueId) -> ValueId {
    let id = operand(arena, id);
    match (op, arena.get(id)) {
        (_, Some(Value::Bottom(_))) => id,
        (UnaryOp::Pos, Some(Value::Int(_) | Value::Float(_))) => id,
        (UnaryOp::Neg, Some(Value::Int(n))) => arena.int(-n),
        (UnaryOp::Neg, Some(Value::Float(n))) => arena.float(-n),
        (UnaryOp::Not, Some(Value::Bool(b))) => arena.bool(!b),
        (_, Some(value))
            if abstract_value(value)
                && (kind(value) == OperandKind::Any
                    || match op {
                        UnaryOp::Pos | UnaryOp::Neg => kind(value) == OperandKind::Number,
                        UnaryOp::Not => kind(value) == OperandKind::Bool,
                        _ => false,
                    }) =>
        {
            arena.bottom_of(BottomKind::Incomplete, "unary operand is not concrete")
        }
        _ => arena.bottom_of(BottomKind::Conflict, "invalid unary operand"),
    }
}

pub(crate) fn integer_division(arena: &mut ValueArena, name: &str, args: &[ValueId]) -> ValueId {
    let [left, right] = args else {
        return arena.bottom_of(
            BottomKind::Conflict,
            format!("{name} requires two integer arguments"),
        );
    };
    for &id in args {
        if matches!(arena.get(id), Some(Value::Bottom(_))) {
            return id;
        }
    }
    let incomplete_integer = |id| {
        matches!(arena.get(id), Some(Value::Top | Value::Bounds { .. }))
            || matches!(arena.get(id), Some(Value::Type(t)) if t.is_integer() || *t == crate::value::TypeKind::Number(NumberKind::Number))
    };
    let integer_candidate =
        |id| incomplete_integer(id) || matches!(arena.get(id), Some(Value::Int(_)));
    if integer_candidate(*left)
        && integer_candidate(*right)
        && (incomplete_integer(*left) || incomplete_integer(*right))
    {
        return arena.bottom_of(BottomKind::Incomplete, "integer argument is not concrete");
    }
    let (Some(Value::Int(a)), Some(Value::Int(b))) = (arena.get(*left), arena.get(*right)) else {
        return arena.bottom_of(
            BottomKind::Conflict,
            format!("{name} requires two integer arguments"),
        );
    };
    if b.is_zero() {
        return arena.bottom_of(BottomKind::Conflict, "division by zero");
    }
    let result = match name {
        "quo" => a / b,
        "rem" => a % b,
        // Euclidean remainder is nonnegative, regardless of divisor sign.
        "div" | "mod" => {
            let mut remainder = a % b;
            if remainder.is_negative() {
                remainder += b.abs();
            }
            if name == "mod" {
                remainder
            } else {
                (a - remainder) / b
            }
        }
        _ => unreachable!("integer division dispatch"),
    };
    arena.int(result)
}

fn comparison(op: BinaryOp, order: Ordering) -> Option<bool> {
    Some(match op {
        BinaryOp::Equal => order == Ordering::Equal,
        BinaryOp::NotEqual => order != Ordering::Equal,
        BinaryOp::Less => order == Ordering::Less,
        BinaryOp::LessEqual => order != Ordering::Greater,
        BinaryOp::Greater => order == Ordering::Greater,
        BinaryOp::GreaterEqual => order != Ordering::Less,
        _ => return None,
    })
}

fn integer_binary(arena: &mut ValueArena, op: BinaryOp, a: BigInt, b: BigInt) -> ValueId {
    if let Some(result) = comparison(op, a.cmp(&b)) {
        return arena.bool(result);
    }
    match op {
        BinaryOp::Add => arena.int(a + b),
        BinaryOp::Sub => arena.int(a - b),
        BinaryOp::Mul => arena.int(a * b),
        BinaryOp::Div => {
            if b.is_zero() {
                return division_by_zero(arena, a.is_zero());
            }
            match (finite_integer(&a), finite_integer(&b)) {
                (Some(a), Some(b)) => float_binary(arena, op, a, b),
                _ => numeric_range(arena),
            }
        }
        _ => arena.bottom_of(BottomKind::Conflict, "invalid numeric operation"),
    }
}

fn finite_integer(n: &BigInt) -> Option<f64> {
    n.to_f64().filter(|n| n.is_finite())
}

fn numeric_range(arena: &mut ValueArena) -> ValueId {
    arena.bottom("number exceeds the evaluator's floating-point range")
}

fn division_by_zero(arena: &mut ValueArena, numerator_is_zero: bool) -> ValueId {
    arena.bottom_of(
        BottomKind::Conflict,
        if numerator_is_zero {
            "division undefined"
        } else {
            "division by zero"
        },
    )
}

fn float_binary(arena: &mut ValueArena, op: BinaryOp, a: f64, b: f64) -> ValueId {
    if !a.is_finite() || !b.is_finite() {
        return numeric_range(arena);
    }
    if let Some(result) = comparison(op, a.partial_cmp(&b).expect("finite operands")) {
        return arena.bool(result);
    }
    let value = match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => {
            if b == 0.0 {
                return division_by_zero(arena, a == 0.0);
            }
            a / b
        }
        _ => return arena.bottom_of(BottomKind::Conflict, "invalid numeric operation"),
    };
    if value.is_finite() {
        arena.float(value)
    } else {
        numeric_range(arena)
    }
}

fn mixed_binary(
    arena: &mut ValueArena,
    op: BinaryOp,
    integer: BigInt,
    float: f64,
    reversed: bool,
) -> ValueId {
    let Some(truncated) = BigInt::from_f64(float) else {
        return numeric_range(arena);
    };
    let order = integer.cmp(&truncated).then_with(|| {
        // BigInt conversion truncates towards zero. Compare the fractional
        // remainder only when the integer equals that truncation.
        if float.fract() > 0.0 {
            Ordering::Less
        } else if float.fract() < 0.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    if let Some(result) = comparison(op, if reversed { order.reverse() } else { order }) {
        return arena.bool(result);
    }
    let Some(integer) = finite_integer(&integer) else {
        return numeric_range(arena);
    };
    if reversed {
        float_binary(arena, op, float, integer)
    } else {
        float_binary(arena, op, integer, float)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OperandKind {
    Any,
    Number,
    String,
    Bytes,
    Bool,
    List,
    Struct,
    Null,
}

fn kind(value: &Value) -> OperandKind {
    use crate::value::TypeKind;
    match value {
        Value::Int(_) | Value::Float(_) => OperandKind::Number,
        Value::String(_) => OperandKind::String,
        Value::Bytes(_) => OperandKind::Bytes,
        Value::Bool(_) => OperandKind::Bool,
        Value::List { .. } => OperandKind::List,
        Value::Struct(_) => OperandKind::Struct,
        Value::Null => OperandKind::Null,
        Value::Type(t) => match t {
            TypeKind::String => OperandKind::String,
            TypeKind::Bytes => OperandKind::Bytes,
            TypeKind::Bool => OperandKind::Bool,
            TypeKind::List => OperandKind::List,
            TypeKind::Struct => OperandKind::Struct,
            TypeKind::Null => OperandKind::Null,
            TypeKind::Number(NumberKind::Number) => OperandKind::Number,
            t if t.is_integer() || t.is_float() => OperandKind::Number,
            _ => OperandKind::Any,
        },
        _ => OperandKind::Any,
    }
}

fn accepts(op: BinaryOp, left: OperandKind, right: OperandKind) -> bool {
    use OperandKind::*;
    let pair = |a, b| (left == Any || left == a) && (right == Any || right == b);
    match op {
        BinaryOp::Add => {
            pair(Number, Number) || pair(String, String) || pair(Bytes, Bytes) || pair(List, List)
        }
        BinaryOp::Sub | BinaryOp::Div => pair(Number, Number),
        BinaryOp::Mul => {
            pair(Number, Number)
                || pair(String, Number)
                || pair(Number, String)
                || pair(Bytes, Number)
                || pair(Number, Bytes)
                || pair(List, Number)
        }
        BinaryOp::Equal | BinaryOp::NotEqual => left == Any || right == Any || left == right,
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            pair(Number, Number) || pair(String, String) || pair(Bytes, Bytes)
        }
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr => pair(Bool, Bool),
        BinaryOp::RegexMatch | BinaryOp::RegexNotMatch => {
            pair(String, String) || pair(Bytes, Bytes)
        }
        _ => false,
    }
}

fn abstract_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Top | Value::Type(_) | Value::Bounds { .. } | Value::RecursiveRef { .. }
    )
}

/// Resolve an already-evaluated selector base: total function of the arena.
/// A definition read before its declaration was evaluated has no fields
/// *yet*; the relaxation loop retries those.
pub(crate) fn select(
    arena: &mut ValueArena,
    closed: &mut crate::closedness::ClosedCopies,
    reading_root_embedding: bool,
    base_id: ValueId,
    field: &str,
) -> ValueId {
    if arena.fields(base_id).is_none()
        && let Some(Value::Bottom(_)) = arena.get(base_id)
    {
        return base_id;
    }
    // A definition read before its declaration was evaluated has no
    // fields *yet*.
    if let Some(Value::RecursiveRef { name, target: None }) = arena.get(base_id) {
        let message = format!("{name} not evaluated yet");
        return arena.bottom_of(BottomKind::Unresolved, message);
    }
    if let Some(s) = arena.fields(base_id) {
        if let Some(f) = s
            .fields
            .get(field)
            .or_else(|| s.definitions.get(field))
            .or_else(|| s.hidden.get(field))
        {
            let val = f.val;
            crate::closedness::read_definition(arena, closed, reading_root_embedding, field, val)
        } else {
            // The base resolved and has no such field. It may still
            // gain one on a later pass, which is why this kind is
            // one the relaxation loop retries.
            arena.bottom_of(
                BottomKind::UndefinedField,
                format!("undefined field: {field}"),
            )
        }
    } else {
        arena.bottom("selector on non-struct")
    }
}

/// Resolve already-evaluated index operands: total function of the arena.
/// Bottom operands pass through; anything else out of shape is a conflict.
pub(crate) fn index(arena: &mut ValueArena, target_id: ValueId, index_id: ValueId) -> ValueId {
    if let Some(Value::Bottom(_)) = arena.get(target_id) {
        return target_id;
    }
    if let Some(Value::Bottom(_)) = arena.get(index_id) {
        return index_id;
    }

    match (arena.get(target_id), arena.get(index_id)) {
        (Some(Value::List { elements, .. }), Some(Value::Int(i))) => {
            if let Some(idx) = i.to_usize() {
                if let Some(&elem) = elements.get(idx) {
                    elem
                } else {
                    let len = elements.len();
                    arena.bottom_of(
                        BottomKind::Conflict,
                        format!("list index {idx} out of bounds (len: {len})"),
                    )
                }
            } else {
                arena.bottom_of(BottomKind::Conflict, "invalid list index")
            }
        }
        (Some(Value::Struct(s)), Some(Value::String(key))) => {
            if let Some(f) = s.fields.get(key).or_else(|| s.definitions.get(key)) {
                f.val
            } else {
                arena.bottom_of(
                    BottomKind::UndefinedField,
                    format!("undefined field: {key}"),
                )
            }
        }
        _ => arena.bottom("indexing unsupported on target"),
    }
}

/// Slice an already-evaluated list between already-evaluated bounds:
/// total function of the arena. A non-integer bound falls back to its end.
pub(crate) fn slice(
    arena: &mut ValueArena,
    target_id: ValueId,
    low_id: Option<ValueId>,
    high_id: Option<ValueId>,
) -> ValueId {
    let Some(Value::List { elements, ellipsis }) = arena.get(target_id).cloned() else {
        return arena.bottom("slice unsupported on non-list");
    };
    let len = elements.len();
    let bound = |id: Option<ValueId>, default: usize| match id.and_then(|id| arena.get(id)) {
        Some(Value::Int(i)) => i.to_usize().unwrap_or(default).min(len),
        _ => default,
    };
    let start = bound(low_id, 0);
    let end = bound(high_id, len);

    if start <= end {
        let sliced = elements[start..end].to_vec();
        arena.alloc(Value::List {
            elements: sliced,
            ellipsis,
        })
    } else {
        arena.bottom(format!("invalid slice range: [{start}:{end}]"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closedness::ClosedCopies;
    use crate::value::StructValue;

    fn list_of(arena: &mut ValueArena, n: i64) -> ValueId {
        let elements: Vec<ValueId> = (0..n).map(|i| arena.int(i)).collect();
        arena.alloc(Value::List {
            elements,
            ellipsis: None,
        })
    }

    #[test]
    fn index_resolves_and_reports_bounds() {
        let mut arena = ValueArena::new();
        let list = list_of(&mut arena, 3);
        let two = arena.int(2);
        let id = index(&mut arena, list, two);
        assert_eq!(arena.get(id), arena.get(two));

        let nine = arena.int(9);
        let id = index(&mut arena, list, nine);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        let s = arena.string("x");
        let id = index(&mut arena, list, s);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }

    #[test]
    fn slice_clamps_and_rejects_reversed_ranges() {
        let mut arena = ValueArena::new();
        let list = list_of(&mut arena, 4);
        let one = arena.int(1);
        let three = arena.int(3);

        let id = slice(&mut arena, list, Some(one), Some(three));
        match arena.get(id) {
            Some(Value::List { elements, .. }) => assert_eq!(elements.len(), 2),
            other => panic!("expected sliced list, got {other:?}"),
        }

        let id = slice(&mut arena, list, Some(three), Some(one));
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        let not_list = arena.int(1);
        let id = slice(&mut arena, not_list, None, None);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }

    #[test]
    fn select_finds_fields_and_reports_missing() {
        let mut arena = ValueArena::new();
        let mut closed = ClosedCopies::default();
        let val = arena.int(7);
        let mut s = StructValue::new(false);
        s.insert_field("a".to_string(), val, false);
        let base = arena.alloc(Value::Struct(s));

        let id = select(&mut arena, &mut closed, false, base, "a");
        assert_eq!(id, val);

        let id = select(&mut arena, &mut closed, false, base, "missing");
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }
}
