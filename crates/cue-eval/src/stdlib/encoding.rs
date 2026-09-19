use crate::value::{StructValue, Value, ValueArena, ValueId};
use num_traits::ToPrimitive;

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const B32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const B32_HEX_CHARS: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";

pub fn value_to_json(arena: &ValueArena, val_id: ValueId) -> Result<serde_json::Value, String> {
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

pub fn json_to_value(arena: &mut ValueArena, j: serde_json::Value) -> ValueId {
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
            let elements = arr
                .into_iter()
                .map(|item| json_to_value(arena, item))
                .collect();
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

pub fn call_encoding(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
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
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let parsed: serde_json::Value =
                    serde_json::from_str(s).map_err(|e| format!("json.Unmarshal failed: {e}"))?;
                return Ok(json_to_value(arena, parsed));
            }
            Err("json.Unmarshal requires 1 JSON string argument".to_string())
        }
        ("json" | "encoding/json", "Indent") => {
            if args.len() >= 3
                && let (
                    Some(Value::String(s)),
                    Some(Value::String(prefix)),
                    Some(Value::String(_indent)),
                ) = (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
            {
                let parsed: serde_json::Value =
                    serde_json::from_str(s).map_err(|e| format!("json.Indent failed: {e}"))?;
                let indented = serde_json::to_string_pretty(&parsed)
                    .map_err(|e| format!("json.Indent failed: {e}"))?;
                return Ok(arena.string(format!("{prefix}{indented}")));
            }
            Err("json.Indent requires (json_string, prefix, indent) arguments".to_string())
        }
        ("json" | "encoding/json", "Compact") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let parsed: serde_json::Value =
                    serde_json::from_str(s).map_err(|e| format!("json.Compact failed: {e}"))?;
                let compact = serde_json::to_string(&parsed)
                    .map_err(|e| format!("json.Compact failed: {e}"))?;
                return Ok(arena.string(compact));
            }
            Err("json.Compact requires 1 JSON string argument".to_string())
        }
        ("json" | "encoding/json", "Valid") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let is_valid = serde_json::from_str::<serde_json::Value>(s).is_ok();
                return Ok(arena.bool(is_valid));
            }
            Err("json.Valid requires 1 JSON string argument".to_string())
        }
        ("json" | "encoding/json", "Validate") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let is_valid = serde_json::from_str::<serde_json::Value>(s).is_ok();
                return Ok(arena.bool(is_valid));
            }
            Err("json.Validate requires 1 JSON string argument".to_string())
        }

        // --- encoding/yaml & yaml package ---
        ("yaml" | "encoding/yaml", "Marshal") => {
            if let Some(&arg0) = args.first() {
                let j = value_to_json(arena, arg0)?;
                let s = serde_yaml_ng::to_string(&j).map_err(|e| e.to_string())?;
                return Ok(arena.string(s));
            }
            Err("yaml.Marshal requires 1 argument".to_string())
        }
        ("yaml" | "encoding/yaml", "Unmarshal") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let parsed: serde_json::Value =
                    serde_yaml_ng::from_str(s).map_err(|e| format!("yaml.Unmarshal failed: {e}"))?;
                return Ok(json_to_value(arena, parsed));
            }
            Err("yaml.Unmarshal requires 1 YAML string argument".to_string())
        }
        ("yaml" | "encoding/yaml", "Valid") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let is_valid = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(s).is_ok();
                return Ok(arena.bool(is_valid));
            }
            Err("yaml.Valid requires 1 YAML string argument".to_string())
        }
        ("yaml" | "encoding/yaml", "Validate") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let is_valid = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(s).is_ok();
                return Ok(arena.bool(is_valid));
            }
            Err("yaml.Validate requires 1 YAML string argument".to_string())
        }
        // --- encoding/base64 package ---
        ("base64" | "encoding/base64", "Encode") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    return Ok(arena.string(base64_encode(&b)));
                }
            }
            Err("base64.Encode requires 1 string or bytes argument".to_string())
        }
        ("base64" | "encoding/base64", "Decode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if let Ok(bytes) = base64_decode(s) {
                    return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                } else {
                    return Err("invalid base64 string".to_string());
                }
            }
            Err("base64.Decode requires 1 base64 string argument".to_string())
        }
        ("base64" | "encoding/base64", "RawURLEncode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                return Ok(arena.string(base64_url_encode(s.as_bytes(), false)));
            }
            Err("base64.RawURLEncode requires 1 string argument".to_string())
        }
        ("base64" | "encoding/base64", "RawURLDecode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if let Ok(bytes) = base64_url_decode(s) {
                    return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                } else {
                    return Err("invalid base64 raw URL string".to_string());
                }
            }
            Err("base64.RawURLDecode requires 1 string argument".to_string())
        }
        ("base64" | "encoding/base64", "URLEncode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                return Ok(arena.string(base64_url_encode(s.as_bytes(), true)));
            }
            Err("base64.URLEncode requires 1 string argument".to_string())
        }
        ("base64" | "encoding/base64", "URLDecode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if let Ok(bytes) = base64_url_decode(s) {
                    return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                } else {
                    return Err("invalid base64 URL string".to_string());
                }
            }
            Err("base64.URLDecode requires 1 string argument".to_string())
        }

        // --- encoding/base32 package ---
        ("base32" | "encoding/base32", "Encode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                return Ok(arena.string(base32_encode(s.as_bytes())));
            }
            Err("base32.Encode requires 1 string argument".to_string())
        }
        ("base32" | "encoding/base32", "Decode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if let Ok(bytes) = base32_decode(s) {
                    return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                } else {
                    return Err("invalid base32 string".to_string());
                }
            }
            Err("base32.Decode requires 1 base32 string argument".to_string())
        }
        ("base32" | "encoding/base32", "HexEncode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                return Ok(arena.string(base32_hex_encode(s.as_bytes())));
            }
            Err("base32.HexEncode requires 1 string argument".to_string())
        }
        ("base32" | "encoding/base32", "HexDecode") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                if let Ok(bytes) = base32_hex_decode(s) {
                    return Ok(arena.string(String::from_utf8_lossy(&bytes).to_string()));
                } else {
                    return Err("invalid base32 extended hex string".to_string());
                }
            }
            Err("base32.HexDecode requires 1 string argument".to_string())
        }

        // --- encoding/hex package ---
        ("hex" | "encoding/hex", "Encode") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    let hex_str: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
                    return Ok(arena.string(hex_str));
                }
            }
            Err("hex.Encode requires 1 string or bytes argument".to_string())
        }
        ("hex" | "encoding/hex", "Decode") => {
            if let Some(&arg0) = args.first() {
                let s_opt = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(Value::Bytes(b)) => String::from_utf8(b.clone()).ok(),
                    _ => None,
                };
                if let Some(s) = s_opt {
                    if let Ok(bytes) = hex_decode(&s) {
                        return Ok(arena.alloc(Value::Bytes(bytes)));
                    } else {
                        return Err("invalid hex string".to_string());
                    }
                }
            }
            Err("hex.Decode requires 1 hex string argument".to_string())
        }
        ("hex" | "encoding/hex", "Dump") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let dumped = s
                    .bytes()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Ok(arena.string(dumped));
            }
            Err("hex.Dump requires 1 string argument".to_string())
        }
        ("hex" | "encoding/hex", "EncodedLen") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(n_val)) = arena.get(arg0)
                && let Some(n) = n_val.to_i64()
            {
                return Ok(arena.int(n * 2));
            }
            Err("hex.EncodedLen requires 1 integer argument".to_string())
        }
        ("hex" | "encoding/hex", "DecodedLen") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(n_val)) = arena.get(arg0)
                && let Some(n) = n_val.to_i64()
            {
                return Ok(arena.int(n / 2));
            }
            Err("hex.DecodedLen requires 1 integer argument".to_string())
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

        // --- encoding/html package ---
        ("html" | "encoding/html", "Escape") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let escaped = s
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;")
                    .replace('\'', "&#39;");
                return Ok(arena.string(escaped));
            }
            Err("html.Escape requires 1 string argument".to_string())
        }
        ("html" | "encoding/html", "Unescape") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let unescaped = s
                    .replace("&quot;", "\"")
                    .replace("&#39;", "'")
                    .replace("&apos;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&");
                return Ok(arena.string(unescaped));
            }
            Err("html.Unescape requires 1 string argument".to_string())
        }
        _ => Err(format!("unknown encoding function: {pkg}.{func_name}")),
    }
}

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

fn base64_url_encode(input: &[u8], pad: bool) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        out.push(B64_URL_CHARS[(b0 >> 2) as usize] as char);
        out.push(B64_URL_CHARS[(((b0 & 3) << 4) | (b1 >> 4)) as usize] as char);

        if i + 1 < input.len() {
            out.push(B64_URL_CHARS[(((b1 & 15) << 2) | (b2 >> 6)) as usize] as char);
        } else if pad {
            out.push('=');
        }

        if i + 2 < input.len() {
            out.push(B64_URL_CHARS[(b2 & 63) as usize] as char);
        } else if pad {
            out.push('=');
        }

        i += 3;
    }
    out
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;

    for ch in input.trim().trim_end_matches('=').chars() {
        let val = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '-' => 62,
            '_' => 63,
            _ => return Err("invalid base64 url char".to_string()),
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

fn base32_encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u64;
        let b1 = if i + 1 < input.len() {
            input[i + 1] as u64
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            input[i + 2] as u64
        } else {
            0
        };
        let b3 = if i + 3 < input.len() {
            input[i + 3] as u64
        } else {
            0
        };
        let b4 = if i + 4 < input.len() {
            input[i + 4] as u64
        } else {
            0
        };

        let combined = (b0 << 32) | (b1 << 24) | (b2 << 16) | (b3 << 8) | b4;

        let rem = input.len() - i;
        let count = match rem {
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            _ => 8,
        };

        for c in 0..count {
            let shift = 35 - c * 5;
            let idx = ((combined >> shift) & 31) as usize;
            out.push(B32_ALPHABET[idx] as char);
        }

        for _ in count..8 {
            out.push('=');
        }

        i += 5;
    }
    out
}

fn base32_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim().trim_end_matches('=');
    let mut bytes = Vec::new();
    let mut buffer = 0u64;
    let mut bits = 0;

    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => ch as u64 - 'A' as u64,
            'a'..='z' => ch as u64 - 'a' as u64,
            '2'..='7' => ch as u64 - '2' as u64 + 26,
            _ => return Err("invalid base32 char".to_string()),
        };
        buffer = (buffer << 5) | val;
        bits += 5;

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

fn base32_hex_encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u64;
        let b1 = if i + 1 < input.len() {
            input[i + 1] as u64
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            input[i + 2] as u64
        } else {
            0
        };
        let b3 = if i + 3 < input.len() {
            input[i + 3] as u64
        } else {
            0
        };
        let b4 = if i + 4 < input.len() {
            input[i + 4] as u64
        } else {
            0
        };

        let chunk = (b0 << 32) | (b1 << 24) | (b2 << 16) | (b3 << 8) | b4;
        let rem_len = input.len() - i;

        out.push(B32_HEX_CHARS[((chunk >> 35) & 31) as usize] as char);
        out.push(B32_HEX_CHARS[((chunk >> 30) & 31) as usize] as char);

        if rem_len >= 2 {
            out.push(B32_HEX_CHARS[((chunk >> 25) & 31) as usize] as char);
            out.push(B32_HEX_CHARS[((chunk >> 20) & 31) as usize] as char);
        } else {
            out.push_str("======");
            break;
        }

        if rem_len >= 3 {
            out.push(B32_HEX_CHARS[((chunk >> 15) & 31) as usize] as char);
        } else {
            out.push_str("====");
            break;
        }

        if rem_len >= 4 {
            out.push(B32_HEX_CHARS[((chunk >> 10) & 31) as usize] as char);
            out.push(B32_HEX_CHARS[((chunk >> 5) & 31) as usize] as char);
        } else {
            out.push_str("===");
            break;
        }

        if rem_len >= 5 {
            out.push(B32_HEX_CHARS[(chunk & 31) as usize] as char);
        } else {
            out.push('=');
            break;
        }

        i += 5;
    }
    out
}

fn base32_hex_decode(input: &str) -> Result<Vec<u8>, ()> {
    let clean = input.trim().trim_end_matches('=');
    let mut out = Vec::new();
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for c in clean.chars() {
        let val = match c.to_ascii_uppercase() {
            '0'..='9' => (c as u8 - b'0') as u64,
            'A'..='V' => (c as u8 - b'A' + 10) as u64,
            _ => return Err(()),
        };
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push((buffer >> bits_left) as u8);
        }
    }
    Ok(out)
}
