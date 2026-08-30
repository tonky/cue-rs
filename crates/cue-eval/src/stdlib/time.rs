use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;
use regex::Regex;

pub fn call_time(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("time", func_name) {
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
        ("time", "Unix") => {
            if args.len() >= 2
                && let (Some(Value::Int(sec_val)), Some(Value::Int(_nsec_val))) = (arena.get(args[0]), arena.get(args[1]))
                && let Some(sec) = sec_val.to_i64() {
                    return Ok(arena.string(format_unix_rfc3339(sec)));
                }
            Err("time.Unix requires (sec, nsec) integer arguments".to_string())
        }
        ("time", "Parse") => {
            if args.len() >= 2
                && let (Some(Value::String(layout)), Some(Value::String(val))) = (arena.get(args[0]), arena.get(args[1])) {
                    match parse_time_layout(layout, val) {
                        Ok(rfc3339) => return Ok(arena.string(rfc3339)),
                        Err(e) => return Err(format!("time.Parse: {e}")),
                    }
                }
            Err("time.Parse requires (layout, value) string arguments".to_string())
        }
        ("time", "FormatDuration") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(nanos_val)) = arena.get(arg0)
                && let Some(nanos) = nanos_val.to_i64() {
                    return Ok(arena.string(format_duration_string(nanos)));
                }
            Err("time.FormatDuration requires 1 integer nanos argument".to_string())
        }
        ("time", "Hour") => Ok(arena.int(3_600_000_000_000i64)),
        ("time", "Minute") => Ok(arena.int(60_000_000_000i64)),
        ("time", "Second") => Ok(arena.int(1_000_000_000i64)),
        ("time", "Millisecond") => Ok(arena.int(1_000_000i64)),
        ("time", "Microsecond") => Ok(arena.int(1_000i64)),
        ("time", "Nanosecond") => Ok(arena.int(1i64)),
        ("time", "Year") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Some(y) = extract_time_part(s, 0, 4) {
                        return Ok(arena.int(y));
                    }
                    return Err(format!("time.Year: cannot extract year from \"{s}\""));
                }
            Err("time.Year requires 1 time string argument".to_string())
        }
        ("time", "Month") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Some(m) = extract_time_part(s, 5, 7) {
                        return Ok(arena.int(m));
                    }
                    return Err(format!("time.Month: cannot extract month from \"{s}\""));
                }
            Err("time.Month requires 1 time string argument".to_string())
        }
        ("time", "Day") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    if let Some(d) = extract_time_part(s, 8, 10) {
                        return Ok(arena.int(d));
                    }
                    return Err(format!("time.Day: cannot extract day from \"{s}\""));
                }
            Err("time.Day requires 1 time string argument".to_string())
        }
        _ => Err(format!("unknown time function: time.{func_name}")),
    }
}

pub fn is_valid_rfc3339(s: &str) -> bool {
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

fn format_unix_rfc3339(sec: i64) -> String {
    let days = sec.div_euclid(86400);
    let rem_sec = sec.rem_euclid(86400);
    let hour = rem_sec / 3600;
    let min = (rem_sec % 3600) / 60;
    let s = rem_sec % 60;

    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y_adj = if m <= 2 { y + 1 } else { y };

    format!("{y_adj:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{s:02}Z")
}

fn format_duration_string(nanos: i64) -> String {
    if nanos == 0 {
        return "0s".to_string();
    }
    let mut s = String::new();
    let mut rem = nanos;
    if rem < 0 {
        s.push('-');
        rem = -rem;
    }
    let hours = rem / 3_600_000_000_000;
    rem %= 3_600_000_000_000;
    let mins = rem / 60_000_000_000;
    rem %= 60_000_000_000;
    let secs = rem / 1_000_000_000;
    rem %= 1_000_000_000;
    let millis = rem / 1_000_000;
    rem %= 1_000_000;
    let micros = rem / 1_000;
    let nsec = rem % 1_000;

    if hours > 0 {
        s.push_str(&format!("{hours}h"));
    }
    if mins > 0 {
        s.push_str(&format!("{mins}m"));
    }
    if secs > 0 || (millis == 0 && micros == 0 && nsec == 0 && hours == 0 && mins == 0) {
        s.push_str(&format!("{secs}s"));
    }
    if millis > 0 {
        s.push_str(&format!("{millis}ms"));
    }
    if micros > 0 {
        s.push_str(&format!("{micros}µs"));
    }
    if nsec > 0 {
        s.push_str(&format!("{nsec}ns"));
    }
    s
}

fn parse_time_layout(layout: &str, val: &str) -> Result<String, String> {
    if layout == "2006-01-02" {
        let parts: Vec<&str> = val.split('-').collect();
        if parts.len() == 3 && parts[0].len() == 4 && parts[1].len() == 2 && parts[2].len() == 2 {
            let y: i32 = parts[0].parse().map_err(|_| "invalid year")?;
            let m: u32 = parts[1].parse().map_err(|_| "invalid month")?;
            let d: u32 = parts[2].parse().map_err(|_| "invalid day")?;
            if (1..=12).contains(&m) && (1..=31).contains(&d) {
                return Ok(format!("{y:04}-{m:02}-{d:02}T00:00:00Z"));
            }
        }
        return Err(format!("parsing time \"{val}\" as \"{layout}\": cannot parse"));
    }
    if layout == "2006-01-02 15:04:05" {
        let dt_parts: Vec<&str> = val.split_whitespace().collect();
        if dt_parts.len() == 2 {
            let d_parts: Vec<&str> = dt_parts[0].split('-').collect();
            let t_parts: Vec<&str> = dt_parts[1].split(':').collect();
            if d_parts.len() == 3 && t_parts.len() == 3 {
                let y: i32 = d_parts[0].parse().map_err(|_| "invalid year")?;
                let m: u32 = d_parts[1].parse().map_err(|_| "invalid month")?;
                let d: u32 = d_parts[2].parse().map_err(|_| "invalid day")?;
                let hr: u32 = t_parts[0].parse().map_err(|_| "invalid hour")?;
                let min: u32 = t_parts[1].parse().map_err(|_| "invalid minute")?;
                let sec: u32 = t_parts[2].parse().map_err(|_| "invalid second")?;
                return Ok(format!("{y:04}-{m:02}-{d:02}T{hr:02}:{min:02}:{sec:02}Z"));
            }
        }
        return Err(format!("parsing time \"{val}\" as \"{layout}\": cannot parse"));
    }
    if is_valid_rfc3339(val) {
        return Ok(val.to_string());
    }
    Err(format!("unsupported layout \"{layout}\""))
}

fn extract_time_part(s: &str, start: usize, end: usize) -> Option<i64> {
    if s.len() >= end {
        s[start..end].parse::<i64>().ok()
    } else {
        None
    }
}
