use crate::value::{Value, ValueArena, ValueId};

pub fn is_valid_uuid(s: &str) -> bool {
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

pub fn call_uuid(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("uuid", func_name) {
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
                && let Some(Value::String(s)) = arena.get(arg0)
            {
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
        ("uuid", "URN") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if is_valid_uuid(s) {
                    return Ok(arena.string(format!("urn:uuid:{}", s.to_lowercase())));
                } else {
                    return Err(format!("uuid.URN: invalid UUID string \"{s}\""));
                }
            }
            Err("uuid.URN requires 1 string argument".to_string())
        }
        _ => Err(format!("unknown uuid function: uuid.{func_name}")),
    }
}
