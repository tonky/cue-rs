use cue_eval::{Evaluator, NumberKind, PackageLoader, TypeKind, Value, ValueId};
use cue_test_harness::conformance::{Check, Observation, Request, Response, Selector};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

pub fn worker(path: &Path) -> anyhow::Result<()> {
    let request: Request = serde_json::from_slice(&std::fs::read(path)?)?;
    let evaluated = PackageLoader::load_dir(&request.directory);
    let observations = request
        .checks
        .iter()
        .map(|check| {
            let observation = match &evaluated {
                Ok((evaluator, root)) => observe(evaluator, *root, check),
                Err(error) => Observation::Error(format!("package load failed: {error}")),
            };
            (check.id.clone(), observation)
        })
        .collect();
    println!("{}", serde_json::to_string(&Response { observations })?);
    Ok(())
}

fn observe(evaluator: &Evaluator, root: ValueId, check: &Check) -> Observation {
    let mut current = root;
    for selector in &check.path {
        let next = match selector {
            Selector::Field(name) => evaluator.arena.fields(current).and_then(|s| {
                s.fields
                    .get(name)
                    .or_else(|| s.definitions.get(name))
                    .or_else(|| s.hidden.get(name))
                    .map(|f| f.val)
            }),
            Selector::Index(index) => {
                if let Some(Value::List { elements, .. }) = evaluator.arena.get(current) {
                    elements.get(*index).copied()
                } else {
                    None
                }
            }
        };
        let Some(next) = next else {
            return Observation::Error(format!("assertion path missing at {selector:?}"));
        };
        current = next;
    }
    match check.operation.as_str() {
        // A struct standing over an error field is erroneous upstream, even
        // though only the leaf is bottom (`issue3778/full` observes `full`,
        // not `z.b`). The subtree search only fires when the value itself
        // is not bottom, so no passing check can flip.
        "error_present" => Observation::Value(json!(
            matches!(evaluator.arena.get(current), Some(Value::Bottom(_)))
                || subtree_error_code(evaluator, current).is_some()
        )),
        "error_code" => match evaluator.arena.get(current) {
            Some(Value::Bottom(reason)) => {
                let code = match bottom_code(&reason.kind) {
                    Some(code) => code,
                    None => {
                        return Observation::Unsupported(
                            "error cause is not typed precisely enough".into(),
                        );
                    }
                };
                Observation::Value(json!(code))
            }
            _ => match subtree_error_code(evaluator, current) {
                Some(code) => Observation::Value(json!(code)),
                None => Observation::Value(serde_json::Value::Null),
            },
        },
        "error_paths" => Observation::Unsupported(
            "absolute diagnostic paths and diagnostic sets are not yet available".into(),
        ),
        "export" => match evaluator.to_json(current) {
            Ok(value) => Observation::Value(value),
            Err(error) => Observation::Error(error),
        },
        "value" => match snapshot(evaluator, current, 0) {
            Ok(value) => Observation::Value(value),
            Err(error) => Observation::Unsupported(error),
        },
        "closed" => match evaluator.arena.get(current) {
            Some(Value::Struct(s)) => Observation::Value(json!(s.is_closed)),
            Some(Value::List { ellipsis, .. }) => Observation::Value(json!(ellipsis.is_none())),
            _ => Observation::Unsupported("closedness observation for non-composite value".into()),
        },
        "kind" => match kind(evaluator.arena.get(current)) {
            Some(kinds) => Observation::Value(json!(kinds)),
            None => Observation::Unsupported("kind observation for this value form".into()),
        },
        other => Observation::Unsupported(format!("operation {other}")),
    }
}

/// The oracle code for one bottom cause; `None` is not typed precisely
/// enough to report.
fn bottom_code(kind: &cue_eval::BottomKind) -> Option<&'static str> {
    use cue_eval::BottomKind;
    Some(match kind {
        BottomKind::Conflict
        | BottomKind::ReferenceNotFound
        | BottomKind::UndefinedFieldDefinite => "eval",
        // A resolved struct missing a field may gain it on a later pass —
        // the relaxation loop retries this kind for the same reason.
        // Upstream reports `incomplete`.
        BottomKind::UndefinedField | BottomKind::Incomplete => "incomplete",
        // A lone custom error settles as definite; inside a disjunction
        // the settle adopts a real code.
        BottomKind::Custom => "eval",
        BottomKind::Cycle | BottomKind::Unresolved => "cycle",
        BottomKind::StructuralCycle => "structural_cycle",
        BottomKind::Other => return None,
    })
}

/// The severest oracle code among the bottoms in a value's subtree, if any.
/// Disjunction interiors are not descended into: a failed alternative is
/// normal settling, not an error surfacing. Cycles in the arena are cut by
/// the visited set.
fn subtree_error_code(evaluator: &Evaluator, id: ValueId) -> Option<&'static str> {
    fn severity(code: &str) -> u8 {
        match code {
            "eval" => 4,
            "incomplete" => 3,
            "cycle" => 2,
            "structural_cycle" => 1,
            _ => 0,
        }
    }
    fn walk(
        evaluator: &Evaluator,
        id: ValueId,
        visited: &mut std::collections::HashSet<ValueId>,
        best: &mut Option<&'static str>,
    ) {
        if !visited.insert(id) {
            return;
        }
        let current_best = best.map(severity).unwrap_or(0);
        if current_best >= 4 {
            return;
        }
        match evaluator.arena.get(id) {
            Some(Value::Bottom(reason)) => {
                if let Some(code) = bottom_code(&reason.kind)
                    && severity(code) > current_best
                {
                    *best = Some(code);
                }
            }
            Some(Value::Struct(s)) => {
                for entry in s
                    .fields
                    .values()
                    .chain(s.definitions.values())
                    .chain(s.hidden.values())
                {
                    walk(evaluator, entry.val, visited, best);
                }
            }
            Some(Value::List { elements, .. }) => {
                for &element in elements {
                    walk(evaluator, element, visited, best);
                }
            }
            _ => {}
        }
    }
    let mut best = None;
    walk(
        evaluator,
        id,
        &mut std::collections::HashSet::new(),
        &mut best,
    );
    best
}

fn snapshot(e: &Evaluator, id: ValueId, depth: usize) -> Result<serde_json::Value, String> {
    if depth > 128 {
        return Err("observation depth limit".into());
    }
    Ok(match e.arena.get(id) {
        Some(Value::Null) => json!({"kind":"null"}),
        Some(Value::Bool(v)) => json!({"kind":"bool","value":v}),
        Some(Value::String(v)) => json!({"kind":"string","value":v}),
        Some(Value::Int(v)) => {
            json!({"kind":"number","value":cue_test_harness::conformance::canonical_number(&v.to_string()).ok_or("invalid numeric observation")?})
        }
        Some(Value::Float(v)) => {
            json!({"kind":"number","value":cue_test_harness::conformance::canonical_number(&v.to_string()).ok_or("non-finite numeric observation")?})
        }
        Some(Value::List {
            elements,
            ellipsis: None,
        }) => {
            json!({"kind":"list","items":elements.iter().map(|id| snapshot(e,*id,depth+1)).collect::<Result<Vec<_>,_>>()?})
        }
        Some(Value::Struct(s))
            if s.definitions.is_empty()
                && s.hidden.is_empty()
                && s.pattern_constraints.is_empty()
                && s.fields.values().all(|f| !f.optional) =>
        {
            let fields: BTreeMap<_, _> = s
                .fields
                .iter()
                .map(|(name, field)| Ok((name, snapshot(e, field.val, depth + 1)?)))
                .collect::<Result<_, String>>()?;
            json!({"kind":"struct","fields":fields})
        }
        _ => return Err(
            "abstract value, constraint, default, error or cycle requires structural observation"
                .into(),
        ),
    })
}

fn kind(value: Option<&Value>) -> Option<Vec<&'static str>> {
    let kind = match value? {
        Value::Null | Value::Type(TypeKind::Null) => "null",
        Value::Bool(_) | Value::Type(TypeKind::Bool) => "bool",
        Value::String(_) | Value::Type(TypeKind::String) => "string",
        Value::Bytes(_) | Value::Type(TypeKind::Bytes) => "bytes",
        Value::Int(_) => "int",
        Value::Float(_)
        | Value::Type(
            TypeKind::Number(NumberKind::Float)
            | TypeKind::Number(NumberKind::Float32)
            | TypeKind::Number(NumberKind::Float64),
        ) => "float",
        Value::Type(t) if t.is_integer() => "int",
        Value::Type(TypeKind::Number(NumberKind::Number)) => return Some(vec!["float", "int"]),
        Value::Struct(_) | Value::Type(TypeKind::Struct) => "struct",
        Value::List { .. } | Value::Type(TypeKind::List) => "list",
        _ => return None,
    };
    Some(vec![kind])
}
