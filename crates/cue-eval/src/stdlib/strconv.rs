use crate::value::{Value, ValueArena, ValueId};
use num_traits::{Signed, ToPrimitive};
use std::str::FromStr;

pub fn call_strconv(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("strconv", func_name) {
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
        ("strconv", "ParseBool") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    match s.trim().to_lowercase().as_str() {
                        "1" | "t" | "true" | "TRUE" | "True" => return Ok(arena.bool(true)),
                        "0" | "f" | "false" | "FALSE" | "False" => return Ok(arena.bool(false)),
                        _ => return Err(format!("strconv.ParseBool: parsing \"{s}\": invalid syntax")),
                    }
                }
            Err("strconv.ParseBool requires 1 boolean string argument".to_string())
        }
        ("strconv", "FormatBool") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Bool(b)) = arena.get(arg0) {
                    return Ok(arena.string(b.to_string()));
                }
            Err("strconv.FormatBool requires 1 boolean argument".to_string())
        }
        ("strconv", "ParseInt") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::Int(base_int))) = (arena.get(args[0]), arena.get(args[1])) {
                    let base = base_int.to_u32().unwrap_or(10);
                    let cleaned = s.trim();
                    if let Some(i) = num_bigint::BigInt::parse_bytes(cleaned.as_bytes(), base) {
                        return Ok(arena.int(i));
                    } else {
                        return Err(format!("strconv.ParseInt: parsing \"{s}\": invalid syntax"));
                    }
                }
            Err("strconv.ParseInt requires (string, base) arguments".to_string())
        }
        ("strconv", "ParseUint") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::Int(base_int))) = (arena.get(args[0]), arena.get(args[1])) {
                    let base = base_int.to_u32().unwrap_or(10);
                    let cleaned = s.trim();
                    if let Some(i) = num_bigint::BigInt::parse_bytes(cleaned.as_bytes(), base)
                        && i.sign() != num_bigint::Sign::Minus {
                            return Ok(arena.int(i));
                        } else {
                            return Err(format!("strconv.ParseUint: parsing \"{s}\": invalid syntax"));
                        }
                }
            Err("strconv.ParseUint requires (string, base) arguments".to_string())
        }
        ("strconv", "FormatInt") => {
            if args.len() >= 2
                && let (Some(Value::Int(i)), Some(Value::Int(base_int))) = (arena.get(args[0]), arena.get(args[1])) {
                    let base = base_int.to_u8().unwrap_or(10);
                    let is_neg = i.sign() == num_bigint::Sign::Minus;
                    let abs_i = i.abs();
                    let formatted = match base {
                        2 => format!("{abs_i:b}"),
                        8 => format!("{abs_i:o}"),
                        16 => format!("{abs_i:x}"),
                        _ => format!("{abs_i}"),
                    };
                    let result = if is_neg { format!("-{formatted}") } else { formatted };
                    return Ok(arena.string(result));
                }
            Err("strconv.FormatInt requires (int, base) arguments".to_string())
        }
        ("strconv", "Quote") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let quoted = format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t"));
                    return Ok(arena.string(quoted));
                }
            Err("strconv.Quote requires 1 string argument".to_string())
        }
        ("strconv", "Unquote") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let trimmed = s.trim();
                    if (trimmed.starts_with('"') && trimmed.ends_with('"')) || (trimmed.starts_with('`') && trimmed.ends_with('`')) {
                        let inner = &trimmed[1..trimmed.len()-1];
                        let unquoted = inner.replace("\\n", "\n").replace("\\t", "\t").replace("\\\"", "\"").replace("\\\\", "\\");
                        return Ok(arena.string(unquoted));
                    }
                    return Err(format!("strconv.Unquote: invalid quoted string {s}"));
                }
            Err("strconv.Unquote requires 1 string argument".to_string())
        }
        ("strconv", "FormatUint") => {
            if args.len() >= 2
                && let (Some(Value::Int(i)), Some(Value::Int(base_int))) = (arena.get(args[0]), arena.get(args[1])) {
                    let base = base_int.to_u8().unwrap_or(10);
                    if i.sign() != num_bigint::Sign::Minus {
                        match base {
                            2 => return Ok(arena.string(format!("{i:b}"))),
                            8 => return Ok(arena.string(format!("{i:o}"))),
                            16 => return Ok(arena.string(format!("{i:x}"))),
                            _ => return Ok(arena.string(i.to_string())),
                        }
                    } else {
                        return Err("strconv.FormatUint requires a non-negative integer".to_string());
                    }
                }
            Err("strconv.FormatUint requires (uint, base) arguments".to_string())
        }
        _ => Err(format!("unknown strconv function: strconv.{func_name}")),
    }
}
