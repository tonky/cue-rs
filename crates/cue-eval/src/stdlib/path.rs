use crate::value::{Value, ValueArena, ValueId};
use std::path::Path;

pub fn call_path(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
// --- path package ---
        ("path", "Clean") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    return Ok(arena.string(path_clean(p)));
                }
            Err("path.Clean requires 1 path string argument".to_string())
        }
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
        ("path", "Match") => {
            if args.len() >= 2
                && let (Some(Value::String(pattern)), Some(Value::String(name))) = (arena.get(args[0]), arena.get(args[1])) {
                    return Ok(arena.bool(glob_match(pattern, name)));
                }
            Err("path.Match requires (pattern, name) string arguments".to_string())
        }
        ("path", "Split") => {
            let parts_opt = if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    let (dir, file) = if let Some(pos) = p.rfind('/') {
                        (p[..=pos].to_string(), p[pos + 1..].to_string())
                    } else {
                        (String::new(), p.clone())
                    };
                    Some((dir, file))
                } else {
                    None
                };
            if let Some((dir, file)) = parts_opt {
                let dir_val = arena.string(dir);
                let file_val = arena.string(file);
                return Ok(arena.alloc(Value::List {
                    elements: vec![dir_val, file_val],
                    ellipsis: None,
                }));
            }
            Err("path.Split requires 1 path string argument".to_string())
        }
        ("path", "IsAbs") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(p)) = arena.get(arg0) {
                    return Ok(arena.bool(p.starts_with('/')));
                }
            Err("path.IsAbs requires 1 path string argument".to_string())
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
        _ => Err(format!("unknown path function: {pkg}.{func_name}")),
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    let mut regex_str = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex_str.push_str("[^/]*"),
            '?' => regex_str.push_str("[^/]"),
            '.' => regex_str.push_str("\\."),
            '+' => regex_str.push_str("\\+"),
            '(' => regex_str.push_str("\\("),
            ')' => regex_str.push_str("\\)"),
            '[' => regex_str.push('['),
            ']' => regex_str.push(']'),
            '{' => regex_str.push_str("\\{"),
            '}' => regex_str.push_str("\\}"),
            c => regex_str.push(c),
        }
    }
    regex_str.push('$');
    regex::Regex::new(&regex_str).map(|r| r.is_match(name)).unwrap_or(false)
}

fn path_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let is_abs = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if let Some(last) = parts.last()
                && *last != ".." {
                    parts.pop();
                    continue;
                }
            if !is_abs {
                parts.push("..");
            }
        } else {
            parts.push(seg);
        }
    }
    if is_abs {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}
