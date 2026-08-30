use crate::value::{Value, ValueArena, ValueId};

pub fn call_tabwriter(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
// --- text/tabwriter package ---
        ("tabwriter" | "text/tabwriter", "Write") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::String(s)) = arena.get(arg0) {
                    return Ok(arena.string(tabwriter_write(s)));
                }
            Err("tabwriter.Write requires 1 string argument".to_string())
        }

        // --- text/template package ---
        ("template" | "text/template", "Execute") => {
            if args.len() >= 2
                && let Some(Value::String(templ)) = arena.get(args[0]) {
                    let res = template_execute(arena, templ, args[1])?;
                    return Ok(arena.string(res));
                }
            Err("template.Execute requires 1 template string and 1 data struct".to_string())
        }
        _ => Err(format!("unknown text function: {pkg}.{func_name}")),
    }
}

fn tabwriter_write(input: &str) -> String {
    let unescaped = input
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r");
    let lines: Vec<Vec<&str>> = unescaped
        .lines()
        .map(|line| line.split('\t').collect())
        .collect();

    let mut col_widths = Vec::new();
    for row in &lines {
        for (col_idx, &col) in row.iter().enumerate() {
            if col_idx >= col_widths.len() {
                col_widths.push(col.len());
            } else if col.len() > col_widths[col_idx] {
                col_widths[col_idx] = col.len();
            }
        }
    }

    let mut out = String::new();
    for (row_idx, row) in lines.iter().enumerate() {
        for (col_idx, &col) in row.iter().enumerate() {
            out.push_str(col);
            if col_idx + 1 < row.len() {
                let padding = col_widths[col_idx] + 2 - col.len();
                out.push_str(&" ".repeat(padding));
            }
        }
        if row_idx + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

fn template_execute(arena: &ValueArena, templ: &str, data_id: ValueId) -> Result<String, String> {
    let s = match arena.get(data_id) {
        Some(Value::Struct(s)) => s,
        _ => return Err("template.Execute requires a struct data argument".to_string()),
    };

    let mut result = templ.to_string();
    for (k, entry) in &s.fields {
        let val_str = match arena.get(entry.val) {
            Some(Value::String(str_val)) => str_val.clone(),
            Some(Value::Int(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            _ => continue,
        };
        let tag1 = format!("{{{{ .{k} }}}}");
        let tag2 = format!("{{{{.{k}}}}}");
        result = result.replace(&tag1, &val_str).replace(&tag2, &val_str);
    }
    Ok(result)
}
