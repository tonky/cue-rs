use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;
use regex::Regex;

pub fn call_regexp(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("regexp", func_name) {
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
        ("regexp", "QuoteMeta") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(regex::escape(s)));
                }
            Err("regexp.QuoteMeta requires 1 string argument".to_string())
        }
        ("regexp", "FindSubmatch") => {
            let extracted = if args.len() >= 2
                && let (Some(Value::String(pat)), Some(Value::String(s))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    Some((pat.clone(), s.clone()))
                } else {
                    None
                };
            if let Some((pat, s)) = extracted {
                if let Ok(re) = Regex::new(&pat) {
                    if let Some(caps) = re.captures(&s) {
                        let match_strs: Vec<String> = caps.iter()
                            .map(|m| m.map(|v| v.as_str().to_string()).unwrap_or_default())
                            .collect();
                        let matches: Vec<ValueId> = match_strs.into_iter()
                            .map(|ms| arena.string(ms))
                            .collect();
                        return Ok(arena.alloc(Value::List {
                            elements: matches,
                            ellipsis: None,
                        }));
                    } else {
                        return Ok(arena.alloc(Value::List {
                            elements: vec![],
                            ellipsis: None,
                        }));
                    }
                } else {
                    return Err(format!("invalid regular expression: {pat}"));
                }
            }
            Err("regexp.FindSubmatch requires (pattern, string) arguments".to_string())
        }
        _ => Err(format!("unknown regexp function: regexp.{func_name}")),
    }
}
