//! Concrete-value serialization, shared by evaluator and builtin encoders.
use crate::value::{Value, ValueArena, ValueId};

pub fn to_json(arena: &ValueArena, val_id: ValueId) -> Result<serde_json::Value, String> {
    to_json_at_path(arena, val_id, "$")
}

pub(crate) fn to_json_at_path(
    arena: &ValueArena,
    val_id: ValueId,
    path: &str,
) -> Result<serde_json::Value, String> {
    match arena.get(val_id) {
        Some(Value::Null) => Ok(serde_json::Value::Null),
        Some(Value::Bool(b)) => Ok(serde_json::Value::Bool(*b)),
        Some(Value::Int(i)) => i.to_string().parse::<serde_json::Number>()
            .map(serde_json::Value::Number).map_err(|error| error.to_string()),
        Some(Value::Float(f)) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("non-finite floating-point result at '{path}' exceeds the evaluator's numeric range")),
        Some(Value::String(s)) => Ok(serde_json::Value::String(s.clone())),
        Some(Value::List { elements, .. }) => {
            let mut arr = Vec::with_capacity(elements.len());
            for (idx, &elem) in elements.iter().enumerate() {
                let elem_path = format!("{}[{}]", path, idx);
                arr.push(to_json_at_path(arena, elem, &elem_path)?);
            }
            Ok(serde_json::Value::Array(arr))
        }
        Some(Value::Struct(s)) => {
            let mut map = serde_json::Map::new();
            for (k, entry) in &s.fields {
                // An optional field is a constraint on a field that may
                // appear, not a field: `cue export` emits none of them
                // whatever they hold, down to `a?: 1` exporting `{}`. This
                // used to ask instead whether the constraint looked
                // concrete, which let `b?: {x?: int}` through as `{}` and
                // `c?: [...string]` as `[]`.
                if entry.optional {
                    continue;
                }
                let field_path = if path == "$" {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                map.insert(k.clone(), to_json_at_path(arena, entry.val, &field_path)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        Some(Value::Disjunction { branches }) => {
            let mut defaults = branches.iter().filter(|branch| branch.default);
            let first = defaults.next();
            if defaults.next().is_some() {
                if path == "$" {
                    Err("cannot export disjunction with ambiguous defaults to JSON".to_string())
                } else {
                    Err(format!(
                        "cannot export disjunction with ambiguous defaults at '{path}' to JSON"
                    ))
                }
            } else if let Some(default_branch) = first {
                to_json_at_path(arena, default_branch.val, path)
            } else if let [branch] = branches.as_slice() {
                to_json_at_path(arena, branch.val, path)
            } else if path == "$" {
                Err("cannot export non-concrete disjunction to JSON".to_string())
            } else {
                Err(format!(
                    "cannot export non-concrete disjunction at '{path}' to JSON"
                ))
            }
        }
        Some(Value::Bottom(b)) => {
            if path == "$" {
                Err(format!("cannot export bottom: {b}"))
            } else {
                Err(format!("cannot export bottom at '{path}': {b}"))
            }
        }
        Some(Value::Top) => {
            if path == "$" {
                Err("cannot export non-concrete top value to JSON".to_string())
            } else {
                Err(format!(
                    "cannot export non-concrete top value at '{path}' to JSON"
                ))
            }
        }
        Some(Value::Type(t)) => {
            if path == "$" {
                Err(format!("cannot export type {t} to JSON"))
            } else {
                Err(format!("cannot export type {t} at '{path}' to JSON"))
            }
        }
        Some(Value::Bounds { .. }) => {
            if path == "$" {
                Err("cannot export bound constraint to JSON".to_string())
            } else {
                Err(format!(
                    "cannot export bound constraint at '{path}' to JSON"
                ))
            }
        }
        Some(Value::BuiltinValidator { .. }) => {
            if path == "$" {
                Err("cannot export validator constraint to JSON".to_string())
            } else {
                Err(format!(
                    "cannot export validator constraint at '{path}' to JSON"
                ))
            }
        }
        Some(Value::Validators(_)) => {
            if path == "$" {
                Err("cannot export validator constraints to JSON".to_string())
            } else {
                Err(format!(
                    "cannot export validator constraints at '{path}' to JSON"
                ))
            }
        }
        // A required field still holding the lazy node that stops a
        // recursive definition expanding is precisely a structural cycle:
        // the value is infinite. It used to be exported as the internal
        // placeholder string. An *optional* recursive field never reaches
        // here, because the struct arm above drops it first - which is what
        // upstream does with `needs?: [...#Stage]` too.
        Some(Value::RecursiveRef { name, .. }) => {
            let name = name.clone();
            if path == "$" {
                Err(format!("structural cycle: '{name}'"))
            } else {
                Err(format!("structural cycle at '{path}': '{name}'"))
            }
        }
        Some(Value::Bytes(b)) => Ok(serde_json::Value::String(crate::binary::base64_encode(b))),
        None => Err("invalid value id".to_string()),
    }
}

/// Serialize JSON data as YAML without leaking serde_json's private exact-number
/// representation into the document. Numbers outside the YAML serializer's
/// range retain their exact JSON spelling in a YAML 1.2 flow document.
pub fn json_to_yaml(value: &serde_json::Value) -> Result<String, String> {
    if yaml_representable(value) {
        serde_yaml_ng::to_string(&YamlJson(value)).map_err(|error| error.to_string())
    } else {
        serde_json::to_string_pretty(value)
            .map(|mut document| {
                document.push('\n');
                document
            })
            .map_err(|error| error.to_string())
    }
}

enum YamlNumber {
    Signed(i128),
    Unsigned(u128),
    Float(f64),
}

impl YamlNumber {
    fn from_json(number: &serde_json::Number) -> Option<Self> {
        let text = number.to_string();
        if let Ok(n) = text.parse::<i128>() {
            return Some(Self::Signed(n));
        }
        if let Ok(n) = text.parse::<u128>() {
            return Some(Self::Unsigned(n));
        }
        let n = number.as_f64()?;
        // A decimal spelling which would change when serialized as f64 must
        // use the exact flow representation too (including enormous integers).
        (serde_json::Number::from_f64(n)?.to_string() == text).then_some(Self::Float(n))
    }
}

fn yaml_representable(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(n) => YamlNumber::from_json(n).is_some(),
        serde_json::Value::Array(values) => values.iter().all(yaml_representable),
        serde_json::Value::Object(fields) => fields.values().all(yaml_representable),
        _ => true,
    }
}

struct YamlJson<'a>(&'a serde_json::Value);

impl serde::Serialize for YamlJson<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self.0 {
            serde_json::Value::Null => serializer.serialize_none(),
            serde_json::Value::Bool(v) => serializer.serialize_bool(*v),
            serde_json::Value::String(v) => serializer.serialize_str(v),
            serde_json::Value::Number(n) => match YamlNumber::from_json(n) {
                Some(YamlNumber::Signed(n)) => serializer.serialize_i128(n),
                Some(YamlNumber::Unsigned(n)) => serializer.serialize_u128(n),
                Some(YamlNumber::Float(n)) => serializer.serialize_f64(n),
                None => Err(serde::ser::Error::custom(
                    "number requires exact JSON flow representation",
                )),
            },
            serde_json::Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&YamlJson(value))?;
                }
                sequence.end()
            }
            serde_json::Value::Object(fields) => {
                let mut mapping = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    mapping.serialize_entry(key, &YamlJson(value))?;
                }
                mapping.end()
            }
        }
    }
}
