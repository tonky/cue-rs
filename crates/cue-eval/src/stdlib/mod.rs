pub mod crypto;
pub mod encoding;
pub mod list;
pub mod math;
pub mod net;
pub mod path;
pub mod regexp;
pub mod strconv;
pub mod strings;
pub mod struct_pkg;
pub mod tabwriter;
pub mod time;
pub mod uuid;

pub use encoding::{json_to_value, value_to_json};
pub use time::is_valid_rfc3339;
pub use uuid::is_valid_uuid;

use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;
use std::collections::HashSet;

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

/// Dispatch CUE standard library function calls to modular domain packages.
pub fn call_stdlib_func(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match pkg {
        "strings" => strings::call_strings(arena, func_name, args),
        "math" | "bits" | "math/bits" => math::call_math(arena, pkg, func_name, args),
        "list" => list::call_list(arena, func_name, args),
        "struct" => struct_pkg::call_struct(arena, func_name, args),
        "time" => time::call_time(arena, func_name, args),
        "net" => net::call_net(arena, func_name, args),
        "path" | "path/filepath" => path::call_path(arena, pkg, func_name, args),
        "regexp" => regexp::call_regexp(arena, func_name, args),
        "strconv" => strconv::call_strconv(arena, func_name, args),
        "uuid" => uuid::call_uuid(arena, func_name, args),
        "tabwriter" | "text/tabwriter" | "template" | "text/template" => {
            tabwriter::call_tabwriter(arena, pkg, func_name, args)
        }
        "sha256" | "crypto/sha256" | "md5" | "crypto/md5" | "sha1" | "crypto/sha1"
        | "sha512" | "crypto/sha512" | "hmac" | "crypto/hmac" => {
            crypto::call_crypto(arena, pkg, func_name, args)
        }
        "json" | "encoding/json" | "yaml" | "encoding/yaml" | "base64" | "encoding/base64"
        | "base32" | "encoding/base32" | "hex" | "encoding/hex" | "csv" | "encoding/csv"
        | "html" | "encoding/html" | "toml" | "encoding/toml" => {
            encoding::call_encoding(arena, pkg, func_name, args)
        }
        _ => Err(format!("unknown stdlib package: {pkg}")),
    }
}
