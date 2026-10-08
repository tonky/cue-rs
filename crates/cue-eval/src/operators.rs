//! Concrete operand selection and scalar operations, independent of lexical evaluation.
use crate::value::{BottomKind, NumberKind, Value, ValueArena, ValueId};
use cue_syntax::ast::{BinaryOp, SelectorField, UnaryOp};
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

        // Removed from the language in v0.11: upstream rejects both forms with
        // a pointer to the builtin that replaces them.
        (BinaryOp::Add, Value::List { .. }, Value::List { .. }) => arena.bottom_of(
            BottomKind::Conflict,
            "Addition of lists is superseded by list.Concat; see https://cuelang.org/e/v0.11-list-arithmetic",
        ),
        (BinaryOp::Mul, Value::List { .. }, Value::Int(_))
        | (BinaryOp::Mul, Value::Int(_), Value::List { .. }) => arena.bottom_of(
            BottomKind::Conflict,
            "Multiplication of lists is superseded by list.Repeat; see https://cuelang.org/e/v0.11-list-arithmetic",
        ),

        (BinaryOp::Add, Value::Bytes(mut a), Value::Bytes(b)) => {
            a.extend(b);
            arena.alloc(Value::Bytes(a))
        }
        (op, Value::String(a), Value::String(b)) if is_ordering(op) => {
            arena.bool(comparison(op, a.cmp(&b)).unwrap_or(false))
        }
        (op, Value::Bytes(a), Value::Bytes(b)) if is_ordering(op) => {
            arena.bool(comparison(op, a.cmp(&b)).unwrap_or(false))
        }
        // The pattern may be given as bytes; the subject must be a string.
        (op @ (BinaryOp::RegexMatch | BinaryOp::RegexNotMatch), Value::String(s), Value::Bytes(pattern)) => {
            match String::from_utf8(pattern) {
                Ok(pattern) => regex_match(arena, op, &s, &pattern),
                Err(_) => arena.bottom_of(BottomKind::Conflict, "regular expression is not valid UTF-8"),
            }
        }
        (
            op @ (BinaryOp::RegexMatch | BinaryOp::RegexNotMatch),
            Value::String(s),
            Value::String(pattern),
        ) => regex_match(arena, op, &s, &pattern),
        (BinaryOp::LogicalAnd, Value::Bool(a), Value::Bool(b)) => arena.bool(a && b),
        (BinaryOp::LogicalOr, Value::Bool(a), Value::Bool(b)) => arena.bool(a || b),
        (op @ (BinaryOp::Equal | BinaryOp::NotEqual), left, right) => {
            match equal(arena, &left, &right) {
                Ok(same) => arena.bool(same == (op == BinaryOp::Equal)),
                Err(bottom) => bottom,
            }
        }

        (op, left, right) if accepts(op, kind(&left), kind(&right)) => {
            arena.bottom("binary operation is not implemented for these operand kinds")
        }
        _ => arena.bottom_of(
            BottomKind::Conflict,
            "invalid operands for binary operation",
        ),
    }
}

/// `==` over two operands whose defaults are already selected. Values of
/// different kinds are unequal, as upstream has it (`1 == "a"` is `false`);
/// lists and structs compare element by element, selecting defaults inside
/// them the same way. A value that is not concrete has no answer yet.
fn equal(arena: &mut ValueArena, left: &Value, right: &Value) -> Result<bool, ValueId> {
    let incomplete = |arena: &mut ValueArena| {
        Err(arena.bottom_of(BottomKind::Incomplete, "comparison operand is not concrete"))
    };
    if !is_concrete_kind(left) || !is_concrete_kind(right) {
        return incomplete(arena);
    }
    // Upstream checks both sides all the way down before comparing, so
    // `[int] == null` is incomplete rather than `false`.
    deep_concrete(arena, left)?;
    deep_concrete(arena, right)?;
    match (left, right) {
        (Value::Null, Value::Null) => Ok(true),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::String(a), Value::String(b)) => Ok(a == b),
        (Value::Bytes(a), Value::Bytes(b)) => Ok(a == b),
        (Value::List { elements: a, .. }, Value::List { elements: b, .. }) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (&x, &y) in a.clone().iter().zip(b.clone().iter()) {
                if !element_equal(arena, x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Struct(a), Value::Struct(b)) => {
            let regular = |s: &crate::value::StructValue| -> Vec<(String, ValueId)> {
                s.fields
                    .iter()
                    .filter(|(_, entry)| !entry.optional)
                    .map(|(name, entry)| (name.clone(), entry.val))
                    .collect()
            };
            // Field order is arc order, not part of the value:
            // `{a: 1, b: 2} == {b: 2, a: 1}`.
            let (a, b) = (regular(a), regular(b));
            if a.len() != b.len() {
                return Ok(false);
            }
            let b: std::collections::HashMap<String, ValueId> = b.into_iter().collect();
            let mut pairs = Vec::with_capacity(a.len());
            for (name, x) in a {
                match b.get(&name) {
                    Some(&y) => pairs.push((x, y)),
                    None => return Ok(false),
                }
            }
            for (x, y) in pairs {
                if !element_equal(arena, x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Fail with the first value inside `value` (list elements, regular fields)
/// that is bottom or not concrete. Optional, hidden and definition fields do
/// not take part in equality, so they are not checked.
fn deep_concrete(arena: &mut ValueArena, value: &Value) -> Result<(), ValueId> {
    let children: Vec<ValueId> = match value {
        Value::List { elements, .. } => elements.clone(),
        Value::Struct(s) => s
            .fields
            .values()
            .filter(|entry| !entry.optional)
            .map(|entry| entry.val)
            .collect(),
        _ => return Ok(()),
    };
    for child in children {
        let child = operand(arena, child);
        let value = match arena.get(child) {
            Some(Value::Bottom(_)) => return Err(child),
            Some(value) => value.clone(),
            None => return Err(arena.bottom("invalid comparison operand")),
        };
        if !is_concrete_kind(&value) {
            return Err(arena.bottom_of(
                BottomKind::Incomplete,
                format!(
                    "comparison operand is not concrete: incomplete value {}",
                    kind_name(&value)
                ),
            ));
        }
        deep_concrete(arena, &value)?;
    }
    Ok(())
}

/// `a == b` as the language defines it: defaults selected, lists and structs
/// compared by content. `Err` holds the bottom when either side is not
/// concrete.
pub(crate) fn element_equal(
    arena: &mut ValueArena,
    a: ValueId,
    b: ValueId,
) -> Result<bool, ValueId> {
    let result = binary(arena, BinaryOp::Equal, a, b);
    match arena.get(result) {
        Some(Value::Bool(same)) => Ok(*same),
        _ => Err(result),
    }
}

/// A value an operation can read as it is: a scalar, a list or a struct.
fn is_concrete_kind(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Bytes(_)
            | Value::List { .. }
    ) || matches!(value, Value::Struct(s) if s.incomplete().is_none())
}

/// What a position that needs a concrete value - an `if` guard, a `for`
/// source, a dynamic label, an index - read, with its default selected.
pub(crate) enum Concrete {
    /// A scalar, a list or a struct.
    Value(ValueId),
    /// A reference the relaxation loop may still resolve.
    Pending(String),
    /// Resolved, but not concrete enough to decide: `bool`, `string`, a
    /// disjunction without a unique default. Unifying it further may decide it.
    Incomplete(String),
    /// A definite error.
    Error(String),
}

pub(crate) fn concrete(arena: &mut ValueArena, id: ValueId) -> Concrete {
    let id = operand(arena, id);
    match arena.get(id) {
        Some(Value::Bottom(reason)) if reason.kind.may_resolve_later() => {
            Concrete::Pending(reason.message.clone())
        }
        Some(Value::Bottom(reason)) if reason.kind == BottomKind::Incomplete => {
            Concrete::Incomplete(reason.message.clone())
        }
        Some(Value::Bottom(reason)) => Concrete::Error(reason.message.clone()),
        Some(Value::RecursiveRef { name, .. }) => {
            Concrete::Pending(format!("{name} not evaluated yet"))
        }
        Some(value) if is_concrete_kind(value) => Concrete::Value(id),
        Some(value) => Concrete::Incomplete(format!("incomplete value {}", kind_name(value))),
        None => Concrete::Error("invalid value".to_string()),
    }
}

/// The kind upstream names in diagnostics: `int`, `string`, `struct`.
pub(crate) fn kind_name(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Int(_) => "int".into(),
        Value::Float(_) => "float".into(),
        Value::String(_) => "string".into(),
        Value::Bytes(_) => "bytes".into(),
        Value::List { .. } => "list".into(),
        Value::Struct(_) => "struct".into(),
        Value::Type(t) => t.to_string(),
        Value::Top => "_".into(),
        Value::Bounds {
            base_type: Some(t), ..
        } => t.to_string(),
        Value::Bounds { .. } => "number".into(),
        Value::Disjunction { .. } => "disjunction".into(),
        Value::BuiltinValidator { name, .. } => name.clone(),
        Value::Validators(_) => "validator".into(),
        Value::RecursiveRef { name, .. } => name.clone(),
        Value::Bottom(_) => "_|_".into(),
    }
}

/// Select a unique default at a concrete operation boundary. Constraints passed
/// to unification do not go through this function.
pub(crate) fn operand(arena: &mut ValueArena, mut id: ValueId) -> ValueId {
    if let Some(partial) = arena.provisional(id) {
        id = partial;
    }
    let mut seen = HashSet::new();
    while let Some(Value::Disjunction { branches }) = arena.get(id) {
        if !seen.insert(id) {
            return arena.bottom_of(BottomKind::Cycle, "cycle while selecting a default operand");
        }
        let mut defaults = branches.iter().filter(|branch| branch.default);
        let alternatives = branches.len();
        id = match (defaults.next(), defaults.next()) {
            (Some(branch), None) => {
                let chosen = branch.val;
                if alternatives > 1 {
                    arena.note_default_chosen();
                }
                chosen
            }
            (None, None) if branches.len() == 1 => branches[0].val,
            _ => {
                return arena.bottom_of(
                    BottomKind::Incomplete,
                    "unresolved disjunction: no unique default",
                );
            }
        };
    }
    id
}

/// A builtin argument: its default, and the defaults of list elements all the
/// way down, so `list.Concat([l])` reads `l: *[1] | [...int]` as `[1]`. An
/// element without a unique default is passed on as it is for the builtin
/// to judge.
pub(crate) fn argument(arena: &mut ValueArena, id: ValueId) -> ValueId {
    let id = operand(arena, id);
    let Some(Value::List { elements, ellipsis }) = arena.get(id) else {
        return id;
    };
    let (elements, ellipsis) = (elements.clone(), *ellipsis);
    let mut changed = false;
    let resolved: Vec<ValueId> = elements
        .iter()
        .map(|&element| {
            let chosen = argument(arena, element);
            if chosen == element || matches!(arena.get(chosen), Some(Value::Bottom(_))) {
                element
            } else {
                changed = true;
                chosen
            }
        })
        .collect();
    if changed {
        arena.alloc(Value::List {
            elements: resolved,
            ellipsis,
        })
    } else {
        id
    }
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

fn is_ordering(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual
    )
}

fn regex_match(arena: &mut ValueArena, op: BinaryOp, subject: &str, pattern: &str) -> ValueId {
    match crate::unify::cached_regex(pattern) {
        Some(re) => arena.bool(re.is_match(subject) == (op == BinaryOp::RegexMatch)),
        None => arena.bottom_of(
            BottomKind::Conflict,
            format!("error parsing regexp: invalid pattern {pattern:?}"),
        ),
    }
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
        BinaryOp::Add => pair(Number, Number) || pair(String, String) || pair(Bytes, Bytes),
        BinaryOp::Sub | BinaryOp::Div => pair(Number, Number),
        BinaryOp::Mul => {
            pair(Number, Number)
                || pair(String, Number)
                || pair(Number, String)
                || pair(Bytes, Number)
                || pair(Number, Bytes)
        }
        BinaryOp::Equal | BinaryOp::NotEqual => left == Any || right == Any || left == right,
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            pair(Number, Number) || pair(String, String) || pair(Bytes, Bytes)
        }
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr => pair(Bool, Bool),
        BinaryOp::RegexMatch | BinaryOp::RegexNotMatch => {
            pair(String, String) || pair(String, Bytes)
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

/// An interpolation operand that is not concrete yet but may still become a
/// string, number, bool or bytes: incomplete, like an abstract arithmetic
/// operand, so a later merge can supply the value.
pub(crate) fn incomplete_interpolation_operand(value: &Value) -> bool {
    use OperandKind::*;
    abstract_value(value) && matches!(kind(value), Any | Number | String | Bytes | Bool)
}

/// Resolve an already-evaluated selector base: total function of the arena.
/// A definition read before its declaration was evaluated has no fields
/// *yet*; the relaxation loop retries those.
pub(crate) fn select(
    arena: &mut ValueArena,
    closed: &mut crate::closedness::ClosedCopies,
    reading_root_embedding: bool,
    in_definition: bool,
    base_id: ValueId,
    field: &SelectorField,
) -> ValueId {
    let base_id = arena.provisional(base_id).unwrap_or(base_id);
    // A selector reads the default of a choice. A choice with no unique
    // default can still answer for the fields its alternatives share.
    let base_id = match arena.get(base_id) {
        Some(Value::Disjunction { .. }) => {
            let chosen = operand(arena, base_id);
            match arena.get(chosen) {
                Some(Value::Bottom(_)) if arena.fields(base_id).is_some() => base_id,
                _ => chosen,
            }
        }
        _ => base_id,
    };
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
        // The spelling picks the map: `a.#e` never answers for a field
        // `"#e"`, nor `a."#e"` for the definition.
        let section = crate::schedule::Section::of_selector(field);
        if let Some(f) = section.map(s).get(field.name()) {
            let val = f.val;
            match section {
                crate::schedule::Section::Definition => crate::closedness::read_definition(
                    arena,
                    closed,
                    reading_root_embedding,
                    field.name(),
                    val,
                ),
                _ => val,
            }
        } else {
            // The base resolved and has no such field. It may still
            // gain one on a later pass, which is why both kinds below
            // are ones the relaxation loop retries. A closed base (read
            // through a definition) or a select under a definition is
            // decided: survivors past the fixpoint observe as `eval`,
            // where an open-world miss stays `incomplete`.
            let kind = if s.is_closed || in_definition {
                BottomKind::UndefinedFieldDefinite
            } else {
                BottomKind::UndefinedField
            };
            arena.bottom_of(kind, format!("undefined field: {field}"))
        }
    } else if matches!(arena.get(base_id), Some(Value::List { .. })) {
        // A list has no fields to gain later: definite error, like an
        // out-of-bounds index. Other non-struct bases stay untyped.
        arena.bottom_of(BottomKind::Conflict, format!("undefined field: {field}"))
    } else {
        arena.bottom("selector on non-struct")
    }
}

/// Resolve already-evaluated index operands: total function of the arena.
/// Bottom operands pass through; anything else out of shape is a conflict.
pub(crate) fn index(
    arena: &mut ValueArena,
    in_definition: bool,
    target_id: ValueId,
    index_id: ValueId,
) -> ValueId {
    let target_id = operand(arena, target_id);
    let index_id = operand(arena, index_id);
    if let Some(Value::Bottom(_)) = arena.get(target_id) {
        return target_id;
    }
    // As for a selector: a definition not evaluated yet has no fields *yet*.
    if let Some(Value::RecursiveRef { name, target: None }) = arena.get(target_id) {
        let message = format!("{name} not evaluated yet");
        return arena.bottom_of(BottomKind::Unresolved, message);
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
                let kind = if s.is_closed || in_definition {
                    BottomKind::UndefinedFieldDefinite
                } else {
                    BottomKind::UndefinedField
                };
                arena.bottom_of(kind, format!("undefined field: {key}"))
            }
        }
        (Some(Value::Struct(_)), Some(Value::Int(i))) => arena.bottom_of(
            BottomKind::Conflict,
            format!("invalid index {i} (found struct, want list)"),
        ),
        (Some(target), Some(index)) if !is_concrete_kind(target) || !is_concrete_kind(index) => {
            let message = format!(
                "invalid non-ground value {} (must be concrete {})",
                kind_name(if is_concrete_kind(target) {
                    index
                } else {
                    target
                }),
                if is_concrete_kind(target) {
                    "int or string"
                } else {
                    "list or struct"
                },
            );
            arena.bottom_of(BottomKind::Incomplete, message)
        }
        _ => arena.bottom("indexing unsupported on target"),
    }
}

/// Slice an already-evaluated list between already-evaluated bounds:
/// total function of the arena. An omitted bound falls back to its end;
/// a present bound must be an integer in `[0, len]`. Upstream rejects
/// negative, out-of-range, and mistyped bounds instead of clamping them.
pub(crate) fn slice(
    arena: &mut ValueArena,
    target_id: ValueId,
    low_id: Option<ValueId>,
    high_id: Option<ValueId>,
) -> ValueId {
    let target_id = operand(arena, target_id);
    let low_id = low_id.map(|id| operand(arena, id));
    let high_id = high_id.map(|id| operand(arena, id));
    if let Some(Value::Bottom(_)) = arena.get(target_id) {
        return target_id;
    }
    if let Some(id) = [low_id, high_id]
        .into_iter()
        .flatten()
        .find(|id| matches!(arena.get(*id), Some(Value::Bottom(_))))
    {
        return id;
    }
    let Some(Value::List { elements, ellipsis }) = arena.get(target_id).cloned() else {
        return arena.bottom_of(BottomKind::Conflict, "slice unsupported on non-list");
    };
    let len = elements.len();
    let resolve =
        |arena: &ValueArena, id: Option<ValueId>, default: usize| -> Result<usize, String> {
            let Some(id) = id else {
                return Ok(default);
            };
            match arena.get(id) {
                Some(Value::Int(i)) => match i.to_usize() {
                    Some(n) if n <= len => Ok(n),
                    Some(n) => Err(format!("index {n} out of range (len: {len})")),
                    None => Err("cannot convert negative number to uint64".to_string()),
                },
                Some(Value::String(s)) => Err(format!(
                    "cannot use {s:?} (type string) as type int in slice index"
                )),
                _ => Err("invalid slice bound".to_string()),
            }
        };
    let (start, end) = match (resolve(arena, low_id, 0), resolve(arena, high_id, len)) {
        (Ok(start), Ok(end)) => (start, end),
        (Err(message), _) | (_, Err(message)) => {
            return arena.bottom_of(BottomKind::Conflict, message);
        }
    };

    if start <= end {
        let sliced = elements[start..end].to_vec();
        arena.alloc(Value::List {
            elements: sliced,
            ellipsis,
        })
    } else {
        arena.bottom_of(
            BottomKind::Conflict,
            format!("invalid slice index: {start} > {end}"),
        )
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
        let id = index(&mut arena, false, list, two);
        assert_eq!(arena.get(id), arena.get(two));

        let nine = arena.int(9);
        let id = index(&mut arena, false, list, nine);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        let s = arena.string("x");
        let id = index(&mut arena, false, list, s);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));
    }

    #[test]
    fn slice_rejects_bad_bounds_and_reversed_ranges() {
        let mut arena = ValueArena::new();
        let list = list_of(&mut arena, 4);
        let one = arena.int(1);
        let three = arena.int(3);

        let id = slice(&mut arena, list, Some(one), Some(three));
        match arena.get(id) {
            Some(Value::List { elements, .. }) => assert_eq!(elements.len(), 2),
            other => panic!("expected sliced list, got {other:?}"),
        }

        // Bounds at exactly `len` are valid (`[0][1:1] == []` upstream).
        let singleton = list_of(&mut arena, 1);
        let id = slice(&mut arena, singleton, Some(one), Some(one));
        match arena.get(id) {
            Some(Value::List { elements, .. }) => assert!(elements.is_empty()),
            other => panic!("expected empty slice, got {other:?}"),
        }

        let id = slice(&mut arena, list, Some(three), Some(one));
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        // Upstream rejects rather than clamps: negative, out-of-range,
        // and mistyped bounds are all `eval` errors.
        let neg = arena.int(-1);
        let id = slice(&mut arena, list, Some(neg), None);
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        let nine = arena.int(9);
        let id = slice(&mut arena, list, Some(one), Some(nine));
        assert!(matches!(arena.get(id), Some(Value::Bottom(_))));

        let s = arena.string("");
        let id = slice(&mut arena, list, Some(s), None);
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
        let base = arena.alloc(Value::Struct(Box::new(s)));

        let id = select(
            &mut arena,
            &mut closed,
            false,
            false,
            base,
            &SelectorField::Ident("a".into()),
        );
        assert_eq!(id, val);

        let id = select(
            &mut arena,
            &mut closed,
            false,
            false,
            base,
            &SelectorField::Ident("missing".into()),
        );
        assert!(matches!(
            arena.get(id),
            Some(Value::Bottom(reason)) if reason.kind == BottomKind::UndefinedField
        ));

        // A closed base decides the miss, like a read through a definition.
        let mut s = StructValue::new(true);
        s.insert_field("a".to_string(), val, false);
        let base = arena.alloc(Value::Struct(Box::new(s)));
        let id = select(
            &mut arena,
            &mut closed,
            false,
            false,
            base,
            &SelectorField::Ident("missing".into()),
        );
        assert!(matches!(
            arena.get(id),
            Some(Value::Bottom(reason)) if reason.kind == BottomKind::UndefinedFieldDefinite
        ));
    }
}
