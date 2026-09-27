use crate::value::*;
use num_traits::ToPrimitive;
use regex::Regex;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

// Compiled patterns by source text. A pattern meets every field of every
// literal it can see, so recompiling the same expression per field is pure
// waste; the cache pays once per distinct pattern. Thread-local because
// evaluation is single-threaded (`Rc` throughout), which also keeps the hot
// path lock-free. Past the cap patterns recompute instead of retaining an
// unbounded table from generated sources.
thread_local! {
    static REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
}

/// How many distinct patterns the cache retains.
const MAX_CACHED_PATTERNS: usize = 4096;

/// A compiled pattern, or `None` when it does not compile. Callers keep
/// their existing fallback for the `None` case.
fn cached_regex(pattern: &str) -> Option<Regex> {
    REGEX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(compiled) = cache.get(pattern) {
            return Some(compiled.clone());
        }
        let compiled = Regex::new(pattern).ok()?;
        if cache.len() < MAX_CACHED_PATTERNS {
            cache.insert(pattern.to_owned(), compiled.clone());
        }
        Some(compiled)
    })
}

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
    pub active_metadata_choices: BTreeSet<(ValueId, ValueId)>,
}

impl UnifyContext {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Unify two values in the arena, computing their greatest lower bound (meet: a ⊓ b).
/// Values carrying lexical recipes need the evaluator's `unify_and_rederive`
/// entrypoint to refresh their cached results against the merged fields.
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

/// A bottom from unification: two values that cannot both hold.
///
/// Worth a helper rather than a kind argument at fifty-odd call sites, and
/// worth marking at all because the relaxation loop treats a conflict and an
/// unresolved reference differently - one is final, the other is "not yet".
fn conflict<S: Into<String>>(arena: &mut ValueArena, msg: S) -> ValueId {
    arena.bottom_of(BottomKind::Conflict, msg)
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
    let (v1_ref, v2_ref) = match (arena.get(v1_id), arena.get(v2_id)) {
        (Some(v1), Some(v2)) => (v1, v2),
        _ => return arena.bottom("invalid node id"),
    };

    if v1_id == v2_id {
        return v1_id;
    }

    if arena.embedded_metadata(v1_id).is_some() || arena.embedded_metadata(v2_id).is_some() {
        // Distribute an ordinary choice before merging its branches' metadata.
        if arena.embedded_metadata(v1_id).is_none()
            && let Value::Disjunction { branches } = v1_ref.clone()
        {
            return unify_disjunction(arena, v1_id, &branches, v2_id, ctx);
        }
        if arena.embedded_metadata(v2_id).is_none()
            && let Value::Disjunction { branches } = v2_ref.clone()
        {
            return unify_disjunction(arena, v2_id, &branches, v1_id, ctx);
        }
        return unify_metadata(arena, v1_id, v2_id, ctx);
    }

    // 1. Bottom propagation: _|_ ⊓ x = _|_
    if matches!(v1_ref, Value::Bottom(_)) {
        return v1_id;
    }
    if matches!(v2_ref, Value::Bottom(_)) {
        return v2_id;
    }

    // 2. Top identity: _ ⊓ x = x
    if matches!(v1_ref, Value::Top) {
        return v2_id;
    }
    if matches!(v2_ref, Value::Top) {
        return v1_id;
    }

    let val1 = v1_ref.clone();
    let val2 = v2_ref.clone();

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
    //
    // A reference with no target yet is a definition read before its
    // declaration was evaluated. Meeting it must stay pending (so the
    // relaxation loop derives the meet again once the target binds),
    // never cache the bare reference (the meet would be lost when the
    // target fills). This matches `select` on the same shape.
    if let Value::RecursiveRef { name, target } = &val1
        && target.is_none()
    {
        return arena.bottom_of(BottomKind::Unresolved, format!("{name} not evaluated yet"));
    }
    if let Value::RecursiveRef { name, target } = &val2
        && target.is_none()
    {
        return arena.bottom_of(BottomKind::Unresolved, format!("{name} not evaluated yet"));
    }
    if let Value::RecursiveRef {
        target: Some(t_id), ..
    } = &val1
    {
        return unify_internal(arena, *t_id, v2_id, ctx);
    }
    if let Value::RecursiveRef {
        target: Some(t_id), ..
    } = &val2
    {
        return unify_internal(arena, v1_id, *t_id, ctx);
    }

    // 7. Types & Concrete Values
    match (&val1, &val2) {
        // Concrete vs Concrete
        (Value::Null, Value::Null) => v1_id,
        (Value::Bool(b1), Value::Bool(b2)) => {
            if b1 == b2 {
                v1_id
            } else {
                conflict(arena, format!("conflicting values: {b1} and {b2}"))
            }
        }
        (Value::Int(i1), Value::Int(i2)) => {
            if i1 == i2 {
                v1_id
            } else {
                conflict(arena, format!("conflicting values: {i1} and {i2}"))
            }
        }
        (Value::Float(f1), Value::Float(f2)) => {
            if (f1 - f2).abs() < f64::EPSILON {
                v1_id
            } else {
                conflict(arena, format!("conflicting values: {f1} and {f2}"))
            }
        }
        (Value::String(s1), Value::String(s2)) => {
            if s1 == s2 {
                v1_id
            } else {
                conflict(arena, format!("conflicting values: \"{s1}\" and \"{s2}\""))
            }
        }
        (Value::Bytes(b1), Value::Bytes(b2)) => {
            if b1 == b2 {
                v1_id
            } else {
                conflict(arena, "conflicting bytes")
            }
        }

        // Type vs Type: the numeric lattice decides. The narrower side wins;
        // unrelated kinds conflict. See `NumberKind::subsumes`.
        (Value::Type(t1), Value::Type(t2)) => {
            if t1 == t2 {
                v1_id
            } else if *t1 == TypeKind::Top {
                v2_id
            } else if *t2 == TypeKind::Top {
                v1_id
            } else if let (Some(n1), Some(n2)) = (t1.number_kind(), t2.number_kind()) {
                if n1.subsumes(n2) {
                    v2_id
                } else if n2.subsumes(n1) {
                    v1_id
                } else {
                    conflict(arena, format!("conflicting types: {t1} and {t2}"))
                }
            } else {
                conflict(arena, format!("conflicting types: {t1} and {t2}"))
            }
        }

        // Type vs Concrete
        (Value::Type(t), concrete) => unify_type_and_concrete(arena, t, concrete, v2_id),
        (concrete, Value::Type(t)) => unify_type_and_concrete(arena, t, concrete, v1_id),

        // Struct vs Struct
        (Value::Struct(s1), Value::Struct(s2)) => unify_structs(arena, v1_id, s1, v2_id, s2, ctx),

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
        _ => conflict(arena, "conflicting incompatible types"),
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
        // Range checks live on `NumberKind`; float kinds accept no integer.
        (TypeKind::Number(kind), Value::Int(i)) => kind.contains_int(i),
        (
            TypeKind::Number(
                NumberKind::Number | NumberKind::Float | NumberKind::Float32 | NumberKind::Float64,
            ),
            Value::Float(_),
        ) => true,
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
        conflict(arena, format!("type mismatch: expected {t}"))
    }
}

fn unify_metadata(
    arena: &mut ValueArena,
    left: ValueId,
    right: ValueId,
    ctx: &mut UnifyContext,
) -> ValueId {
    let left_meta = arena.embedded_metadata(left).cloned();
    let right_meta = arena.embedded_metadata(right).cloned();
    let fields = if arena.has_embedded_recipe(left) || arena.has_embedded_recipe(right) {
        let mut inputs = None;
        for id in [left, right] {
            let body = arena.metadata(id).map(|m| m.fields).unwrap_or(id);
            let Some(Value::Struct(mut body)) = arena.get(body).cloned() else {
                continue;
            };
            body.is_closed = false;
            let next = arena.alloc(Value::Struct(body));
            inputs = Some(match inputs {
                Some(previous) => unify_internal(arena, previous, next, ctx),
                None => next,
            });
        }
        inputs.expect("an embedded recipe has inputs")
    } else {
        match (&left_meta, &right_meta) {
            (Some(left), Some(right)) => unify_internal(arena, left.fields, right.fields, ctx),
            (Some(metadata), None) | (None, Some(metadata)) => metadata.fields,
            (None, None) => unreachable!("metadata merge has an annotated operand"),
        }
    };
    if !matches!(arena.get(fields), Some(Value::Struct(_))) {
        return fields;
    }
    let conjuncts: Vec<Conjunct> = [(left, left_meta), (right, right_meta)]
        .into_iter()
        .flat_map(|(id, metadata)| match metadata {
            Some(metadata) if matches!(metadata.source, MetadataSource::Closed { .. }) => {
                vec![Conjunct::Value(id)]
            }
            Some(metadata) => metadata
                .conjuncts()
                .expect("embedded metadata has conjuncts")
                .to_vec(),
            None => vec![Conjunct::Value(id)],
        })
        .collect();
    let left = crate::metadata::payload(arena, left);
    let right = crate::metadata::payload(arena, right);
    let value = unify_internal(arena, left, right, ctx);
    crate::metadata::finish_with_context(arena, value, fields, conjuncts, ctx)
}

/// Whether a bound base type narrows to the other side: `number` to
/// `int`/`float` only. Deliberately narrower than the Type-vs-Type meet —
/// `int` does not narrow to `uint8` here; that pair conflicts.
fn narrows_number_base(base: &TypeKind, other: &TypeKind) -> bool {
    *base == TypeKind::Number(NumberKind::Number)
        && matches!(other, TypeKind::Number(NumberKind::Int | NumberKind::Float))
}

/// Whether two base kinds demanded by bound targets are incompatible:
/// number against string. Other pairs (bool, null, top) demand nothing
/// and never conflict here.
fn bound_kinds_conflict(a: &TypeKind, b: &TypeKind) -> bool {
    matches!(
        (a, b),
        (TypeKind::Number(_), TypeKind::String) | (TypeKind::String, TypeKind::Number(_))
    )
}

/// The incompatibility between merged range constraints, if any:
/// `<1 & >2` and `<"a" & >"b"` are both eval-bottom upstream. Each group
/// (numbers, strings, bytes) keeps its strongest lower and upper bound —
/// exclusive wins ties, equality contributes both — and conflicts when
/// the lower passes the upper or touches it exclusively. An integer base
/// snaps both sides to integers first (`>1 & <2` admits no integer).
/// Bounds of different groups never meet here; the demand check rejects
/// those first.
fn incompatible_range(
    arena: &ValueArena,
    constraints: &[(Bound, ValueId)],
    base: Option<&TypeKind>,
) -> Option<String> {
    // (value, inclusive, rendering like `<1`).
    let mut lower_num: Vec<(f64, bool, String)> = Vec::new();
    let mut upper_num: Vec<(f64, bool, String)> = Vec::new();
    let mut lower_text: Vec<(Vec<u8>, bool, String)> = Vec::new();
    let mut upper_text: Vec<(Vec<u8>, bool, String)> = Vec::new();
    for (op, target) in constraints {
        let rendered = |text: String| format!("{op}{text}");
        match arena.get(*target) {
            Some(Value::Int(i)) => {
                let text = rendered(i.to_string());
                let value = i.to_f64().unwrap_or(f64::INFINITY);
                match op {
                    Bound::Greater => lower_num.push((value, false, text)),
                    Bound::GreaterEqual => lower_num.push((value, true, text)),
                    Bound::Less => upper_num.push((value, false, text)),
                    Bound::LessEqual => upper_num.push((value, true, text)),
                    Bound::Equal => {
                        lower_num.push((value, true, text.clone()));
                        upper_num.push((value, true, text));
                    }
                    _ => {}
                }
            }
            Some(Value::Float(f)) => {
                let text = rendered(format!("{f:?}"));
                match op {
                    Bound::Greater => lower_num.push((*f, false, text)),
                    Bound::GreaterEqual => lower_num.push((*f, true, text)),
                    Bound::Less => upper_num.push((*f, false, text)),
                    Bound::LessEqual => upper_num.push((*f, true, text)),
                    Bound::Equal => {
                        lower_num.push((*f, true, text.clone()));
                        upper_num.push((*f, true, text));
                    }
                    _ => {}
                }
            }
            Some(Value::String(s)) => {
                let text = rendered(format!("{s:?}"));
                match op {
                    Bound::Greater => lower_text.push((s.clone().into_bytes(), false, text)),
                    Bound::GreaterEqual => lower_text.push((s.clone().into_bytes(), true, text)),
                    Bound::Less => upper_text.push((s.clone().into_bytes(), false, text)),
                    Bound::LessEqual => upper_text.push((s.clone().into_bytes(), true, text)),
                    Bound::Equal => {
                        lower_text.push((s.clone().into_bytes(), true, text.clone()));
                        upper_text.push((s.clone().into_bytes(), true, text));
                    }
                    _ => {}
                }
            }
            Some(Value::Bytes(b)) => {
                let text = rendered(format!("'{}'", String::from_utf8_lossy(b)));
                match op {
                    Bound::Greater => lower_text.push((b.clone(), false, text)),
                    Bound::GreaterEqual => lower_text.push((b.clone(), true, text)),
                    Bound::Less => upper_text.push((b.clone(), false, text)),
                    Bound::LessEqual => upper_text.push((b.clone(), true, text)),
                    Bound::Equal => {
                        lower_text.push((b.clone(), true, text.clone()));
                        upper_text.push((b.clone(), true, text));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    use std::cmp::Ordering;
    // Strongest lower: largest value, exclusive winning ties. Strongest
    // upper: smallest value, exclusive winning ties.
    let best_lower_num = lower_num
        .iter()
        .filter(|(value, _, _)| value.is_finite())
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.1.cmp(&a.1))
        });
    let best_upper_num = upper_num
        .iter()
        .filter(|(value, _, _)| value.is_finite())
        .min_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
    // An integer base snaps both sides to integers first: `>1 & <2`
    // admits no integer even though rationals fit between.
    let integer_base = matches!(base, Some(TypeKind::Number(NumberKind::Int)));
    let kind_name = if integer_base { "integer" } else { "number" };
    if let (
        Some((lower_value, lower_inclusive, lower_text)),
        Some((upper_value, upper_inclusive, upper_text)),
    ) = (best_lower_num, best_upper_num)
    {
        let admissible = if integer_base {
            let lower = if *lower_inclusive {
                lower_value.ceil()
            } else {
                lower_value.floor() + 1.0
            };
            let upper = if *upper_inclusive {
                upper_value.floor()
            } else {
                upper_value.ceil() - 1.0
            };
            lower <= upper
        } else {
            lower_value < upper_value
                || (lower_value == upper_value && *lower_inclusive && *upper_inclusive)
        };
        if !admissible {
            return Some(format!(
                "incompatible {kind_name} bounds {upper_text} and {lower_text}"
            ));
        }
    }
    let best_lower_text = lower_text
        .iter()
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    let best_upper_text = upper_text
        .iter()
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    if let (
        Some((lower_value, lower_inclusive, lower_text)),
        Some((upper_value, upper_inclusive, upper_text)),
    ) = (best_lower_text, best_upper_text)
        && (lower_value > upper_value
            || (lower_value == upper_value && !(*lower_inclusive && *upper_inclusive)))
    {
        // Bytes literals render the same way; the oracle names the group
        // after them when any endpoint is bytes.
        let has_bytes = constraints
            .iter()
            .any(|(_, target)| matches!(arena.get(*target), Some(Value::Bytes(_))));
        let kind_name = if has_bytes { "bytes" } else { "string" };
        return Some(format!(
            "incompatible {kind_name} bounds {upper_text} and {lower_text}"
        ));
    }
    None
}

/// The first constraint target whose demanded base kind is incompatible
/// with `base`, if any. Non-concrete targets demand nothing.
fn conflicting_demand(
    arena: &ValueArena,
    constraints: &[(Bound, ValueId)],
    base: Option<&TypeKind>,
) -> Option<(TypeKind, TypeKind)> {
    let base = *base?;
    constraints.iter().find_map(|(_, target)| {
        let demand = match arena.get(*target) {
            Some(Value::Int(_) | Value::Float(_)) => TypeKind::Number(NumberKind::Number),
            Some(Value::String(_)) => TypeKind::String,
            _ => return None,
        };
        bound_kinds_conflict(&demand, &base).then_some((demand, base))
    })
}

fn unify_bounds(
    arena: &mut ValueArena,
    base_type: Option<TypeKind>,
    mut constraints: Vec<(Bound, ValueId)>,
    other_id: ValueId,
) -> ValueId {
    let other = match arena.get(other_id) {
        Some(v) => v.clone(),
        None => return arena.bottom("invalid node id"),
    };

    // Unlike an ordered bound, !=null does not restrict the candidate's kind.
    // Discharge it only when that kind excludes null, preserving any remaining
    // constraints and their base type.
    let excludes_null = matches!(
        other,
        Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Bytes(_)
            | Value::List { .. }
            | Value::Struct(_)
    ) || matches!(other, Value::Type(kind) if !matches!(kind, TypeKind::Top | TypeKind::Bottom | TypeKind::Null));
    if excludes_null {
        let before = constraints.len();
        constraints.retain(|(op, target)| {
            !(*op == Bound::NotEqual && matches!(arena.get(*target), Some(Value::Null)))
        });
        if before != constraints.len() && constraints.is_empty() {
            return match base_type {
                Some(kind) => {
                    let base = arena.type_kind(kind);
                    unify(arena, base, other_id)
                }
                None => other_id,
            };
        }
    }

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
                    } else if narrows_number_base(&b1, &b2) {
                        Some(b2)
                    } else if narrows_number_base(&b2, &b1) {
                        Some(b1)
                    } else {
                        return conflict(
                            arena,
                            format!("conflicting bound base types: {b1} and {b2}"),
                        );
                    }
                }
                (Some(b), None) | (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            constraints.extend(other_constraints);
            // A concrete target demands its base kind: `!="a" & <5` meets
            // string against number, which upstream rejects as eval-bottom.
            // Non-concrete targets (null, references, disjunctions) demand
            // nothing yet.
            if let Some((demand, base)) =
                conflicting_demand(arena, &constraints, merged_base.as_ref())
            {
                return conflict(
                    arena,
                    format!("conflicting bound base types: {demand} and {base}"),
                );
            }
            // Ranges must overlap: `<1 & >2` is eval-bottom upstream.
            if let Some(message) = incompatible_range(arena, &constraints, merged_base.as_ref()) {
                return conflict(arena, message);
            }
            let mut demanded: Option<TypeKind> = None;
            for (_, target) in &constraints {
                let demand = match arena.get(*target) {
                    Some(Value::Int(_)) | Some(Value::Float(_)) => {
                        Some(TypeKind::Number(NumberKind::Number))
                    }
                    Some(Value::String(_)) => Some(TypeKind::String),
                    _ => None,
                };
                let Some(demand) = demand else {
                    continue;
                };
                if let Some(seen) = &demanded
                    && bound_kinds_conflict(&demand, seen)
                {
                    return conflict(
                        arena,
                        format!("conflicting bound base types: {seen} and {demand}"),
                    );
                }
                demanded = Some(demand);
            }
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
                    } else if narrows_number_base(&b, &t) {
                        Some(t)
                    } else if narrows_number_base(&t, &b) {
                        Some(b)
                    } else {
                        return conflict(
                            arena,
                            format!("conflicting bound base types: {b} and {t}"),
                        );
                    }
                }
                None => Some(t),
            };
            // Concrete targets must agree with the enforced base, just as
            // when two bound sets meet: `=="foo" & int` is eval-bottom.
            if let Some((demand, base)) =
                conflicting_demand(arena, &constraints, merged_base.as_ref())
            {
                return conflict(
                    arena,
                    format!("conflicting bound base types: {demand} and {base}"),
                );
            }
            // Ranges must overlap under the enforced base too: an integer
            // base admits no value between `>1` and `<2`.
            if let Some(message) = incompatible_range(arena, &constraints, merged_base.as_ref()) {
                return conflict(arena, message);
            }
            arena.alloc(Value::Bounds {
                base_type: merged_base,
                constraints,
            })
        }

        // Unifying Bounds with concrete value -> test all constraints against candidate
        Value::Int(ref i_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::Number(NumberKind::Int)
                && bt != TypeKind::Number(NumberKind::Number)
            {
                return conflict(arena, format!("type mismatch: expected {bt}, found int"));
            }
            for (op, target_id) in constraints {
                match arena.get(target_id) {
                    Some(Value::Int(t_val)) => {
                        let ok = match op {
                            Bound::Less => i_val < t_val,
                            Bound::LessEqual => i_val <= t_val,
                            Bound::Greater => i_val > t_val,
                            Bound::GreaterEqual => i_val >= t_val,
                            Bound::Equal => i_val == t_val,
                            Bound::NotEqual => i_val != t_val,
                            _ => false,
                        };
                        if !ok {
                            return conflict(
                                arena,
                                format!("value {i_val} does not satisfy bound {op} {t_val}"),
                            );
                        }
                    }
                    Some(Value::Float(t_val)) => {
                        let i_f = i_val.to_f64().unwrap_or(0.0);
                        let ok = match op {
                            Bound::Less => i_f < *t_val,
                            Bound::LessEqual => i_f <= *t_val,
                            Bound::Greater => i_f > *t_val,
                            Bound::GreaterEqual => i_f >= *t_val,
                            Bound::Equal => (i_f - *t_val).abs() <= f64::EPSILON,
                            Bound::NotEqual => (i_f - *t_val).abs() > f64::EPSILON,
                            _ => false,
                        };
                        if !ok {
                            return conflict(
                                arena,
                                format!("value {i_val} does not satisfy bound {op} {t_val}"),
                            );
                        }
                    }
                    _ => {
                        return conflict(arena, "bound target type mismatch for int");
                    }
                }
            }
            other_id
        }

        Value::Float(f_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::Number(NumberKind::Float)
                && bt != TypeKind::Number(NumberKind::Number)
            {
                return conflict(arena, format!("type mismatch: expected {bt}, found float"));
            }
            for (op, target_id) in constraints {
                let target_f = match arena.get(target_id) {
                    Some(Value::Float(t_val)) => Some(*t_val),
                    Some(Value::Int(t_val)) => t_val.to_f64(),
                    _ => None,
                };

                if let Some(t_val) = target_f {
                    let ok = match op {
                        Bound::Less => f_val < t_val,
                        Bound::LessEqual => f_val <= t_val,
                        Bound::Greater => f_val > t_val,
                        Bound::GreaterEqual => f_val >= t_val,
                        Bound::Equal => (f_val - t_val).abs() <= f64::EPSILON,
                        Bound::NotEqual => (f_val - t_val).abs() > f64::EPSILON,
                        _ => false,
                    };
                    if !ok {
                        return conflict(
                            arena,
                            format!("value {f_val} does not satisfy bound {op} {t_val}"),
                        );
                    }
                } else {
                    return conflict(arena, "bound target type mismatch for float");
                }
            }
            other_id
        }

        Value::String(ref s_val) => {
            if let Some(bt) = base_type
                && bt != TypeKind::String
            {
                return conflict(arena, format!("type mismatch: expected {bt}, found string"));
            }
            for (op, target_id) in constraints {
                // A concrete non-string target constrains the value to its
                // own kind (upstream: `"foo" & !=5` conflicts). Null and
                // not-yet-concrete targets demand nothing here.
                match arena.get(target_id) {
                    Some(Value::Int(_)) => {
                        return conflict(arena, "type mismatch: expected string, found int");
                    }
                    Some(Value::Float(_)) => {
                        return conflict(arena, "type mismatch: expected string, found float");
                    }
                    Some(Value::Bool(_)) => {
                        return conflict(arena, "type mismatch: expected string, found bool");
                    }
                    _ => {}
                }
                if let Some(Value::String(pattern)) = arena.get(target_id) {
                    match op {
                        Bound::Less
                        | Bound::LessEqual
                        | Bound::Greater
                        | Bound::GreaterEqual
                        | Bound::Equal => {
                            let ok = match op {
                                Bound::Less => s_val < pattern,
                                Bound::LessEqual => s_val <= pattern,
                                Bound::Greater => s_val > pattern,
                                Bound::GreaterEqual => s_val >= pattern,
                                _ => s_val == pattern,
                            };
                            if !ok {
                                return conflict(
                                    arena,
                                    format!(
                                        "string {s_val:?} does not satisfy bound {op} {pattern:?}"
                                    ),
                                );
                            }
                        }
                        Bound::NotEqual => {
                            if s_val == pattern {
                                return conflict(
                                    arena,
                                    format!(
                                        "string {s_val:?} does not satisfy bound {op} {pattern:?}"
                                    ),
                                );
                            }
                        }
                        Bound::RegexMatch => {
                            if let Some(re) = cached_regex(pattern) {
                                if !re.is_match(s_val) {
                                    return conflict(
                                        arena,
                                        format!(
                                            "string \"{s_val}\" does not match regex \"{pattern}\""
                                        ),
                                    );
                                }
                            } else {
                                return conflict(arena, format!("invalid regex: \"{pattern}\""));
                            }
                        }
                        Bound::RegexNotMatch => {
                            if let Some(re) = cached_regex(pattern) {
                                if re.is_match(s_val) {
                                    return conflict(
                                        arena,
                                        format!("string \"{s_val}\" matches regex \"{pattern}\""),
                                    );
                                }
                            } else {
                                return conflict(arena, format!("invalid regex: \"{pattern}\""));
                            }
                        }
                    }
                }
            }
            other_id
        }

        Value::Bool(ref b_val) => {
            for (op, target_id) in constraints {
                match arena.get(target_id) {
                    Some(Value::Bool(t_val)) => {
                        let ok = match op {
                            Bound::Equal => b_val == t_val,
                            Bound::NotEqual => b_val != t_val,
                            _ => false,
                        };
                        if !ok {
                            return conflict(
                                arena,
                                format!("value {b_val} does not satisfy bound {op} {t_val}"),
                            );
                        }
                    }
                    _ => {
                        return conflict(arena, "bound target type mismatch for bool");
                    }
                }
            }
            other_id
        }

        Value::List { .. } | Value::Struct(_) => {
            for (op, target_id) in constraints {
                if !matches!(
                    arena.get(target_id),
                    Some(Value::List { .. } | Value::Struct(_))
                ) {
                    return conflict(arena, "bound target type mismatch for composite");
                }
                let mut active = std::collections::BTreeSet::new();
                let mut budget = MAX_EQUIVALENCE_NODES;
                let same = equivalent(&*arena, target_id, other_id, &mut active, &mut budget);
                let ok = match op {
                    Bound::Equal => same,
                    Bound::NotEqual => !same,
                    _ => false,
                };
                if !ok {
                    return conflict(arena, format!("value does not satisfy bound {op}"));
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

        _ => conflict(arena, "cannot unify bound constraint with value"),
    }
}

pub fn field_matches_pattern(arena: &ValueArena, pattern_val: ValueId, field_name: &str) -> bool {
    match arena.get(pattern_val) {
        // `[_]` evaluates to bare Top, which the declaration-site check (a
        // plain unification against the field name) already accepts: every
        // name matches it there, so every name matches it here too.
        Some(Value::Top) => true,
        Some(Value::Type(TypeKind::String | TypeKind::Top)) => true,
        Some(Value::String(s)) => s == field_name,
        Some(Value::Bounds { constraints, .. }) => constraints.iter().all(|(op, target_id)| {
            if let Some(Value::String(pat)) = arena.get(*target_id)
                && let Some(re) = cached_regex(pat)
            {
                match op {
                    Bound::RegexMatch => re.is_match(field_name),
                    Bound::RegexNotMatch => !re.is_match(field_name),
                    _ => false,
                }
            } else {
                true
            }
        }),
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

/// A merged field is the unification of both sides' conjuncts, in written order:
/// the left literal's expression, then whatever overrode it.
fn merge_conjuncts(e1: &FieldEntry, e2: &FieldEntry) -> Vec<Conjunct> {
    let mut conjuncts = Vec::with_capacity(e1.conjuncts.len() + e2.conjuncts.len());
    conjuncts.extend(e1.conjuncts.iter().cloned());
    conjuncts.extend(e2.conjuncts.iter().cloned());
    conjuncts
}

/// Whether a bottom in a field should collapse the struct that holds it.
///
/// A conflict should: `{a: 1} & {a: 2}` is bottom, not a struct with a bottom
/// field, and every caller relies on that. A reference that has not resolved
/// should not. It is the evaluator saying *not yet*, the relaxation loop exists
/// because a later pass may say something else, and collapsing discards the very
/// siblings that would let it resolve - `stages: {build: …, test: {needs:
/// [stages.build]}}` loses `build` while `test` is still pending. Left where it
/// belongs, a reference that never resolves is reported at its own path, `x.r`
/// rather than `x`, which is how upstream reports it too.
pub(crate) fn collapses_struct(arena: &ValueArena, val: ValueId) -> bool {
    // This is a cached recipe result, which the evaluator replaces after the
    // merged fields have settled. It is not an immutable bottom conjunct.
    if arena.has_embedded_recipe(val) {
        return false;
    }
    match arena.get(val) {
        Some(Value::Bottom(reason)) => {
            !reason.kind.may_resolve_later()
                && reason.kind != BottomKind::Incomplete
                && reason.kind != BottomKind::Cycle
        }
        _ => false,
    }
}

/// The first regular field of `other` that closed struct `closed` does not
/// allow, if any. A field is allowed when `closed` declares it or one of its
/// pattern constraints matches the label. An optional field adds no value, so
/// upstream lets a closed struct meet one it does not declare.
fn disallowed_field<'a>(
    arena: &ValueArena,
    closed: &StructValue,
    other: &'a StructValue,
) -> Option<&'a str> {
    // Only a postfix spread reopens: an open literal unified with a closed
    // struct leaves it closed (`#x & {...}` still rejects new fields), while
    // a spread value keeps accepting them (`#Def... & {a, b}` accepts `b`,
    // and so does a later `& {c}`). `is_open` alone only blocks auto-closing.
    if !closed.is_closed || closed.spread_open {
        return None;
    }
    other
        .fields
        .iter()
        .filter(|(_, entry)| !entry.optional)
        .map(|(name, _)| name.as_str())
        .find(|name| {
            !closed.fields.contains_key(*name)
                && !closed
                    .pattern_constraints
                    .iter()
                    .any(|pc| field_matches_pattern(arena, pc.pattern_val, name))
        })
}

fn unify_structs_inner(
    arena: &mut ValueArena,
    s1: &StructValue,
    s2: &StructValue,
    ctx: &mut UnifyContext,
) -> ValueId {
    for (closed, other) in [(s1, s2), (s2, s1)] {
        if let Some(name) = disallowed_field(arena, closed, other) {
            return conflict(arena, format!("{name}: field not allowed"));
        }
    }

    let mut merged = StructValue::new(s1.is_closed || s2.is_closed);
    // A closed result carries no open marker: `#x & {...}` stays closed
    // (the marker is consumed by the merge), while an open result keeps it
    // (`#ServiceSpec & {port}` stays open through a definition boundary).
    // Spread permission is disjunctive instead: it survives `&`, and only a
    // definition read of a marker-less merge revokes it (see `close_deep`).
    merged.is_open = (s1.is_open || s2.is_open) && !merged.is_closed;
    merged.spread_open = s1.spread_open || s2.spread_open;

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
                if collapses_struct(arena, unified_val) && !(e1.optional && e2.optional) {
                    return unified_val;
                }
                FieldEntry::with_conjuncts(
                    unified_val,
                    e1.optional && e2.optional,
                    merge_conjuncts(e1, e2),
                )
            }
            (Some(e1), None) => e1.clone(),
            (None, Some(e2)) => e2.clone(),
            (None, None) => unreachable!(),
        };

        // Apply all pattern constraints matching this field
        let mut cur_val = entry.val;
        for pc in &merged.pattern_constraints {
            if field_matches_pattern(arena, pc.pattern_val, &key) {
                // A field that already failed keeps its own error: meeting it
                // with the pattern again would widen the failure to the whole
                // struct and hide the path the error belongs to.
                if collapses_struct(arena, cur_val) {
                    continue;
                }
                cur_val = unify_internal(arena, cur_val, pc.target_val, ctx);
                // A pattern-induced failure stays on the field: the error
                // belongs to this path, and collapsing would hide it from
                // the walk that reports it. A retryable bottom keeps meeting
                // later patterns; a settled one is left alone above.
                if collapses_struct(arena, cur_val) {
                    break;
                }
            }
        }

        merged.fields.insert(
            key,
            FieldEntry::with_conjuncts(cur_val, entry.optional, entry.conjuncts),
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
                if collapses_struct(arena, unified_val) {
                    return unified_val;
                }
                FieldEntry::with_conjuncts(
                    unified_val,
                    e1.optional && e2.optional,
                    merge_conjuncts(e1, e2),
                )
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
                if collapses_struct(arena, unified_val) {
                    return unified_val;
                }
                FieldEntry::with_conjuncts(
                    unified_val,
                    e1.optional && e2.optional,
                    merge_conjuncts(e1, e2),
                )
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
        return conflict(
            arena,
            format!("conflicting list lengths: {} and {}", e1.len(), e2.len()),
        );
    }
    if el1.is_none() && e2.len() > e1.len() {
        return conflict(
            arena,
            format!(
                "list length {} exceeds closed list length {}",
                e2.len(),
                e1.len()
            ),
        );
    }
    if el2.is_none() && e1.len() > e2.len() {
        return conflict(
            arena,
            format!(
                "list length {} exceeds closed list length {}",
                e1.len(),
                e2.len()
            ),
        );
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
    let res = crate::metadata::merged_choice_view(arena, res, disj_id, other_id, ctx);

    ctx.disjunction_depth = ctx.disjunction_depth.saturating_sub(1);
    ctx.active_disjunctions.remove(&pair);
    res
}

/// Keeps a branch only if no branch already kept holds the same content.
///
/// Unifying a disjunction against a structurally equal copy of itself examines
/// every pair and finds that half of them succeed - producing the branches it
/// already had. Pushing those blindly doubles the width for no change in
/// meaning, and four files each unifying one service disjunction took that to
/// 2^23 branches and 3.1 GB before the allocator refused.
///
/// Three things this has to get right. Each trial allocates a fresh node, so
/// the comparison is over content and not over [`ValueId`]. It runs as a branch
/// is kept rather than over the finished list, because it is quadratic in the
/// branches kept and the whole point is that the count stays small. And a
/// survivor inherits the default mark of every copy it absorbs, since losing it
/// would silently change which branch an ambiguous disjunction exports.
///
/// [`Equivalence::Unknown`] - the comparison budget running out on a large
/// branch - keeps the branch. A duplicate kept costs width; a distinct branch
/// dropped costs meaning.
pub(crate) fn push_branch(
    arena: &ValueArena,
    kept: &mut Vec<DisjunctionBranch>,
    branch: DisjunctionBranch,
) {
    for existing in kept.iter_mut() {
        if compare_values(arena, existing.val, branch.val) == Equivalence::Equal {
            existing.default |= branch.default;
            return;
        }
    }
    kept.push(branch);
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
        let d1_has_default = branches.iter().any(|b| b.default);
        let d2_has_default = other_branches.iter().any(|b| b.default);
        let mut valid_branches = Vec::new();
        let mut branch_errors = Vec::new();
        for b1 in branches {
            for b2 in &other_branches {
                let cp = arena.speculate();
                let u = unify_internal(arena, b1.val, b2.val, ctx);
                if !arena.has_embedded_recipe(u)
                    && let Some(Value::Bottom(b)) = arena.get(u)
                {
                    branch_errors.push(b.clone());
                    arena.rollback(cp);
                    continue;
                }
                let default = match (d1_has_default, d2_has_default) {
                    (true, true) => b1.default && b2.default,
                    (true, false) => b1.default,
                    (false, true) => b2.default,
                    (false, false) => false,
                };
                push_branch(
                    arena,
                    &mut valid_branches,
                    DisjunctionBranch { default, val: u },
                );
                arena.commit_speculation();
            }
        }
        return settle_disjunction(arena, valid_branches, &branch_errors);
    }

    let mut valid_branches = Vec::new();
    let mut branch_errors = Vec::new();

    for branch in branches {
        let cp = arena.speculate();
        let u = unify_internal(arena, branch.val, other_id, ctx);
        if !arena.has_embedded_recipe(u)
            && let Some(Value::Bottom(b)) = arena.get(u)
        {
            // This branch conflicted, rollback allocations made during the branch
            branch_errors.push(b.clone());
            arena.rollback(cp);
            continue;
        }
        push_branch(
            arena,
            &mut valid_branches,
            DisjunctionBranch {
                default: branch.default,
                val: u,
            },
        );
        arena.commit_speculation();
    }

    settle_disjunction(arena, valid_branches, &branch_errors)
}

/// The value of a disjunction once every branch has been tried.
///
/// A branch that failed only because a reference has not resolved yet has not
/// failed: the relaxation loop evaluates it again once the name is known. So
/// when no branch survived and one of them is still pending, the result is
/// pending too. `(int | [...]) & [a.b]` with `a` declared after it, or in
/// another file of the package, used to fail for good on the first pass.
pub(crate) fn settle_disjunction(
    arena: &mut ValueArena,
    mut valid_branches: Vec<DisjunctionBranch>,
    branch_errors: &[BottomReason],
) -> ValueId {
    match valid_branches.len() {
        0 => {
            let custom = branch_errors.iter().find(|e| e.kind == BottomKind::Custom);
            let kind = if branch_errors.iter().any(|e| e.kind.may_resolve_later()) {
                BottomKind::Unresolved
            } else {
                BottomKind::Conflict
            };
            let message = match custom {
                // A custom error names the failure: it wins over the
                // generated summary once nothing can still resolve.
                Some(reason) if kind != BottomKind::Unresolved => reason.message.clone(),
                _ => {
                    if branch_errors.is_empty() {
                        "no matching disjunction branch".to_string()
                    } else {
                        let errors: Vec<String> =
                            branch_errors.iter().map(ToString::to_string).collect();
                        format!("no matching disjunction branch: [{}]", errors.join("; "))
                    }
                }
            };
            arena.bottom_of(kind, message)
        }
        1 => {
            let branch = valid_branches.pop().unwrap().val;
            match arena.get(branch) {
                // One surviving failure adopts a custom message but keeps
                // its own code: `x+1 | error(m)` is incomplete, not eval.
                // A surviving value drops the custom branches entirely.
                Some(Value::Bottom(reason)) => {
                    let kind = reason.kind;
                    match branch_errors.iter().find(|e| e.kind == BottomKind::Custom) {
                        Some(custom) => arena.bottom_of(kind, custom.message.clone()),
                        None => branch,
                    }
                }
                _ => branch,
            }
        }
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
            | TypeKind::Number(NumberKind::Int)
            | TypeKind::Number(NumberKind::Number),
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
            | TypeKind::Number(NumberKind::Int)
            | TypeKind::Number(NumberKind::Number),
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
                        return conflict(
                            arena,
                            format!(
                                "string length {} is less than minimum runes {min}",
                                s.chars().count()
                            ),
                        );
                    }
                }
            } else if name.starts_with("strings.MaxRunes(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(max) = i.to_usize()
                {
                    if s.chars().count() <= max {
                        return candidate_id;
                    } else {
                        return conflict(
                            arena,
                            format!(
                                "string length {} exceeds maximum runes {max}",
                                s.chars().count()
                            ),
                        );
                    }
                }
            } else if name == "time.Time"
                && let Some(re) = cached_regex(
                    r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$",
                )
            {
                if re.is_match(s) {
                    return candidate_id;
                } else {
                    return conflict(
                        arena,
                        format!("string \"{s}\" is not a valid RFC3339 timestamp"),
                    );
                }
            } else if name == "time.Duration" {
                if crate::stdlib::time::parse_duration_nanos(s).is_ok() {
                    return candidate_id;
                } else {
                    return conflict(arena, format!("string \"{s}\" is not a valid duration"));
                }
            } else if name == "net.IPv4" {
                if s.parse::<std::net::Ipv4Addr>().is_ok() {
                    return candidate_id;
                } else {
                    return conflict(arena, format!("string \"{s}\" is not a valid IPv4 address"));
                }
            } else if name == "net.IPv6" {
                if s.parse::<std::net::Ipv6Addr>().is_ok() {
                    return candidate_id;
                } else {
                    return conflict(arena, format!("string \"{s}\" is not a valid IPv6 address"));
                }
            } else if name == "net.IP" {
                if s.parse::<std::net::IpAddr>().is_ok() {
                    return candidate_id;
                } else {
                    return conflict(arena, format!("string \"{s}\" is not a valid IP address"));
                }
            } else if name == "uuid.Valid" {
                if is_valid_uuid_str(s) {
                    return candidate_id;
                } else {
                    return conflict(arena, format!("string \"{s}\" is not a valid UUID"));
                }
            }
            conflict(
                arena,
                format!("validator '{name}' failed on string \"{s}\""),
            )
        }
        Value::Struct(ref s) => {
            if name.starts_with("struct.MinFields(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(min) = i.to_usize()
                {
                    if s.fields.len() >= min {
                        return candidate_id;
                    } else {
                        return conflict(
                            arena,
                            format!(
                                "struct has {} fields, expected at least {min}",
                                s.fields.len()
                            ),
                        );
                    }
                }
            } else if name.starts_with("struct.MaxFields(")
                && let Some(Value::Int(i)) = arena.get(target_id)
                && let Some(max) = i.to_usize()
            {
                if s.fields.len() <= max {
                    return candidate_id;
                } else {
                    return conflict(
                        arena,
                        format!(
                            "struct has {} fields, expected at most {max}",
                            s.fields.len()
                        ),
                    );
                }
            }
            conflict(arena, format!("validator '{name}' failed on struct"))
        }
        Value::List { ref elements, .. } => {
            if name.starts_with("list.MinItems(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(min) = i.to_usize()
                {
                    if elements.len() >= min {
                        return candidate_id;
                    } else {
                        return conflict(
                            arena,
                            format!(
                                "list length {} is less than minimum items {min}",
                                elements.len()
                            ),
                        );
                    }
                }
            } else if name.starts_with("list.MaxItems(") {
                if let Some(Value::Int(i)) = arena.get(target_id)
                    && let Some(max) = i.to_usize()
                {
                    if elements.len() <= max {
                        return candidate_id;
                    } else {
                        return conflict(
                            arena,
                            format!("list length {} exceeds maximum items {max}", elements.len()),
                        );
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
                        return conflict(arena, "list contains duplicate elements");
                    }
                }
                return candidate_id;
            } else if name.starts_with("list.MatchN") {
                return candidate_id;
            }
            conflict(arena, format!("validator '{name}' failed on list"))
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
                    return conflict(
                        arena,
                        format!("number {num} is not a multiple of {mod_val}"),
                    );
                }
            }
            conflict(arena, format!("validator '{name}' failed on integer {i}"))
        }
        _ => conflict(
            arena,
            format!("validator '{name}' is not applicable to value"),
        ),
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
    bytes
        .iter()
        .enumerate()
        .all(|(i, &b)| matches!(i, 8 | 13 | 18 | 23) || b.is_ascii_hexdigit())
}

/// Comparison budget for [`compare_values`], in node pairs. The walk is over a
/// graph that may share and repeat subtrees, and only the pairs on the current
/// path are remembered, so a comparison of two large values is bounded here
/// rather than paid in full.
const MAX_EQUIVALENCE_NODES: usize = 4096;

/// What comparing two values under the budget could establish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Equivalence {
    /// Same content throughout.
    Equal,
    /// A difference was found.
    Different,
    /// The budget ran out first. Neither answer is available, and a caller that
    /// must choose should note that "different" is not the cautious one: it is
    /// what a caller looking for a fixpoint would loop on forever.
    Unknown,
}

/// Whether two values hold the same content, not merely the same id.
///
/// Evaluating one expression twice yields two ids for one value, so identity
/// alone cannot tell a field that was overridden from a field that was derived
/// again. Everything reachable is compared, and a pair already under comparison
/// counts as equal, which is what makes a recursive schema terminate.
pub fn compare_values(arena: &ValueArena, a: ValueId, b: ValueId) -> Equivalence {
    let mut active = BTreeSet::new();
    let mut budget = MAX_EQUIVALENCE_NODES;
    if equivalent(arena, a, b, &mut active, &mut budget) {
        Equivalence::Equal
    } else if budget == 0 {
        Equivalence::Unknown
    } else {
        Equivalence::Different
    }
}

fn equivalent(
    arena: &ValueArena,
    a: ValueId,
    b: ValueId,
    active: &mut BTreeSet<(ValueId, ValueId)>,
    budget: &mut usize,
) -> bool {
    if a == b {
        return true;
    }
    if *budget == 0 {
        return false;
    }
    *budget -= 1;

    let pair = (a.min(b), a.max(b));
    if !active.insert(pair) {
        // Already being compared further up: a cycle, and a difference would have
        // shown on the way in.
        return true;
    }
    let result = equivalent_inner(arena, a, b, active, budget);
    active.remove(&pair);
    result
}

fn equivalent_inner(
    arena: &ValueArena,
    a: ValueId,
    b: ValueId,
    active: &mut BTreeSet<(ValueId, ValueId)>,
    budget: &mut usize,
) -> bool {
    match (arena.metadata(a), arena.metadata(b)) {
        (Some(left), Some(right)) => {
            if left.conjuncts().is_some() != right.conjuncts().is_some()
                || !equivalent(arena, left.fields, right.fields, active, budget)
                || !equivalent(
                    arena,
                    left.view.unwrap_or(left.fields),
                    right.view.unwrap_or(right.fields),
                    active,
                    budget,
                )
            {
                return false;
            }
        }
        (None, None) => {}
        _ => return false,
    }
    equivalent_payload(arena, a, b, active, budget)
}

/// Compare a refreshed payload without allocating an unannotated copy of its
/// previous value. Child values still include their retained fields.
pub(crate) fn same_payload(arena: &ValueArena, a: ValueId, b: ValueId) -> bool {
    let mut active = BTreeSet::from([(a.min(b), a.max(b))]);
    let mut budget = MAX_EQUIVALENCE_NODES;
    equivalent_payload(arena, a, b, &mut active, &mut budget)
}

fn equivalent_payload(
    arena: &ValueArena,
    a: ValueId,
    b: ValueId,
    active: &mut BTreeSet<(ValueId, ValueId)>,
    budget: &mut usize,
) -> bool {
    let (Some(left), Some(right)) = (arena.get(a), arena.get(b)) else {
        return false;
    };
    match (left, right) {
        (Value::Struct(left), Value::Struct(right)) => {
            if left.is_closed != right.is_closed
                || left.is_open != right.is_open
                || left.spread_open != right.spread_open
                || left.pattern_constraints.len() != right.pattern_constraints.len()
            {
                return false;
            }
            let sections = [
                (&left.fields, &right.fields),
                (&left.definitions, &right.definitions),
                (&left.hidden, &right.hidden),
            ];
            // An `if` + `return`, not a bare statement: the sections check
            // is an early exit, and a bare `.all()` would discard it.
            if !sections.into_iter().all(|(left, right)| {
                left.len() == right.len()
                    && left.iter().zip(right).all(
                        |((left_name, left_entry), (right_name, right_entry))| {
                            left_name == right_name
                                && left_entry.optional == right_entry.optional
                                && equivalent(
                                    arena,
                                    left_entry.val,
                                    right_entry.val,
                                    active,
                                    budget,
                                )
                        },
                    )
            }) {
                return false;
            }
            left.pattern_constraints
                .iter()
                .zip(&right.pattern_constraints)
                .all(|(left, right)| {
                    equivalent(arena, left.pattern_val, right.pattern_val, active, budget)
                        && equivalent(arena, left.target_val, right.target_val, active, budget)
                })
        }
        (
            Value::List {
                elements: left,
                ellipsis: left_tail,
            },
            Value::List {
                elements: right,
                ellipsis: right_tail,
            },
        ) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equivalent(arena, *left, *right, active, budget))
                && match (left_tail, right_tail) {
                    (None, None) => true,
                    (Some(left), Some(right)) => equivalent(arena, *left, *right, active, budget),
                    _ => false,
                }
        }
        (Value::Disjunction { branches: left }, Value::Disjunction { branches: right }) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    left.default == right.default
                        && equivalent(arena, left.val, right.val, active, budget)
                })
        }
        (
            Value::Bounds {
                base_type: left_type,
                constraints: left,
            },
            Value::Bounds {
                base_type: right_type,
                constraints: right,
            },
        ) => {
            left_type == right_type
                && left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    left.0 == right.0 && equivalent(arena, left.1, right.1, active, budget)
                })
        }
        (
            Value::BuiltinValidator {
                name: left_name,
                target: left,
            },
            Value::BuiltinValidator {
                name: right_name,
                target: right,
            },
        ) => left_name == right_name && equivalent(arena, *left, *right, active, budget),
        (Value::Validators(left), Value::Validators(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equivalent(arena, *left, *right, active, budget))
        }
        (
            Value::RecursiveRef {
                name: left_name,
                target: left,
            },
            Value::RecursiveRef {
                name: right_name,
                target: right,
            },
        ) => {
            left_name == right_name
                && match (left, right) {
                    (None, None) => true,
                    (Some(left), Some(right)) => equivalent(arena, *left, *right, active, budget),
                    _ => false,
                }
        }
        (left, right) => left == right,
    }
}
