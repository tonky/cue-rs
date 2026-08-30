use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;

pub fn call_strings(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("strings", func_name) {
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
        ("strings", "TrimSpace") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(s.trim().to_string()));
                }
            Err("strings.TrimSpace requires 1 string argument".to_string())
        }
        ("strings", "TrimLeft") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(cutset))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let trimmed = s.trim_start_matches(|c| cutset.contains(c)).to_string();
                    return Ok(arena.string(trimmed));
                }
            Err("strings.TrimLeft requires (string, cutset) arguments".to_string())
        }
        ("strings", "TrimRight") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(cutset))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let trimmed = s.trim_end_matches(|c| cutset.contains(c)).to_string();
                    return Ok(arena.string(trimmed));
                }
            Err("strings.TrimRight requires (string, cutset) arguments".to_string())
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
        ("strings", "Replace") => {
            if args.len() >= 4
                && let (Some(Value::String(s)), Some(Value::String(old)), Some(Value::String(new)), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]), arena.get(args[3]))
                {
                    let count = n_val.to_isize().unwrap_or(-1);
                    let res = if count < 0 {
                        s.replace(old, new)
                    } else {
                        s.replacen(old, new, count as usize)
                    };
                    return Ok(arena.string(res));
                }
            Err("strings.Replace requires (s, old, new, n) arguments".to_string())
        }
        ("strings", "ReplaceAll") => {
            if args.len() >= 3
                && let (Some(Value::String(s)), Some(Value::String(old)), Some(Value::String(new))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                {
                    return Ok(arena.string(s.replace(old, new)));
                }
            Err("strings.ReplaceAll requires (s, old, new) string arguments".to_string())
        }
        ("strings", "TrimPrefixAny") => {
            if args.len() >= 2
                && let Some(Value::String(s)) = arena.get(args[0])
                && let Some(Value::List { elements, .. }) = arena.get(args[1]) {
                    let mut cur = s.clone();
                    let elems = elements.clone();
                    'outer: loop {
                        let mut changed = false;
                        for &elem in &elems {
                            if let Some(Value::String(pfx)) = arena.get(elem)
                                && let Some(stripped) = cur.strip_prefix(pfx.as_str()) {
                                    cur = stripped.to_string();
                                    changed = true;
                                    break;
                                }
                        }
                        if !changed {
                            break 'outer;
                        }
                    }
                    return Ok(arena.string(cur));
                }
            Err("strings.TrimPrefixAny requires (string, list_of_prefixes) arguments".to_string())
        }
        ("strings", "TrimSuffixAny") => {
            if args.len() >= 2
                && let Some(Value::String(s)) = arena.get(args[0])
                && let Some(Value::List { elements, .. }) = arena.get(args[1]) {
                    let mut cur = s.clone();
                    let elems = elements.clone();
                    'outer: loop {
                        let mut changed = false;
                        for &elem in &elems {
                            if let Some(Value::String(sfx)) = arena.get(elem)
                                && let Some(stripped) = cur.strip_suffix(sfx.as_str()) {
                                    cur = stripped.to_string();
                                    changed = true;
                                    break;
                                }
                        }
                        if !changed {
                            break 'outer;
                        }
                    }
                    return Ok(arena.string(cur));
                }
            Err("strings.TrimSuffixAny requires (string, list_of_suffixes) arguments".to_string())
        }
        ("strings", "Fields") => {
            let words_opt = if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    Some(s.split_whitespace().map(|w| w.to_string()).collect::<Vec<String>>())
                } else {
                    None
                };
            if let Some(words) = words_opt {
                let elems: Vec<ValueId> = words.into_iter().map(|w| arena.string(w)).collect();
                return Ok(arena.alloc(Value::List {
                    elements: elems,
                    ellipsis: None,
                }));
            }
            Err("strings.Fields requires 1 string argument".to_string())
        }
        ("strings", "Split") => {
            let parts_opt = if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sep))) = (arena.get(args[0]), arena.get(args[1])) {
                    Some(s.split(sep.as_str()).map(|part| part.to_string()).collect::<Vec<String>>())
                } else {
                    None
                };
            if let Some(parts) = parts_opt {
                let elems: Vec<ValueId> = parts.into_iter().map(|p| arena.string(p)).collect();
                return Ok(arena.alloc(Value::List {
                    elements: elems,
                    ellipsis: None,
                }));
            }
            Err("strings.Split requires (string, sep) arguments".to_string())
        }
        ("strings", "SplitN") => {
            let parts_opt = if args.len() >= 3
                && let (Some(Value::String(s)), Some(Value::String(sep)), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                && let Some(n) = n_val.to_isize() {
                    let parts: Vec<String> = if n == 0 {
                        vec![]
                    } else if n < 0 {
                        s.split(sep.as_str()).map(|part| part.to_string()).collect()
                    } else {
                        s.splitn(n as usize, sep.as_str()).map(|part| part.to_string()).collect()
                    };
                    Some(parts)
                } else {
                    None
                };
            if let Some(parts) = parts_opt {
                let elems: Vec<ValueId> = parts.into_iter().map(|p| arena.string(p)).collect();
                return Ok(arena.alloc(Value::List {
                    elements: elems,
                    ellipsis: None,
                }));
            }
            Err("strings.SplitN requires (string, sep, n) arguments".to_string())
        }
        ("strings", "HasPrefixAny") => {
            if args.len() >= 2
                && let Some(Value::String(s)) = arena.get(args[0])
                && let Some(Value::List { elements, .. }) = arena.get(args[1]) {
                    let s_clone = s.clone();
                    let elems = elements.clone();
                    for &elem in &elems {
                        if let Some(Value::String(pfx)) = arena.get(elem)
                            && s_clone.starts_with(pfx.as_str()) {
                                return Ok(arena.bool(true));
                            }
                    }
                    return Ok(arena.bool(false));
                }
            Err("strings.HasPrefixAny requires (string, list_of_prefixes) arguments".to_string())
        }
        ("strings", "HasSuffixAny") => {
            if args.len() >= 2
                && let Some(Value::String(s)) = arena.get(args[0])
                && let Some(Value::List { elements, .. }) = arena.get(args[1]) {
                    let s_clone = s.clone();
                    let elems = elements.clone();
                    for &elem in &elems {
                        if let Some(Value::String(sfx)) = arena.get(elem)
                            && s_clone.ends_with(sfx.as_str()) {
                                return Ok(arena.bool(true));
                            }
                    }
                    return Ok(arena.bool(false));
                }
            Err("strings.HasSuffixAny requires (string, list_of_suffixes) arguments".to_string())
        }
        ("strings", "Index") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sub))) = (arena.get(args[0]), arena.get(args[1])) {
                    let idx = s.find(sub.as_str()).map(|i| i as i64).unwrap_or(-1);
                    return Ok(arena.int(idx));
                }
            Err("strings.Index requires (string, substr) arguments".to_string())
        }
        ("strings", "LastIndex") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sub))) = (arena.get(args[0]), arena.get(args[1])) {
                    let idx = s.rfind(sub.as_str()).map(|i| i as i64).unwrap_or(-1);
                    return Ok(arena.int(idx));
                }
            Err("strings.LastIndex requires (string, substr) arguments".to_string())
        }
        ("strings", "Compare") => {
            if args.len() >= 2
                && let (Some(Value::String(a)), Some(Value::String(b))) = (arena.get(args[0]), arena.get(args[1])) {
                    let cmp = match a.cmp(b) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    };
                    return Ok(arena.int(cmp));
                }
            Err("strings.Compare requires (a, b) string arguments".to_string())
        }
        ("strings", "Count") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(sub))) = (arena.get(args[0]), arena.get(args[1])) {
                    let count = s.matches(sub.as_str()).count() as i64;
                    return Ok(arena.int(count));
                }
            Err("strings.Count requires (s, substr) string arguments".to_string())
        }
        ("strings", "Title") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(title_case(s)));
                }
            Err("strings.Title requires 1 string argument".to_string())
        }
        ("strings", "ContainsAny") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(chars))) = (arena.get(args[0]), arena.get(args[1])) {
                    let result = s.chars().any(|c| chars.contains(c));
                    return Ok(arena.bool(result));
                }
            Err("strings.ContainsAny requires (string, chars) arguments".to_string())
        }
        ("strings", "EqualFold") => {
            if args.len() >= 2
                && let (Some(Value::String(a)), Some(Value::String(b))) = (arena.get(args[0]), arena.get(args[1])) {
                    return Ok(arena.bool(a.eq_ignore_ascii_case(b)));
                }
            Err("strings.EqualFold requires 2 string arguments".to_string())
        }
        ("strings", "RuneCount") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.int(s.chars().count() as i64));
                }
            Err("strings.RuneCount requires 1 string argument".to_string())
        }
        ("strings", "ByteAt") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::Int(idx))) = (arena.get(args[0]), arena.get(args[1]))
                && let Some(i) = idx.to_usize() {
                    let bytes = s.as_bytes();
                    if i < bytes.len() {
                        return Ok(arena.int(bytes[i] as i64));
                    } else {
                        return Err(format!("strings.ByteAt: index {i} out of range for string of length {}", bytes.len()));
                    }
                }
            Err("strings.ByteAt requires (string, int) arguments".to_string())
        }
        ("strings", "ByteSlice") => {
            if args.len() >= 3
                && let (Some(Value::String(s)), Some(Value::Int(start_val)), Some(Value::Int(end_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                && let (Some(start), Some(end)) = (start_val.to_usize(), end_val.to_usize()) {
                    let bytes = s.as_bytes();
                    let end_clamped = end.min(bytes.len());
                    let start_clamped = start.min(end_clamped);
                    let slice = &bytes[start_clamped..end_clamped];
                    let result = String::from_utf8_lossy(slice).to_string();
                    return Ok(arena.string(result));
                }
            Err("strings.ByteSlice requires (string, start, end) arguments".to_string())
        }
        ("strings", "Runes") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    let runes: Vec<String> = s.chars().map(|c| c.to_string()).collect();
                    let elems: Vec<ValueId> = runes.into_iter().map(|r| arena.string(r)).collect();
                    return Ok(arena.alloc(Value::List {
                        elements: elems,
                        ellipsis: None,
                    }));
                }
            Err("strings.Runes requires 1 string argument".to_string())
        }
        ("strings", "SliceRunes") => {
            if args.len() >= 3
                && let (Some(Value::String(s)), Some(Value::Int(start_val)), Some(Value::Int(end_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                && let (Some(start), Some(end)) = (start_val.to_usize(), end_val.to_usize()) {
                    let chars: Vec<char> = s.chars().collect();
                    let end_clamped = end.min(chars.len());
                    let start_clamped = start.min(end_clamped);
                    let result: String = chars[start_clamped..end_clamped].iter().collect();
                    return Ok(arena.string(result));
                }
            Err("strings.SliceRunes requires (string, start, end) arguments".to_string())
        }
        ("strings", "ToValidUTF8") => {
            if args.len() >= 2
                && let (Some(Value::String(s)), Some(Value::String(repl))) =
                    (arena.get(args[0]), arena.get(args[1])) {
                    // Rust strings are always valid UTF-8, so just return the original
                    let _ = repl; // acknowledge replacement param
                    return Ok(arena.string(s.to_string()));
                }
            Err("strings.ToValidUTF8 requires (string, replacement) arguments".to_string())
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
        _ => Err(format!("unknown strings function: strings.{func_name}")),
    }
}

fn title_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                for u in c.to_uppercase() {
                    result.push(u);
                }
                capitalize_next = false;
            } else {
                for l in c.to_lowercase() {
                    result.push(l);
                }
            }
        } else {
            capitalize_next = true;
            result.push(c);
        }
    }
    result
}
