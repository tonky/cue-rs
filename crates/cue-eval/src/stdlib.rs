use crate::value::*;
use num_traits::{Signed, ToPrimitive};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdlibValidator {
    StringsMinRunes(usize),
    StringsMaxRunes(usize),
    ListMinItems(usize),
    ListMaxItems(usize),
    ListUniqueItems,
    MathMultipleOf(i64),
    StructMinFields(usize),
    StructMaxFields(usize),
    TimeRFC3339,
    NetIPv4,
    NetIPv6,
    NetIP,
    UuidValid,
}

impl StdlibValidator {
    pub fn validate(&self, arena: &ValueArena, val_id: ValueId) -> Result<(), String> {
        let val = match arena.get(val_id) {
            Some(v) => v,
            None => return Err("invalid node id".to_string()),
        };

        match self {
            StdlibValidator::StringsMinRunes(min) => match val {
                Value::String(s) => {
                    let count = s.chars().count();
                    if count >= *min {
                        Ok(())
                    } else {
                        Err(format!(
                            "string length {count} is less than minimum runes {min}"
                        ))
                    }
                }
                _ => Err("strings.MinRunes validator expects a string value".to_string()),
            },
            StdlibValidator::StringsMaxRunes(max) => match val {
                Value::String(s) => {
                    let count = s.chars().count();
                    if count <= *max {
                        Ok(())
                    } else {
                        Err(format!(
                            "string length {count} exceeds maximum runes {max}"
                        ))
                    }
                }
                _ => Err("strings.MaxRunes validator expects a string value".to_string()),
            },
            StdlibValidator::ListMinItems(min) => match val {
                Value::List { elements, .. } => {
                    if elements.len() >= *min {
                        Ok(())
                    } else {
                        Err(format!(
                            "list length {} is less than minimum items {min}",
                            elements.len()
                        ))
                    }
                }
                _ => Err("list.MinItems validator expects a list value".to_string()),
            },
            StdlibValidator::ListMaxItems(max) => match val {
                Value::List { elements, .. } => {
                    if elements.len() <= *max {
                        Ok(())
                    } else {
                        Err(format!(
                            "list length {} exceeds maximum items {max}",
                            elements.len()
                        ))
                    }
                }
                _ => Err("list.MaxItems validator expects a list value".to_string()),
            },
            StdlibValidator::ListUniqueItems => match val {
                Value::List { elements, .. } => {
                    let mut seen = HashSet::new();
                    for &elem in elements {
                        let repr = match arena.get(elem) {
                            Some(Value::String(s)) => format!("str:{s}"),
                            Some(Value::Int(i)) => format!("int:{i}"),
                            Some(Value::Float(f)) => format!("flt:{f}"),
                            Some(Value::Bool(b)) => format!("bool:{b}"),
                            _ => format!("id:{elem:?}"),
                        };
                        if !seen.insert(repr) {
                            return Err("list contains duplicate elements".to_string());
                        }
                    }
                    Ok(())
                }
                _ => Err("list.UniqueItems validator expects a list value".to_string()),
            },
            StdlibValidator::MathMultipleOf(n) => match val {
                Value::Int(i) => {
                    if let Some(num) = i.to_i64() {
                        if *n != 0 && num % n == 0 {
                            Ok(())
                        } else {
                            Err(format!("number {num} is not a multiple of {n}"))
                        }
                    } else {
                        Err("integer out of range for math.MultipleOf".to_string())
                    }
                }
                _ => Err("math.MultipleOf validator expects an integer value".to_string()),
            },
            StdlibValidator::StructMinFields(min) => match val {
                Value::Struct(s) => {
                    if s.fields.len() >= *min {
                        Ok(())
                    } else {
                        Err(format!(
                            "struct has {} fields, expected at least {min}",
                            s.fields.len()
                        ))
                    }
                }
                _ => Err("struct.MinFields validator expects a struct value".to_string()),
            },
            StdlibValidator::StructMaxFields(max) => match val {
                Value::Struct(s) => {
                    if s.fields.len() <= *max {
                        Ok(())
                    } else {
                        Err(format!(
                            "struct has {} fields, expected at most {max}",
                            s.fields.len()
                        ))
                    }
                }
                _ => Err("struct.MaxFields validator expects a struct value".to_string()),
            },
            StdlibValidator::TimeRFC3339 => match val {
                Value::String(s) => {
                    if is_valid_rfc3339(s) {
                        Ok(())
                    } else {
                        Err(format!("string \"{s}\" is not a valid RFC3339 timestamp"))
                    }
                }
                _ => Err("time.Time validator expects a string value".to_string()),
            },
            StdlibValidator::NetIPv4 => match val {
                Value::String(s) => {
                    if s.parse::<std::net::Ipv4Addr>().is_ok() {
                        Ok(())
                    } else {
                        Err(format!("string \"{s}\" is not a valid IPv4 address"))
                    }
                }
                _ => Err("net.IPv4 validator expects a string value".to_string()),
            },
            StdlibValidator::NetIPv6 => match val {
                Value::String(s) => {
                    if s.parse::<std::net::Ipv6Addr>().is_ok() {
                        Ok(())
                    } else {
                        Err(format!("string \"{s}\" is not a valid IPv6 address"))
                    }
                }
                _ => Err("net.IPv6 validator expects a string value".to_string()),
            },
            StdlibValidator::NetIP => match val {
                Value::String(s) => {
                    if s.parse::<std::net::IpAddr>().is_ok() {
                        Ok(())
                    } else {
                        Err(format!("string \"{s}\" is not a valid IP address"))
                    }
                }
                _ => Err("net.IP validator expects a string value".to_string()),
            },
            StdlibValidator::UuidValid => match val {
                Value::String(s) => {
                    if is_valid_uuid(s) {
                        Ok(())
                    } else {
                        Err(format!("string \"{s}\" is not a valid UUID"))
                    }
                }
                _ => Err("uuid.Valid validator expects a string value".to_string()),
            },
        }
    }
}

fn is_valid_uuid(s: &str) -> bool {
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

/// Helper function to convert a CUE ValueId to serde_json::Value for stdlib marshaling.
fn value_to_json(arena: &ValueArena, val_id: ValueId) -> Result<serde_json::Value, String> {
    match arena.get(val_id) {
        Some(Value::Null) => Ok(serde_json::Value::Null),
        Some(Value::Bool(b)) => Ok(serde_json::Value::Bool(*b)),
        Some(Value::Int(i)) => {
            if let Some(n) = i.to_i64() {
                Ok(serde_json::json!(n))
            } else {
                Ok(serde_json::Value::String(i.to_string()))
            }
        }
        Some(Value::Float(f)) => Ok(serde_json::json!(f)),
        Some(Value::String(s)) => Ok(serde_json::Value::String(s.clone())),
        Some(Value::List { elements, .. }) => {
            let mut arr = Vec::new();
            for &elem in elements {
                arr.push(value_to_json(arena, elem)?);
            }
            Ok(serde_json::Value::Array(arr))
        }
        Some(Value::Struct(s)) => {
            let mut map = serde_json::Map::new();
            for (k, entry) in &s.fields {
                map.insert(k.clone(), value_to_json(arena, entry.val)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Err("cannot marshal non-concrete value".to_string()),
    }
}

/// Helper function to convert a serde_json::Value into a CUE ValueId.
fn json_to_value(arena: &mut ValueArena, j: serde_json::Value) -> ValueId {
    match j {
        serde_json::Value::Null => arena.null(),
        serde_json::Value::Bool(b) => arena.bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                arena.int(i)
            } else if let Some(f) = n.as_f64() {
                arena.float(f)
            } else {
                arena.bottom("invalid json number")
            }
        }
        serde_json::Value::String(s) => arena.string(s),
        serde_json::Value::Array(arr) => {
            let elements = arr.into_iter().map(|item| json_to_value(arena, item)).collect();
            arena.alloc(Value::List {
                elements,
                ellipsis: None,
            })
        }
        serde_json::Value::Object(obj) => {
            let mut st = StructValue::new(false);
            for (k, v) in obj {
                let v_id = json_to_value(arena, v);
                st.insert_field(k, v_id, false);
            }
            arena.alloc(Value::Struct(st))
        }
    }
}

/// Dispatch CUE standard library function calls.
pub fn call_stdlib_func(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
        // --- strings package ---
        ("strings", "ToUpper") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(s.to_uppercase()));
                }
            Err("strings.ToUpper requires 1 string argument".to_string())
        }
        ("strings", "ToLower") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(s.to_lowercase()));
                }
            Err("strings.ToLower requires 1 string argument".to_string())
        }
        ("strings", "Contains") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sub))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    return Ok(arena.bool(s.contains(sub.as_str())));
                }
            Err("strings.Contains requires 2 string arguments".to_string())
        }
        ("strings", "HasPrefix") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(pfx))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    return Ok(arena.bool(s.starts_with(pfx.as_str())));
                }
            Err("strings.HasPrefix requires 2 string arguments".to_string())
        }
        ("strings", "HasSuffix") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sfx))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    return Ok(arena.bool(s.ends_with(sfx.as_str())));
                }
            Err("strings.HasSuffix requires 2 string arguments".to_string())
        }
        ("strings", "Join") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, .. }), Some(Value::String(sep))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let mut items = Vec::new();
                    for &elem in elements {
                        if let Some(Value::String(s)) = arena.get(elem) {
                            items.push(s.as_str());
                        } else {
                            return Err("strings.Join requires a list of strings".to_string());
                        }
                    }
                    return Ok(arena.string(items.join(sep.as_str())));
                }
            Err("strings.Join requires a list of strings and a separator".to_string())
        }
        ("strings", "Trim") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(cutset))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let cutset_chars: Vec<char> = cutset.chars().collect();
                    let trimmed = s.trim_matches(&cutset_chars[..]).to_string();
                    return Ok(arena.string(trimmed));
                }
            Err("strings.Trim requires 2 string arguments".to_string())
        }
        ("strings", "TrimPrefix") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(pfx))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let stripped = s.strip_prefix(pfx.as_str()).unwrap_or(s.as_str()).to_string();
                    return Ok(arena.string(stripped));
                }
            Err("strings.TrimPrefix requires 2 string arguments".to_string())
        }
        ("strings", "TrimSuffix") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sfx))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let stripped = s.strip_suffix(sfx.as_str()).unwrap_or(s.as_str()).to_string();
                    return Ok(arena.string(stripped));
                }
            Err("strings.TrimSuffix requires 2 string arguments".to_string())
        }
        ("strings", "Repeat") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::Int(count))) =
                    (arena.get(args[0]), arena.get(args[1]))
                    && let Some(n) = count.to_usize() {
                        return Ok(arena.string(s.repeat(n)));
                    }
            Err("strings.Repeat requires 1 string and 1 positive integer argument".to_string())
        }
        ("strings", "MinRunes") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("strings.MinRunes({n})"),
                            target,
                        }));
                    }
            Err("strings.MinRunes requires 1 positive integer argument".to_string())
        }
        ("strings", "MaxRunes") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("strings.MaxRunes({n})"),
                            target,
                        }));
                    }
            Err("strings.MaxRunes requires 1 positive integer argument".to_string())
        }

        // --- math package ---
        ("math", "Floor") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.floor()));
                }
            Err("math.Floor requires 1 float argument".to_string())
        }
        ("math", "Ceil") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.ceil()));
                }
            Err("math.Ceil requires 1 float argument".to_string())
        }
        ("math", "Round") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.round()));
                }
            Err("math.Round requires 1 float argument".to_string())
        }
        ("math", "Abs") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.abs()));
                } else if let Some(Value::Int(i)) = arena.get(arg0) {
                    return Ok(arena.int(i.abs()));
                }
            }
            Err("math.Abs requires 1 number argument".to_string())
        }
        ("math", "Sqrt") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.sqrt()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.sqrt()));
                    }
            }
            Err("math.Sqrt requires 1 number argument".to_string())
        }
        ("math", "Pow") => {
            if args.len() >= 2 {
                let base = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let exp = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(b), Some(e)) = (base, exp) {
                    return Ok(arena.float(b.powf(e)));
                }
            }
            Err("math.Pow requires 2 number arguments (base, exp)".to_string())
        }
        ("math", "Log") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.ln()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.ln()));
                    }
            }
            Err("math.Log requires 1 number argument".to_string())
        }
        ("math", "Sin") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.sin()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.sin()));
                    }
            }
            Err("math.Sin requires 1 number argument".to_string())
        }
        ("math", "Cos") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.cos()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.cos()));
                    }
            }
            Err("math.Cos requires 1 number argument".to_string())
        }
        ("math", "Max") => {
            if args.len() >= 2 {
                match (arena.get(args[0]), arena.get(args[1])) {
                    (Some(Value::Int(a)), Some(Value::Int(b))) => {
                        return Ok(arena.alloc(Value::Int(a.clone().max(b.clone()))));
                    }
                    (Some(Value::Float(a)), Some(Value::Float(b))) => {
                        return Ok(arena.float(a.max(*b)));
                    }
                    _ => {}
                }
            }
            Err("math.Max requires 2 comparable numbers".to_string())
        }
        ("math", "Min") => {
            if args.len() >= 2 {
                match (arena.get(args[0]), arena.get(args[1])) {
                    (Some(Value::Int(a)), Some(Value::Int(b))) => {
                        return Ok(arena.alloc(Value::Int(a.clone().min(b.clone()))));
                    }
                    (Some(Value::Float(a)), Some(Value::Float(b))) => {
                        return Ok(arena.float(a.min(*b)));
                    }
                    _ => {}
                }
            }
            Err("math.Min requires 2 comparable numbers".to_string())
        }
        ("math", "Pi") => {
            Ok(arena.float(std::f64::consts::PI))
        }
        ("math", "E") => {
            Ok(arena.float(std::f64::consts::E))
        }
        ("math", "MultipleOf") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_i64() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("math.MultipleOf({n})"),
                            target,
                        }));
                    }
            Err("math.MultipleOf requires 1 integer argument".to_string())
        }

        // --- list package ---
        ("list", "MinItems") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("list.MinItems({n})"),
                            target,
                        }));
                    }
            Err("list.MinItems requires 1 positive integer argument".to_string())
        }
        ("list", "MaxItems") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("list.MaxItems({n})"),
                            target,
                        }));
                    }
            Err("list.MaxItems requires 1 positive integer argument".to_string())
        }
        ("list", "UniqueItems") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "list.UniqueItems()".to_string(),
                target: dummy,
            }))
        }
        ("list", "Contains") => {
            if args.len() >= 2
                && let Some(Value::List { elements, .. }) = arena.get(args[0]) {
                    let target_id = args[1];
                    let contains = elements.contains(&target_id);
                    return Ok(arena.bool(contains));
                }
            Err("list.Contains requires 1 list and 1 element argument".to_string())
        }
        ("list", "Range") => {
            if args.len() >= 3
                && let (Some(Value::Int(start)), Some(Value::Int(end)), Some(Value::Int(step))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                    && let (Some(s), Some(e), Some(st)) = (start.to_i64(), end.to_i64(), step.to_i64())
                        && st != 0 {
                            let mut elements = Vec::new();
                            let mut cur = s;
                            while (st > 0 && cur < e) || (st < 0 && cur > e) {
                                elements.push(arena.int(cur));
                                cur += st;
                            }
                            return Ok(arena.alloc(Value::List {
                                elements,
                                ellipsis: None,
                            }));
                        }
            Err("list.Range requires start, end, step integer arguments".to_string())
        }
        ("list", "Take") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, ellipsis }), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let n = n_val.to_usize().unwrap_or(0).min(elements.len());
                    let taken = elements[..n].to_vec();
                    let el = *ellipsis;
                    return Ok(arena.alloc(Value::List {
                        elements: taken,
                        ellipsis: el,
                    }));
                }
            Err("list.Take requires 1 list and 1 count argument".to_string())
        }
        ("list", "Drop") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, ellipsis }), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let n = n_val.to_usize().unwrap_or(0).min(elements.len());
                    let dropped = elements[n..].to_vec();
                    let el = *ellipsis;
                    return Ok(arena.alloc(Value::List {
                        elements: dropped,
                        ellipsis: el,
                    }));
                }
            Err("list.Drop requires 1 list and 1 count argument".to_string())
        }
        ("list", "Sort") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let mut elems = elements.clone();
                    let el = *ellipsis;
                    elems.sort_by(|&a_id, &b_id| {
                        match (arena.get(a_id), arena.get(b_id)) {
                            (Some(Value::Int(a)), Some(Value::Int(b))) => a.cmp(b),
                            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
                            _ => std::cmp::Ordering::Equal,
                        }
                    });
                    return Ok(arena.alloc(Value::List {
                        elements: elems,
                        ellipsis: el,
                    }));
                }
            Err("list.Sort requires 1 list argument".to_string())
        }
        ("list", "FlattenN") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, .. }), Some(Value::Int(d_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let depth = d_val.to_usize().unwrap_or(1);
                    let flattened = flatten_list(arena, elements, depth);
                    return Ok(arena.alloc(Value::List {
                        elements: flattened,
                        ellipsis: None,
                    }));
                }
            Err("list.FlattenN requires 1 list and 1 depth integer argument".to_string())
        }

        // --- struct package ---
        ("struct", "MinFields") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("struct.MinFields({n})"),
                            target,
                        }));
                    }
            Err("struct.MinFields requires 1 positive integer argument".to_string())
        }
        ("struct", "MaxFields") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("struct.MaxFields({n})"),
                            target,
                        }));
                    }
            Err("struct.MaxFields requires 1 positive integer argument".to_string())
        }

        // --- time package ---
        ("time", "Time") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "time.Time".to_string(),
                target: dummy,
            }))
        }
        ("time", "Duration") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Ok(nanos) = parse_duration_nanos(s) {
                        return Ok(arena.int(nanos));
                    } else {
                        return Err(format!("invalid duration string: {s}"));
                    }
                }
            Err("time.Duration requires 1 duration string argument".to_string())
        }

        // --- encoding/json & json package ---
        ("json" | "encoding/json", "Marshal") => {
            if let Some(&arg0) = args.first() {
                let j = value_to_json(arena, arg0)?;
                let s = serde_json::to_string(&j).map_err(|e| e.to_string())?;
                return Ok(arena.string(s));
            }
            Err("json.Marshal requires 1 argument".to_string())
        }
        ("json" | "encoding/json", "Unmarshal") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let parsed: serde_json::Value =
                        serde_json::from_str(s).map_err(|e| format!("json.Unmarshal failed: {e}"))?;
                    return Ok(json_to_value(arena, parsed));
                }
            Err("json.Unmarshal requires 1 JSON string argument".to_string())
        }

        // --- encoding/yaml & yaml package ---
        ("yaml" | "encoding/yaml", "Marshal") => {
            if let Some(&arg0) = args.first() {
                let j = value_to_json(arena, arg0)?;
                let s = serde_yaml::to_string(&j).map_err(|e| e.to_string())?;
                return Ok(arena.string(s));
            }
            Err("yaml.Marshal requires 1 argument".to_string())
        }
        ("yaml" | "encoding/yaml", "Unmarshal") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let parsed: serde_json::Value =
                        serde_yaml::from_str(s).map_err(|e| format!("yaml.Unmarshal failed: {e}"))?;
                    return Ok(json_to_value(arena, parsed));
                }
            Err("yaml.Unmarshal requires 1 YAML string argument".to_string())
        }

        // --- path package ---
        ("path", "Base") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    let base = Path::new(p)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(p.as_str())
                        .to_string();
                    return Ok(arena.string(base));
                }
            Err("path.Base requires 1 path string argument".to_string())
        }
        ("path", "Dir") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    let dir = Path::new(p)
                        .parent()
                        .and_then(|s| s.to_str())
                        .unwrap_or(".")
                        .to_string();
                    return Ok(arena.string(dir));
                }
            Err("path.Dir requires 1 path string argument".to_string())
        }
        ("path", "Ext") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    let ext = Path::new(p)
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| format!(".{s}"))
                        .unwrap_or_default();
                    return Ok(arena.string(ext));
                }
            Err("path.Ext requires 1 path string argument".to_string())
        }
        ("path", "Join") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let mut buf = std::path::PathBuf::new();
                    for &elem in elements {
                        if let Some(Value::String(part)) = arena.get(elem) {
                            buf.push(part);
                        } else {
                            return Err("path.Join requires a list of strings".to_string());
                        }
                    }
                    return Ok(arena.string(buf.to_string_lossy().to_string()));
                }
            Err("path.Join requires a list of path string parts".to_string())
        }

        // --- regexp package ---
        ("regexp", "Valid") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(pat)) = arena.get(arg0) {
                    return Ok(arena.bool(Regex::new(pat).is_ok()));
                }
            Err("regexp.Valid requires 1 string pattern argument".to_string())
        }
        ("regexp", "Match") => {
            if args.len() >= 2
                && let (Some(Value::String(pat)), Some(Value::String(s))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    if let Ok(re) = Regex::new(pat) {
                        return Ok(arena.bool(re.is_match(s)));
                    } else {
                        return Err(format!("invalid regular expression: {pat}"));
                    }
                }
            Err("regexp.Match requires 1 regex pattern and 1 target string".to_string())
        }
        ("regexp", "Find") => {
            if args.len() >= 2
                && let (Some(Value::String(pat)), Some(Value::String(s))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    if let Ok(re) = Regex::new(pat) {
                        if let Some(m) = re.find(s) {
                            let matched = m.as_str().to_string();
                            return Ok(arena.string(matched));
                        } else {
                            return Ok(arena.bottom("regexp.Find: no match found"));
                        }
                    } else {
                        return Err(format!("invalid regular expression: {pat}"));
                    }
                }
            Err("regexp.Find requires 1 pattern and 1 target string".to_string())
        }
        ("regexp", "FindAll") => {
            if args.len() >= 3
                && let (Some(Value::String(pat)), Some(Value::String(s)), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                {
                    if let Ok(re) = Regex::new(pat) {
                        let limit = n_val.to_isize().unwrap_or(-1);
                        let mut matches_str = Vec::new();
                        for m in re.find_iter(s) {
                            if limit >= 0 && matches_str.len() as isize >= limit {
                                break;
                            }
                            matches_str.push(m.as_str().to_string());
                        }
                        let elements = matches_str.into_iter().map(|s| arena.string(s)).collect();
                        return Ok(arena.alloc(Value::List {
                            elements,
                            ellipsis: None,
                        }));
                    } else {
                        return Err(format!("invalid regular expression: {pat}"));
                    }
                }
            Err("regexp.FindAll requires 1 pattern, 1 target string, and 1 limit argument".to_string())
        }
        ("regexp", "ReplaceAll") => {
            if args.len() >= 3
                && let (Some(Value::String(pat)), Some(Value::String(s)), Some(Value::String(repl))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                {
                    if let Ok(re) = Regex::new(pat) {
                        let res = re.replace_all(s, repl.as_str()).to_string();
                        return Ok(arena.string(res));
                    } else {
                        return Err(format!("invalid regular expression: {pat}"));
                    }
                }
            Err("regexp.ReplaceAll requires 1 pattern, 1 target string, and 1 replacement".to_string())
        }

        // --- encoding/base64 package ---
        ("base64" | "encoding/base64", "Encode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(base64_encode(s.as_bytes())));
                }
            Err("base64.Encode requires 1 string argument".to_string())
        }
        ("base64" | "encoding/base64", "Decode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Ok(bytes) = base64_decode(s) {
                        return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                    } else {
                        return Err("invalid base64 string".to_string());
                    }
                }
            Err("base64.Decode requires 1 base64 string argument".to_string())
        }

        // --- encoding/hex package ---
        ("hex" | "encoding/hex", "Encode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let hex_str: String = s.bytes().map(|b| format!("{b:02x}")).collect();
                    return Ok(arena.string(hex_str));
                }
            Err("hex.Encode requires 1 string argument".to_string())
        }
        ("hex" | "encoding/hex", "Decode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Ok(bytes) = hex_decode(s) {
                        return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                    } else {
                        return Err("invalid hex string".to_string());
                    }
                }
            Err("hex.Decode requires 1 hex string argument".to_string())
        }

        // --- encoding/csv package ---
        ("csv" | "encoding/csv", "Decode") => {
            if let Some(&arg0) = args.first() {
                let s_opt = if let Some(Value::String(s)) = arena.get(arg0) {
                    Some(s.clone())
                } else {
                    None
                };
                if let Some(s) = s_opt {
                    let res = csv_decode(arena, &s);
                    return Ok(res);
                }
            }
            Err("csv.Decode requires 1 CSV string argument".to_string())
        }
        ("csv" | "encoding/csv", "Encode") => {
            if let Some(&arg0) = args.first() {
                let csv_text = csv_encode(arena, arg0)?;
                return Ok(arena.string(csv_text));
            }
            Err("csv.Encode requires 1 list argument".to_string())
        }

        // --- crypto/sha256 package ---
        ("sha256" | "crypto/sha256", "Sum") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(sha256_digest(s.as_bytes())));
                }
            Err("sha256.Sum requires 1 string argument".to_string())
        }

        // --- crypto/md5 package ---
        ("md5" | "crypto/md5", "Sum") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(md5_digest(s.as_bytes())));
                }
            Err("md5.Sum requires 1 string argument".to_string())
        }

        // --- crypto/sha1 package ---
        ("sha1" | "crypto/sha1", "Sum") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(sha1_digest(s.as_bytes())));
                }
            Err("sha1.Sum requires 1 string argument".to_string())
        }

        // --- net package ---
        ("net", "IPv4") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "net.IPv4".to_string(),
                target: dummy,
            }))
        }
        ("net", "IPv6") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "net.IPv6".to_string(),
                target: dummy,
            }))
        }
        ("net", "IP") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "net.IP".to_string(),
                target: dummy,
            }))
        }

        // --- strconv package ---
        ("strconv", "Atoi") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let cleaned = s.trim();
                    if let Ok(i) = num_bigint::BigInt::from_str(cleaned) {
                        return Ok(arena.int(i));
                    } else {
                        return Err(format!("strconv.Atoi: parsing \"{s}\": invalid syntax"));
                    }
                }
            Err("strconv.Atoi requires 1 string argument".to_string())
        }
        ("strconv", "Itoa") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0) {
                    return Ok(arena.string(i.to_string()));
                }
            Err("strconv.Itoa requires 1 integer argument".to_string())
        }
        ("strconv", "ParseFloat") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let cleaned = s.trim();
                    if let Ok(f) = cleaned.parse::<f64>() {
                        return Ok(arena.float(f));
                    } else {
                        return Err(format!("strconv.ParseFloat: parsing \"{s}\": invalid syntax"));
                    }
                }
            Err("strconv.ParseFloat requires 1 float string argument".to_string())
        }
        ("strconv", "FormatFloat") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.string(f.to_string()));
                } else if let Some(Value::Int(i)) = arena.get(arg0) {
                    return Ok(arena.string(format!("{i}.0")));
                }
            }
            Err("strconv.FormatFloat requires 1 float argument".to_string())
        }

        // --- uuid package ---
        ("uuid", "Valid") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "uuid.Valid".to_string(),
                target: dummy,
            }))
        }
        ("uuid", "Version") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if is_valid_uuid(s) {
                        let ver_char = s.chars().nth(14).unwrap_or('0');
                        let ver = ver_char.to_digit(10).unwrap_or(0) as i64;
                        return Ok(arena.int(ver));
                    } else {
                        return Err(format!("uuid.Version: invalid UUID string \"{s}\""));
                    }
                }
            Err("uuid.Version requires 1 string argument".to_string())
        }

        _ => Err(format!("unknown stdlib function: {pkg}.{func_name}")),
    }
}

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        out.push(B64_CHARS[(b0 >> 2) as usize] as char);
        out.push(B64_CHARS[(((b0 & 3) << 4) | (b1 >> 4)) as usize] as char);

        if i + 1 < input.len() {
            out.push(B64_CHARS[(((b1 & 15) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if i + 2 < input.len() {
            out.push(B64_CHARS[(b2 & 63) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut bytes = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;

    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err("invalid base64 char".to_string()),
        };
        buffer = (buffer << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    Ok(bytes)
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(2) {
        return Err("hex string length must be even".to_string());
    }
    let mut bytes = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    for chunk in chars.chunks(2) {
        let s: String = chunk.iter().collect();
        let b = u8::from_str_radix(&s, 16).map_err(|e| e.to_string())?;
        bytes.push(b);
    }
    Ok(bytes)
}

fn sha256_digest(input: &[u8]) -> String {
    // Pure Rust SHA-256 implementation
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_var = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_var.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_var = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_var);
    }

    h.iter().map(|x| format!("{x:08x}")).collect::<Vec<_>>().join("")
}

fn flatten_list(arena: &ValueArena, elements: &[ValueId], depth: usize) -> Vec<ValueId> {
    let mut out = Vec::new();
    for &elem in elements {
        if depth > 0
            && let Some(Value::List { elements: inner, .. }) = arena.get(elem) {
                out.extend(flatten_list(arena, inner, depth - 1));
                continue;
            }
        out.push(elem);
    }
    out
}

fn is_valid_rfc3339(s: &str) -> bool {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$").unwrap();
    re.is_match(s)
}

fn parse_duration_nanos(mut s: &str) -> Result<i64, String> {
    s = s.trim();
    if s.is_empty() {
        return Err("empty duration".to_string());
    }

    let mut total_nanos: i64 = 0;
    let mut chars = s.char_indices().peekable();

    while chars.peek().is_some() {
        // Read numeric part
        let start = chars.peek().unwrap().0;
        let mut end = start;
        let mut has_digit = false;
        while let Some(&(idx, ch)) = chars.peek() {
            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && start == idx) {
                has_digit = true;
                end = idx + ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        if !has_digit {
            return Err("missing number in duration".to_string());
        }

        let num_str = &s[start..end];
        let num: f64 = num_str.parse().map_err(|e| format!("invalid number in duration: {e}"))?;

        // Read unit part
        let unit_start = end;
        let mut unit_end = unit_start;
        while let Some(&(idx, ch)) = chars.peek() {
            if ch.is_alphabetic() || ch == 'µ' || ch == 'μ' {
                unit_end = idx + ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let unit_str = &s[unit_start..unit_end];
        let factor: f64 = match unit_str {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1_000.0,
            "ms" => 1_000_000.0,
            "s" => 1_000_000_000.0,
            "m" => 60.0 * 1_000_000_000.0,
            "h" => 3600.0 * 1_000_000_000.0,
            "d" => 86400.0 * 1_000_000_000.0,
            _ => return Err(format!("unknown duration unit: {unit_str}")),
        };

        total_nanos += (num * factor) as i64;
    }

    Ok(total_nanos)
}

fn md5_digest(input: &[u8]) -> String {
    let mut a: u32 = 0x67452301;
    let mut b: u32 = 0xefcdab89;
    let mut c: u32 = 0x98badcfe;
    let mut d: u32 = 0x10325476;

    let s = [
        7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,
        5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,
        4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,
        6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,
    ];

    let k: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        let mut aa = a;
        let mut bb = b;
        let mut cc = c;
        let mut dd = d;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((bb & cc) | ((!bb) & dd), i),
                16..=31 => ((dd & bb) | ((!dd) & cc), (5 * i + 1) % 16),
                32..=47 => (bb ^ cc ^ dd, (3 * i + 5) % 16),
                _ => (cc ^ (bb | (!dd)), (7 * i) % 16),
            };
            let temp = dd;
            dd = cc;
            cc = bb;
            bb = bb.wrapping_add((aa.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g])).rotate_left(s[i]));
            aa = temp;
        }

        a = a.wrapping_add(aa);
        b = b.wrapping_add(bb);
        c = c.wrapping_add(cc);
        d = d.wrapping_add(dd);
    }

    let mut result = String::with_capacity(32);
    for word in [a, b, c, d] {
        for byte in word.to_le_bytes() {
            result.push_str(&format!("{byte:02x}"));
        }
    }
    result
}

fn sha1_digest(input: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &w_i) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w_i);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = String::with_capacity(40);
    for word in [h0, h1, h2, h3, h4] {
        result.push_str(&format!("{word:08x}"));
    }
    result
}

fn csv_decode(arena: &mut ValueArena, input: &str) -> ValueId {
    let mut rows = Vec::new();
    let unescaped = input.replace("\\n", "\n").replace("\\r", "\r");
    for line in unescaped.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<ValueId> = trimmed
            .split(',')
            .map(|field| arena.string(field.trim().trim_matches('"').to_string()))
            .collect();
        let row_id = arena.alloc(Value::List {
            elements: fields,
            ellipsis: None,
        });
        rows.push(row_id);
    }
    arena.alloc(Value::List {
        elements: rows,
        ellipsis: None,
    })
}

fn csv_encode(arena: &ValueArena, val_id: ValueId) -> Result<String, String> {
    let rows = match arena.get(val_id) {
        Some(Value::List { elements, .. }) => elements,
        _ => return Err("csv.Encode expects a list of lists".to_string()),
    };
    let mut out = String::new();
    for (i, &row_id) in rows.iter().enumerate() {
        let fields = match arena.get(row_id) {
            Some(Value::List { elements, .. }) => elements,
            _ => return Err("csv.Encode expects inner list elements".to_string()),
        };
        let mut row_str = Vec::new();
        for &field_id in fields {
            match arena.get(field_id) {
                Some(Value::String(s)) => {
                    if s.contains(',') || s.contains('"') || s.contains('\n') {
                        row_str.push(format!("\"{}\"", s.replace('"', "\"\"")));
                    } else {
                        row_str.push(s.clone());
                    }
                }
                Some(Value::Int(i)) => row_str.push(i.to_string()),
                Some(Value::Float(f)) => row_str.push(f.to_string()),
                Some(Value::Bool(b)) => row_str.push(b.to_string()),
                _ => return Err("csv.Encode unsupported element type".to_string()),
            }
        }
        out.push_str(&row_str.join(","));
        if i + 1 < rows.len() {
            out.push('\n');
        }
    }
    Ok(out)
}
