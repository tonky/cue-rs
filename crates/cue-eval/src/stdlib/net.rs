use crate::value::{Value, ValueArena, ValueId};

pub fn call_net(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("net", func_name) {
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
        ("net", "ParseIP") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let trimmed = s.trim();
                if let Ok(addr) = trimmed.parse::<std::net::IpAddr>() {
                    // Return the canonical string representation
                    return Ok(arena.string(addr.to_string()));
                } else {
                    return Err(format!("net.ParseIP: invalid IP address: \"{s}\""));
                }
            }
            Err("net.ParseIP requires 1 string argument".to_string())
        }
        ("net", "SplitHostPort") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                // Parse host:port or [host]:port
                let trimmed = s.trim();
                let (host, port) = if trimmed.starts_with('[') {
                    // IPv6: [host]:port
                    if let Some(bracket_end) = trimmed.find(']') {
                        let h = &trimmed[1..bracket_end];
                        let rest = &trimmed[bracket_end + 1..];
                        if let Some(p) = rest.strip_prefix(':') {
                            (h.to_string(), p.to_string())
                        } else {
                            return Err(format!("net.SplitHostPort: missing port in \"{s}\""));
                        }
                    } else {
                        return Err(format!("net.SplitHostPort: missing ']' in \"{s}\""));
                    }
                } else if let Some(colon_pos) = trimmed.rfind(':') {
                    // IPv4 or hostname: host:port
                    let h = &trimmed[..colon_pos];
                    let p = &trimmed[colon_pos + 1..];
                    (h.to_string(), p.to_string())
                } else {
                    return Err(format!("net.SplitHostPort: missing port in \"{s}\""));
                };
                let elems = vec![arena.string(host), arena.string(port)];
                return Ok(arena.alloc(Value::List {
                    elements: elems,
                    ellipsis: None,
                }));
            }
            Err("net.SplitHostPort requires 1 string argument".to_string())
        }
        ("net", "JoinHostPort") => {
            if args.len() >= 2
                && let (Some(Value::String(host)), Some(Value::String(port))) =
                    (arena.get(args[0]), arena.get(args[1]))
            {
                let result = if host.contains(':') {
                    // IPv6: wrap in brackets
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                };
                return Ok(arena.string(result));
            }
            Err("net.JoinHostPort requires (host, port) string arguments".to_string())
        }
        ("net", "FQDN") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0)
            {
                let domain = s.strip_suffix('.').unwrap_or(s);
                // RFC 1035: labels separated by dots, each 1-63 chars, total <= 253
                let valid = !domain.is_empty()
                    && domain.len() <= 253
                    && domain.split('.').all(|label| {
                        !label.is_empty()
                            && label.len() <= 63
                            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                            && !label.starts_with('-')
                            && !label.ends_with('-')
                    })
                    && domain.contains('.');
                return Ok(arena.bool(valid));
            }
            Err("net.FQDN requires 1 string argument".to_string())
        }
        _ => Err(format!("unknown net function: net.{func_name}")),
    }
}
