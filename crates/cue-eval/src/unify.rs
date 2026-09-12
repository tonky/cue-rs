use crate::value::*;
use num_traits::ToPrimitive;
use regex::Regex;
use std::collections::BTreeSet;

/// Maximum nesting depth for struct unification before failing with a cycle error.
pub const MAX_STRUCT_DEPTH: usize = 64;

/// Maximum nesting depth for disjunction unification before failing with a cycle error.
pub const MAX_DISJUNCTION_DEPTH: usize = 64;

/// Maximum total recursion depth for unification before failing with a cycle error.
pub const MAX_TOTAL_DEPTH: usize = 128;

/// Tracking context for cycle detection and recursion depth guarding in unification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnifyContext {
    pub struct_depth: usize,
    pub disjunction_depth: usize,
    pub total_depth: usize,
    pub active_pairs: BTreeSet<(ValueId, ValueId)>,
    pub active_structs: BTreeSet<(ValueId, ValueId)>,
    pub active_disjunctions: BTreeSet<(ValueId, ValueId)>,
}

impl UnifyContext {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Unify two values in the arena, computing their greatest lower bound (meet: a ⊓ b).
pub fn unify(arena: &mut ValueArena, v1_id: ValueId, v2_id: ValueId) -> ValueId {
    let mut ctx = UnifyContext::new();
    unify_with_context(arena, v1_id, v2_id, &mut ctx)
}

/// Unify two values with an explicit unification context for recursion and cycle guarding.
pub fn unify_with_context(
    arena: &mut ValueArena,
    v1_id: ValueId,
    v2_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    unify_internal(arena, v1_id, v2_id, ctx)
}

fn unify_internal(
    arena: &mut ValueArena,
    v1_id: ValueId,
    v2_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    if v1_id == v2_id {
        return v1_id;
    }

    if ctx.total_depth >= MAX_TOTAL_DEPTH {
        return arena.bottom("cycle error: unification recursion depth limit exceeded");
    }

    let pair = (v1_id.min(v2_id), v1_id.max(v2_id));
    if !ctx.active_pairs.insert(pair) {
        return arena.bottom("cycle error: cyclic dependency between values detected");
    }
    ctx.total_depth += 1;

    let res = unify_logic(arena, v1_id, v2_id, ctx);

    ctx.total_depth = ctx.total_depth.saturating_sub(1);
    ctx.active_pairs.remove(&pair);
    res
}

fn unify_logic(
    arena: &mut ValueArena,
    v1_id: ValueId,
    v2_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    let Some(val1) = arena.get(v1_id).cloned() else {
        return arena.bottom("invalid node id");
    };
    let Some(val2) = arena.get(v2_id).cloned() else {
        return arena.bottom("invalid node id");
    };

    // 1. Bottom propagation: _|_ ⊓ x = _|_
    if let Value::Bottom(_) = &val1 {
        return v1_id;
    }
    if let Value::Bottom(_) = &val2 {
        return v2_id;
    }

    // 2. Top identity: _ ⊓ x = x
    if let Value::Top = &val1 {
        return v2_id;
    }
    if let Value::Top = &val2 {
        return v1_id;
    }

    // 3. Disjunction handling: (A | B) ⊓ C
    if let Value::Disjunction { branches } = &val1 {
        return unify_disjunction(arena, v1_id, branches, v2_id, ctx);
    }
    if let Value::Disjunction { branches } = &val2 {
        return unify_disjunction(arena, v2_id, branches, v1_id, ctx);
    }

    // 4. Bounds constraints
    if let Value::Bounds {
        base_type,
        constraints,
    } = &val1
    {
        return unify_bounds(arena, *base_type, constraints.clone(), v2_id);
    }
    if let Value::Bounds {
        base_type,
        constraints,
    } = &val2
    {
        return unify_bounds(arena, *base_type, constraints.clone(), v1_id);
    }

    // 5. Validators & Composite Validators
    if let Value::Validators(list) = &val1 {
        return unify_validators_list(arena, list.clone(), v2_id, ctx);
    }
    if let Value::Validators(list) = &val2 {
        return unify_validators_list(arena, list.clone(), v1_id, ctx);
    }

    if let Value::BuiltinValidator { name, target } = &val1 {
        return unify_validator(arena, v1_id, name.clone(), *target, v2_id);
    }
    if let Value::BuiltinValidator { name, target } = &val2 {
        return unify_validator(arena, v2_id, name.clone(), *target, v1_id);
    }

    // 6. Recursive Reference Resolution
    if let Value::RecursiveRef { target, .. } = &val1 {
        if let Some(t_id) = target {
            return unify_internal(arena, *t_id, v2_id, ctx);
        }
        return v1_id;
    }
    if let Value::RecursiveRef { target, .. } = &val2 {
        if let Some(t_id) = target {
            return unify_internal(arena, v1_id, *t_id, ctx);
        }
        return v2_id;
    }

    // 7. Types & Concrete Values
    match (&val1, &val2) {
        // Concrete vs Concrete
        (Value::Null, Value::Null) => arena.alloc(Value::Null),
        (Value::Bool(b1), Value::Bool(b2)) => {
            if b1 == b2 {
                arena.alloc(Value::Bool(*b1))
            } else {
                arena.bottom(format!("conflicting values: {b1} and {b2}"))
            }
        }
        (Value::Int(i1), Value::Int(i2)) => {
            if i1 == i2 {
                arena.alloc(Value::Int(i1.clone()))
            } else {
                arena.bottom(format!("conflicting values: {i1} and {i2}"))
            }
        }
        (Value::Float(f1), Value::Float(f2)) => {
            if (f1 - f2).abs() < f64::EPSILON {
                arena.alloc(Value::Float(*f1))
            } else {
                arena.bottom(format!("conflicting values: {f1} and {f2}"))
            }
        }
        (Value::String(s1), Value::String(s2)) => {
            if s1 == s2 {
                arena.alloc(Value::String(s1.clone()))
            } else {
                arena.bottom(format!("conflicting values: \"{s1}\" and \"{s2}\""))
            }
        }
        (Value::Bytes(b1), Value::Bytes(b2)) => {
            if b1 == b2 {
                arena.alloc(Value::Bytes(b1.clone()))
            } else {
                arena.bottom("conflicting bytes")
            }
        }

        // Type vs Type
        (Value::Type(t1), Value::Type(t2)) => {
            if t1 == t2 {
                v1_id
            } else if *t1 == TypeKind::Top {
                v2_id
            } else if *t2 == TypeKind::Top {
                v1_id
            } else if *t1 == TypeKind::Number && (t2.is_integer() || t2.is_float()) {
                v2_id
            } else if *t2 == TypeKind::Number && (t1.is_integer() || t1.is_float()) {
                v1_id
            } else if *t1 == TypeKind::Int && t2.is_integer() {
                v2_id
            } else if *t2 == TypeKind::Int && t1.is_integer() {
                v1_id
            } else if *t1 == TypeKind::Uint && t2.is_unsigned_integer() {
                v2_id
            } else if *t2 == TypeKind::Uint && t1.is_unsigned_integer() {
                v1_id
            } else if *t1 == TypeKind::Float && t2.is_float() {
                v2_id
            } else if *t2 == TypeKind::Float && t1.is_float() {
                v1_id
            } else {
                arena.bottom(format!("conflicting types: {t1} and {t2}"))
            }
        }

        // Type vs Concrete
        (Value::Type(t), concrete) => unify_type_and_concrete(arena, t, concrete, v2_id),
        (concrete, Value::Type(t)) => unify_type_and_concrete(arena, t, concrete, v1_id),

        // Struct vs Struct
        (Value::Struct(s1), Value::Struct(s2)) => {
            unify_structs(arena, v1_id, s1, v2_id, s2, ctx)
        }

        // List vs List
        (
            Value::List {
                elements: e1,
                ellipsis: el1,
            },
            Value::List {
                elements: e2,
                ellipsis: el2,
            },
        ) => unify_lists(arena, e1, el1, e2, el2, ctx),

        // Mismatched types
        _ => arena.bottom("conflicting incompatible types"),
    }
}

fn unify_type_and_concrete(
    arena: &mut ValueArena,
    t: &TypeKind,
    concrete: &Value,
    concrete_id: ValueId,
) -> ValueId {
    let matches = match (t, concrete) {
        (TypeKind::Null, Value::Null) => true,
        (TypeKind::Bool, Value::Bool(_)) => true,
        (TypeKind::Int, Value::Int(_)) => true,
        (TypeKind::Uint, Value::Int(i)) => i.sign() != num_bigint::Sign::Minus,
        (TypeKind::Uint8, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (0..=255).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Uint16, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (0..=65535).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Uint32, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (0..=4294967295).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Uint64, Value::Int(i)) => {
            i.sign() != num_bigint::Sign::Minus && i.to_u64().is_some()
        }
        (TypeKind::Int8, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (-128..=127).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Int16, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (-32768..=32767).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Int32, Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                (-2147483648..=2147483647).contains(&n)
            } else {
                false
            }
        }
        (TypeKind::Int64, Value::Int(i)) => i.to_i64().is_some(),
        (TypeKind::Float, Value::Float(_)) => true,
        (TypeKind::Float32, Value::Float(_)) => true,
        (TypeKind::Float64, Value::Float(_)) => true,
        (TypeKind::Number, Value::Int(_) | Value::Float(_)) => true,
        (TypeKind::String, Value::String(_)) => true,
        (TypeKind::Bytes, Value::Bytes(_)) => true,
        (TypeKind::List, Value::List { .. }) => true,
        (TypeKind::Struct, Value::Struct(_)) => true,
        (TypeKind::Top, _) => true,
        _ => false,
    };

    if matches {
        concrete_id
    } else {
        arena.bottom(format!("type mismatch: expected {t}"))
    }
}

fn unify_bounds(
    arena: &mut ValueArena,
    base_type: Option<TypeKind>,
    mut constraints: Vec<(BoundOp, ValueId)>,
    other_id: ValueId,
) -> ValueId {
    let other = match arena.get(other_id) {
        Some(v) => v.clone(),
        None => return arena.bottom("invalid node id"),
    };

    match other {
        // Unifying Bounds with another Bounds -> merge constraints and base types
        Value::Bounds {
            base_type: other_base,
            constraints: other_constraints,
        } => {
            let merged_base = match (base_type, other_base) {
                (Some(b1), Some(b2)) => {
                    if b1 == b2 {
                        Some(b1)
                    } else if b1 == TypeKind::Number
                        && (b2 == TypeKind::Int || b2 == TypeKind::Float)
                    {
                        Some(b2)
                    } else if b2 == TypeKind::Number
                        && (b1 == TypeKind::Int || b1 == TypeKind::Float)
                    {
                        Some(b1)
                    } else {
                        return arena
                            .bottom(format!("conflicting bound base types: {b1} and {b2}"));
                    }
                }
                (Some(b), None) | (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            constraints.extend(other_constraints);
            arena.alloc(Value::Bounds {
                base_type: merged_base,
                constraints,
            })
        }

        // Unifying Bounds with Type -> enforce base type
        Value::Type(t) => {
            let merged_base = match base_type {
                Some(b) => {
                    if b == t {
                        Some(b)
                    } else if b == TypeKind::Number && (t == TypeKind::Int || t == TypeKind::Float)
                    {
                        Some(t)
                    } else if t == TypeKind::Number && (b == TypeKind::Int || b == TypeKind::Float)
                    {
                        Some(b)
                    } else {
                        return arena.bottom(format!("conflicting bound base types: {b} and {t}"));
                    }
                }
                None => Some(t),
            };
            arena.alloc(Value::Bounds {
                base_type: merged_base,
                constraints,
            })
        }

        // Unifying Bounds with concrete value -> test all constraints against candidate
        Value::Int(ref i_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::Int
                && bt != TypeKind::Number
            {
                return arena.bottom(format!("type mismatch: expected {bt}, found int"));
            }
            for (op, target_id) in constraints {
                match arena.get(target_id) {
                    Some(Value::Int(t_val)) => {
                        let ok = match op {
                            BoundOp::Less => i_val < t_val,
                            BoundOp::LessEqual => i_val <= t_val,
                            BoundOp::Greater => i_val > t_val,
                            BoundOp::GreaterEqual => i_val >= t_val,
                            BoundOp::NotEqual => i_val != t_val,
                            _ => false,
                        };
                        if !ok {
                            return arena.bottom(format!(
                                "value {i_val} does not satisfy bound {op} {t_val}"
                            ));
                        }
                    }
                    Some(Value::Float(t_val)) => {
                        let i_f = i_val.to_f64().unwrap_or(0.0);
                        let ok = match op {
                            BoundOp::Less => i_f < *t_val,
                            BoundOp::LessEqual => i_f <= *t_val,
                            BoundOp::Greater => i_f > *t_val,
                            BoundOp::GreaterEqual => i_f >= *t_val,
                            BoundOp::NotEqual => (i_f - *t_val).abs() > f64::EPSILON,
                            _ => false,
                        };
                        if !ok {
                            return arena.bottom(format!(
                                "value {i_val} does not satisfy bound {op} {t_val}"
                            ));
                        }
                    }
                    _ => {
                        return arena.bottom("bound target type mismatch for int");
                    }
                }
            }
            other_id
        }

        Value::Float(f_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::Float
                && bt != TypeKind::Number
            {
                return arena.bottom(format!("type mismatch: expected {bt}, found float"));
            }
            for (op, target_id) in constraints {
                let target_f = match arena.get(target_id) {
                    Some(Value::Float(t_val)) => Some(*t_val),
                    Some(Value::Int(t_val)) => t_val.to_f64(),
                    _ => None,
                };

                if let Some(t_val) = target_f {
                    let ok = match op {
                        BoundOp::Less => f_val < t_val,
                        BoundOp::LessEqual => f_val <= t_val,
                        BoundOp::Greater => f_val > t_val,
                        BoundOp::GreaterEqual => f_val >= t_val,
                        BoundOp::NotEqual => (f_val - t_val).abs() > f64::EPSILON,
                        _ => false,
                    };
                    if !ok {
                        return arena
                            .bottom(format!("value {f_val} does not satisfy bound {op} {t_val}"));
                    }
                } else {
                    return arena.bottom("bound target type mismatch for float");
                }
            }
            other_id
        }

        Value::String(ref s_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::String
            {
                return arena.bottom(format!("type mismatch: expected {bt}, found string"));
            }
            for (op, target_id) in constraints {
                if let Some(Value::String(pattern)) = arena.get(target_id) {
                    match op {
                        BoundOp::RegexMatch => {
                            if let Ok(re) = Regex::new(pattern) {
                                if !re.is_match(s_val) {
                                    return arena.bottom(format!(
                                        "string \"{s_val}\" does not match regex \"{pattern}\""
                                    ));
                                }
                            } else {
                                return arena.bottom(format!("invalid regex: \"{pattern}\""));
                            }
                        }
                        BoundOp::RegexNotMatch => {
                            if let Ok(re) = Regex::new(pattern) {
                                if re.is_match(s_val) {
                                    return arena.bottom(format!(
                                        "string \"{s_val}\" matches regex \"{pattern}\""
                                    ));
                                }
                            } else {
                                return arena.bottom(format!("invalid regex: \"{pattern}\""));
                            }
                        }
                        _ => return arena.bottom("unsupported bound op on string"),
                    }
                }
            }
            other_id
        }

        Value::BuiltinValidator { .. } => {
            let bounds_id = arena.alloc(Value::Bounds {
                base_type,
                constraints,
            });
            arena.alloc(Value::Validators(vec![bounds_id, other_id]))
        }
        Value::Validators(mut list) => {
            let bounds_id = arena.alloc(Value::Bounds {
                base_type,
                constraints,
            });
            list.push(bounds_id);
            arena.alloc(Value::Validators(list))
        }

        _ => arena.bottom("cannot unify bound constraint with value"),
    }
}

pub fn field_matches_pattern(arena: &ValueArena, pattern_val: ValueId, field_name: &str) -> bool {
    match arena.get(pattern_val) {
        Some(Value::Type(TypeKind::String | TypeKind::Top)) => true,
        Some(Value::String(s)) => s == field_name,
        Some(Value::Bounds { constraints, .. }) => {
            for (op, target_id) in constraints {
                if let Some(Value::String(pat)) = arena.get(*target_id)
                    && let Ok(re) = Regex::new(pat)
                {
                    let matched = match op {
                        BoundOp::RegexMatch => re.is_match(field_name),
                        BoundOp::RegexNotMatch => !re.is_match(field_name),
                        _ => false,
                    };
                    if !matched {
                        return false;
                    }
                }
            }
            true
        }
        _ => false,
    }
}

fn unify_structs(
    arena: &mut ValueArena,
    s1_id: ValueId,
    s1: &StructValue,
    s2_id: ValueId,
    s2: &StructValue,
    ctx: &mut UnifyContext,
) -> ValueId {
    if ctx.struct_depth >= MAX_STRUCT_DEPTH {
        return arena.bottom("cycle error: struct recursion depth limit exceeded");
    }
    let pair = (s1_id.min(s2_id), s1_id.max(s2_id));
    if !ctx.active_structs.insert(pair) {
        return arena.bottom("cycle error: cyclic struct unification detected");
    }
    ctx.struct_depth += 1;

    let res = unify_structs_inner(arena, s1, s2, ctx);

    ctx.struct_depth = ctx.struct_depth.saturating_sub(1);
    ctx.active_structs.remove(&pair);
    res
}

fn unify_structs_inner(
    arena: &mut ValueArena,
    s1: &StructValue,
    s2: &StructValue,
    ctx: &mut UnifyContext,
) -> ValueId {
    // Closedness validation with pattern constraints support
    if s1.is_closed {
        for k in s2.fields.keys() {
            let allowed_in_fields = s1.fields.contains_key(k);
            let allowed_by_pattern = s1
                .pattern_constraints
                .iter()
                .any(|pc| field_matches_pattern(arena, pc.pattern_val, k));

            if !allowed_in_fields && !allowed_by_pattern {
                return arena.bottom(format!("field '{k}' not allowed in closed struct"));
            }
        }
    }
    if s2.is_closed {
        for k in s1.fields.keys() {
            let allowed_in_fields = s2.fields.contains_key(k);
            let allowed_by_pattern = s2
                .pattern_constraints
                .iter()
                .any(|pc| field_matches_pattern(arena, pc.pattern_val, k));

            if !allowed_in_fields && !allowed_by_pattern {
                return arena.bottom(format!("field '{k}' not allowed in closed struct"));
            }
        }
    }

    let is_closed = s1.is_closed || s2.is_closed;
    let mut merged = StructValue::new(is_closed);

    // Merge pattern constraints
    merged
        .pattern_constraints
        .extend(s1.pattern_constraints.clone());
    merged
        .pattern_constraints
        .extend(s2.pattern_constraints.clone());

    // Merge regular fields
    let all_keys: BTreeSet<String> = s1.fields.keys().chain(s2.fields.keys()).cloned().collect();
    for key in all_keys {
        let entry = match (s1.fields.get(&key), s2.fields.get(&key)) {
            (Some(e1), Some(e2)) => {
                let unified_val = unify_internal(arena, e1.val, e2.val, ctx);
                if let Some(Value::Bottom(_)) = arena.get(unified_val) {
                    return unified_val;
                }
                FieldEntry {
                    val: unified_val,
                    optional: e1.optional && e2.optional,
                }
            }
            (Some(e1), None) => e1.clone(),
            (None, Some(e2)) => e2.clone(),
            (None, None) => unreachable!(),
        };

        // Apply all pattern constraints matching this field
        let mut cur_val = entry.val;
        for pc in &merged.pattern_constraints {
            if field_matches_pattern(arena, pc.pattern_val, &key) {
                cur_val = unify_internal(arena, cur_val, pc.target_val, ctx);
                if let Some(Value::Bottom(_)) = arena.get(cur_val) {
                    return cur_val;
                }
            }
        }

        merged.fields.insert(
            key,
            FieldEntry {
                val: cur_val,
                optional: entry.optional,
            },
        );
    }

    // Merge definitions (#Def)
    let all_def_keys: BTreeSet<String> = s1
        .definitions
        .keys()
        .chain(s2.definitions.keys())
        .cloned()
        .collect();
    for key in all_def_keys {
        let entry = match (s1.definitions.get(&key), s2.definitions.get(&key)) {
            (Some(e1), Some(e2)) => {
                let unified_val = unify_internal(arena, e1.val, e2.val, ctx);
                if let Some(Value::Bottom(_)) = arena.get(unified_val) {
                    return unified_val;
                }
                FieldEntry {
                    val: unified_val,
                    optional: e1.optional && e2.optional,
                }
            }
            (Some(e1), None) => e1.clone(),
            (None, Some(e2)) => e2.clone(),
            (None, None) => unreachable!(),
        };
        merged.definitions.insert(key, entry);
    }

    // Merge hidden fields (_hidden)
    let all_hidden_keys: BTreeSet<String> =
        s1.hidden.keys().chain(s2.hidden.keys()).cloned().collect();
    for key in all_hidden_keys {
        let entry = match (s1.hidden.get(&key), s2.hidden.get(&key)) {
            (Some(e1), Some(e2)) => {
                let unified_val = unify_internal(arena, e1.val, e2.val, ctx);
                if let Some(Value::Bottom(_)) = arena.get(unified_val) {
                    return unified_val;
                }
                FieldEntry {
                    val: unified_val,
                    optional: e1.optional && e2.optional,
                }
            }
            (Some(e1), None) => e1.clone(),
            (None, Some(e2)) => e2.clone(),
            (None, None) => unreachable!(),
        };
        merged.hidden.insert(key, entry);
    }

    arena.alloc(Value::Struct(merged))
}

fn unify_lists(
    arena: &mut ValueArena,
    e1: &[ValueId],
    el1: &Option<ValueId>,
    e2: &[ValueId],
    el2: &Option<ValueId>,
    ctx: &mut UnifyContext,
) -> ValueId {
    let max_len = e1.len().max(e2.len());

    if el1.is_none() && el2.is_none() && e1.len() != e2.len() {
        return arena.bottom(format!(
            "conflicting list lengths: {} and {}",
            e1.len(),
            e2.len()
        ));
    }
    if el1.is_none() && e2.len() > e1.len() {
        return arena.bottom(format!(
            "list length {} exceeds closed list length {}",
            e2.len(),
            e1.len()
        ));
    }
    if el2.is_none() && e1.len() > e2.len() {
        return arena.bottom(format!(
            "list length {} exceeds closed list length {}",
            e1.len(),
            e2.len()
        ));
    }

    let mut unified_elements = Vec::with_capacity(max_len);
    for idx in 0..max_len {
        let val1 = match (e1.get(idx).copied(), *el1) {
            (Some(v), _) => Some(v),
            (None, Some(p)) => Some(p),
            (None, None) => None,
        };
        let val2 = match (e2.get(idx).copied(), *el2) {
            (Some(v), _) => Some(v),
            (None, Some(p)) => Some(p),
            (None, None) => None,
        };

        match (val1, val2) {
            (Some(v1), Some(v2)) => {
                let u = unify_internal(arena, v1, v2, ctx);
                if let Some(Value::Bottom(_)) = arena.get(u) {
                    return u;
                }
                unified_elements.push(u);
            }
            (Some(v), None) | (None, Some(v)) => {
                unified_elements.push(v);
            }
            (None, None) => {}
        }
    }

    let unified_ellipsis = match (*el1, *el2) {
        (Some(p1), Some(p2)) => {
            let u = unify_internal(arena, p1, p2, ctx);
            if let Some(Value::Bottom(_)) = arena.get(u) {
                return u;
            }
            Some(u)
        }
        (Some(p), None) | (None, Some(p))
            if e1.len() == e2.len() || (el1.is_some() && el2.is_some()) =>
        {
            Some(p)
        }
        _ => None,
    };

    arena.alloc(Value::List {
        elements: unified_elements,
        ellipsis: unified_ellipsis,
    })
}

fn unify_disjunction(
    arena: &mut ValueArena,
    disj_id: ValueId,
    branches: &[DisjunctionBranch],
    other_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    if ctx.disjunction_depth >= MAX_DISJUNCTION_DEPTH {
        return arena.bottom("cycle error: disjunction recursion depth limit exceeded");
    }
    let pair = (disj_id.min(other_id), disj_id.max(other_id));
    if !ctx.active_disjunctions.insert(pair) {
        return arena.bottom("cycle error: cyclic disjunction unification detected");
    }
    ctx.disjunction_depth += 1;

    let res = unify_disjunction_inner(arena, branches, other_id, ctx);

    ctx.disjunction_depth = ctx.disjunction_depth.saturating_sub(1);
    ctx.active_disjunctions.remove(&pair);
    res
}

fn unify_disjunction_inner(
    arena: &mut ValueArena,
    branches: &[DisjunctionBranch],
    other_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    if let Some(Value::Disjunction {
        branches: other_branches,
    }) = arena.get(other_id).cloned()
    {
        let mut valid_branches = Vec::new();
        let mut branch_errors = Vec::new();
        for b1 in branches {
            for b2 in &other_branches {
                let cp = arena.checkpoint();
                let u = unify_internal(arena, b1.val, b2.val, ctx);
                if let Some(Value::Bottom(b)) = arena.get(u) {
                    branch_errors.push(b.to_string());
                    arena.rollback(cp);
                    continue;
                }
                valid_branches.push(DisjunctionBranch {
                    default: b1.default || b2.default,
                    val: u,
                });
            }
        }
        return match valid_branches.len() {
            0 => {
                if branch_errors.is_empty() {
                    arena.bottom("no matching disjunction branch")
                } else {
                    arena.bottom(format!(
                        "no matching disjunction branch: [{}]",
                        branch_errors.join("; ")
                    ))
                }
            }
            1 => valid_branches.pop().unwrap().val,
            _ => arena.alloc(Value::Disjunction {
                branches: valid_branches,
            }),
        };
    }

    let mut valid_branches = Vec::new();
    let mut branch_errors = Vec::new();

    for branch in branches {
        let cp = arena.checkpoint();
        let u = unify_internal(arena, branch.val, other_id, ctx);
        if let Some(Value::Bottom(b)) = arena.get(u) {
            // This branch conflicted, rollback allocations made during the branch
            branch_errors.push(b.to_string());
            arena.rollback(cp);
            continue;
        }
        valid_branches.push(DisjunctionBranch {
            default: branch.default,
            val: u,
        });
    }

    match valid_branches.len() {
        0 => {
            if branch_errors.is_empty() {
                arena.bottom("no matching disjunction branch")
            } else {
                arena.bottom(format!(
                    "no matching disjunction branch: [{}]",
                    branch_errors.join("; ")
                ))
            }
        }
        1 => valid_branches.pop().unwrap().val,
        _ => arena.alloc(Value::Disjunction {
            branches: valid_branches,
        }),
    }
}

fn unify_validators_list(
    arena: &mut ValueArena,
    mut list: Vec<ValueId>,
    other_id: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    let other = match arena.get(other_id) {
        Some(v) => v.clone(),
        None => return arena.bottom("invalid node id"),
    };

    match other {
        Value::Type(
            TypeKind::Top
            | TypeKind::String
            | TypeKind::List
            | TypeKind::Struct
            | TypeKind::Int
            | TypeKind::Number,
        ) => arena.alloc(Value::Validators(list)),
        Value::BuiltinValidator { .. } => {
            list.push(other_id);
            arena.alloc(Value::Validators(list))
        }
        Value::Bounds { .. } => {
            list.push(other_id);
            arena.alloc(Value::Validators(list))
        }
        Value::Validators(other_list) => {
            list.extend(other_list);
            arena.alloc(Value::Validators(list))
        }
        _ => {
            let mut cur = other_id;
            for validator_id in list {
                cur = unify_internal(arena, validator_id, cur, ctx);
                if let Some(Value::Bottom(_)) = arena.get(cur) {
                    return cur;
                }
            }
            cur
        }
    }
}

fn unify_validator(
    arena: &mut ValueArena,
    validator_id: ValueId,
    name: String,
    target_id: ValueId,
    candidate_id: ValueId,
) -> ValueId {
    let candidate = match arena.get(candidate_id) {
        Some(v) => v.clone(),
        None => return arena.bottom("invalid node id"),
    };

    match candidate {
        Value::Type(
            TypeKind::Top
            | TypeKind::String
            | TypeKind::List
            | TypeKind::Struct
            | TypeKind::Int
            | TypeKind::Number,
        ) => validator_id,
        Value::BuiltinValidator { .. } => {
            arena.alloc(Value::Validators(vec![validator_id, candidate_id]))
        }
        Value::Bounds { .. } => arena.alloc(Value::Validators(vec![validator_id, candidate_id])),
        Value::Validators(mut list) => {
            list.push(validator_id);
            arena.alloc(Value::Validators(list))
        }
        Value::String(ref s) => {
            if name.starts_with("strings.MinRunes(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(min) = i.to_usize()
                {
                    if s.chars().count() >= min {
                        return candidate_id;
                    } else {
                        return arena.bottom(format!(
                            "string length {} is less than minimum runes {min}",
                            s.chars().count()
                        ));
                    }
                }
            } else if name.starts_with("strings.MaxRunes(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(max) = i.to_usize()
                {
                    if s.chars().count() <= max {
                        return candidate_id;
                    } else {
                        return arena.bottom(format!(
                            "string length {} exceeds maximum runes {max}",
                            s.chars().count()
                        ));
                    }
                }
            } else if name == "time.Time"
                && let Ok(re) = regex::Regex::new(
                    r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$",
                )
            {
                if re.is_match(s) {
                    return candidate_id;
                } else {
                    return arena
                        .bottom(format!("string \"{s}\" is not a valid RFC3339 timestamp"));
                }
            } else if name == "time.Duration" {
                if crate::stdlib::time::parse_duration_nanos(s).is_ok() {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("string \"{s}\" is not a valid duration"));
                }
            } else if name == "net.IPv4" {
                if s.parse::<std::net::Ipv4Addr>().is_ok() {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("string \"{s}\" is not a valid IPv4 address"));
                }
            } else if name == "net.IPv6" {
                if s.parse::<std::net::Ipv6Addr>().is_ok() {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("string \"{s}\" is not a valid IPv6 address"));
                }
            } else if name == "net.IP" {
                if s.parse::<std::net::IpAddr>().is_ok() {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("string \"{s}\" is not a valid IP address"));
                }
            } else if name == "uuid.Valid" {
                if is_valid_uuid_str(s) {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("string \"{s}\" is not a valid UUID"));
                }
            }
            arena.bottom(format!("validator '{name}' failed on string \"{s}\""))
        }
        Value::Struct(ref s) => {
            if name.starts_with("struct.MinFields(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(min) = i.to_usize()
                {
                    if s.fields.len() >= min {
                        return candidate_id;
                    } else {
                        return arena.bottom(format!(
                            "struct has {} fields, expected at least {min}",
                            s.fields.len()
                        ));
                    }
                }
            } else if name.starts_with("struct.MaxFields(")
                && let Some(Value::Int(i)) = arena.get(target_id)
                && let Some(max) = i.to_usize()
            {
                if s.fields.len() <= max {
                    return candidate_id;
                } else {
                    return arena.bottom(format!(
                        "struct has {} fields, expected at most {max}",
                        s.fields.len()
                    ));
                }
            }
            arena.bottom(format!("validator '{name}' failed on struct"))
        }
        Value::List { ref elements, .. } => {
            if name.starts_with("list.MinItems(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(min) = i.to_usize()
                {
                    if elements.len() >= min {
                        return candidate_id;
                    } else {
                        return arena.bottom(format!(
                            "list length {} is less than minimum items {min}",
                            elements.len()
                        ));
                    }
                }
            } else if name.starts_with("list.MaxItems(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(max) = i.to_usize()
                {
                    if elements.len() <= max {
                        return candidate_id;
                    } else {
                        return arena.bottom(format!(
                            "list length {} exceeds maximum items {max}",
                            elements.len()
                        ));
                    }
                }
            } else if name == "list.UniqueItems()" {
                let mut seen = std::collections::HashSet::new();
                for &elem in elements {
                    let repr = match arena.get(elem) {
                        Some(Value::String(s)) => format!("str:{s}"),
                        Some(Value::Int(i)) => format!("int:{i}"),
                        Some(Value::Float(f)) => format!("flt:{f}"),
                        Some(Value::Bool(b)) => format!("bool:{b}"),
                        _ => format!("id:{elem:?}"),
                    };
                    if !seen.insert(repr) {
                        return arena.bottom("list contains duplicate elements");
                    }
                }
                return candidate_id;
            } else if name.starts_with("list.MatchN") {
                return candidate_id;
            }
            arena.bottom(format!("validator '{name}' failed on list"))
        }
        Value::Int(ref i) => {
            if name == "time.Duration" {
                return candidate_id;
            }
            if name.starts_with("math.MultipleOf(")
                && let Some(Value::Int(t)) = arena.get(target_id)
                && let (Some(num), Some(mod_val)) = (i.to_i64(), t.to_i64())
            {
                if mod_val != 0 && num % mod_val == 0 {
                    return candidate_id;
                } else {
                    return arena.bottom(format!("number {num} is not a multiple of {mod_val}"));
                }
            }
            arena.bottom(format!("validator '{name}' failed on integer {i}"))
        }
        _ => arena.bottom(format!("validator '{name}' is not applicable to value")),
    }
}

fn is_valid_uuid_str(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            continue;
        }
        if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}
