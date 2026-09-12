use serde_json::Value;

/// Convert a JSON Schema JSON value into idiomatic CUE source code.
pub fn json_schema_to_cue(schema: &Value, root_name: Option<&str>) -> Result<String, String> {
    let mut out = String::new();

    // Check if standard library packages might be needed
    let schema_str = schema.to_string();
    let mut imports = Vec::new();
    if schema_str.contains("minLength") || schema_str.contains("maxLength") {
        imports.push("strings");
    }
    if schema_str.contains("minItems")
        || schema_str.contains("maxItems")
        || schema_str.contains("uniqueItems")
    {
        imports.push("list");
    }
    if schema_str.contains("minProperties") || schema_str.contains("maxProperties") {
        imports.push("struct");
    }

    for imp in imports {
        out.push_str(&format!("import \"{imp}\"\n"));
    }
    if !out.is_empty() {
        out.push('\n');
    }

    // Definitions ($defs or definitions)
    if let Some(defs) = schema.get("$defs").or_else(|| schema.get("definitions"))
        && let Some(defs_map) = defs.as_object()
    {
        for (def_name, def_val) in defs_map {
            let cue_expr = schema_val_to_cue(def_val, 1)?;
            out.push_str(&format!("#{def_name}: {cue_expr}\n\n"));
        }
    }

    // Root schema definition
    let root_def_name = root_name.unwrap_or("Schema");
    let root_expr = schema_val_to_cue(schema, 0)?;
    out.push_str(&format!("#{root_def_name}: {root_expr}\n"));

    Ok(out)
}

fn schema_val_to_cue(val: &Value, indent_level: usize) -> Result<String, String> {
    let indent = "\t".repeat(indent_level);
    let inner_indent = "\t".repeat(indent_level + 1);

    if let Some(r) = val.get("$ref").and_then(|v| v.as_str()) {
        let name = r.split('/').next_back().unwrap_or(r);
        return Ok(format!("#{name}"));
    }

    if let Some(enums) = val.get("enum").and_then(|v| v.as_array()) {
        let options: Vec<String> = enums
            .iter()
            .map(|e| match e {
                Value::String(s) => format!("\"{s}\""),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                _ => e.to_string(),
            })
            .collect();
        return Ok(options.join(" | "));
    }

    if let Some(one_of) = val
        .get("oneOf")
        .or_else(|| val.get("anyOf"))
        .and_then(|v| v.as_array())
    {
        let mut branches = Vec::new();
        for branch in one_of {
            branches.push(schema_val_to_cue(branch, indent_level)?);
        }
        return Ok(branches.join(" | "));
    }

    if let Some(all_of) = val.get("allOf").and_then(|v| v.as_array()) {
        let mut conjuncts = Vec::new();
        for conjunct in all_of {
            conjuncts.push(schema_val_to_cue(conjunct, indent_level)?);
        }
        return Ok(conjuncts.join(" & "));
    }

    let type_name = val.get("type").and_then(|v| v.as_str());

    match type_name {
        Some("string") => {
            let mut parts = vec!["string".to_string()];
            if let Some(pat) = val.get("pattern").and_then(|v| v.as_str()) {
                parts.push(format!("=~\"{pat}\""));
            }
            if let Some(min) = val.get("minLength").and_then(|v| v.as_u64()) {
                parts.push(format!("strings.MinRunes({min})"));
            }
            if let Some(max) = val.get("maxLength").and_then(|v| v.as_u64()) {
                parts.push(format!("strings.MaxRunes({max})"));
            }
            Ok(parts.join(" & "))
        }
        Some("integer") => {
            let mut parts = vec!["int".to_string()];
            add_numeric_bounds(val, &mut parts);
            Ok(parts.join(" & "))
        }
        Some("number") => {
            let mut parts = vec!["number".to_string()];
            add_numeric_bounds(val, &mut parts);
            Ok(parts.join(" & "))
        }
        Some("boolean") => Ok("bool".to_string()),
        Some("null") => Ok("null".to_string()),
        Some("array") => {
            let mut prefix = String::new();
            if let Some(min) = val.get("minItems").and_then(|v| v.as_u64()) {
                prefix.push_str(&format!("list.MinItems({min}) & "));
            }
            if let Some(max) = val.get("maxItems").and_then(|v| v.as_u64()) {
                prefix.push_str(&format!("list.MaxItems({max}) & "));
            }
            if let Some(uniq) = val.get("uniqueItems").and_then(|v| v.as_bool())
                && uniq
            {
                prefix.push_str("list.UniqueItems() & ");
            }

            if let Some(items) = val.get("items") {
                let item_cue = schema_val_to_cue(items, indent_level)?;
                Ok(format!("{prefix}[...{item_cue}]"))
            } else {
                Ok(format!("{prefix}[...]"))
            }
        }
        Some("object") | None => {
            if let Some(props) = val.get("properties").and_then(|v| v.as_object()) {
                let required_set: std::collections::HashSet<&str> = val
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                let mut lines = Vec::new();
                for (prop_name, prop_schema) in props {
                    let is_required = required_set.contains(prop_name.as_str());
                    let opt_mark = if is_required { "" } else { "?" };
                    let prop_cue = schema_val_to_cue(prop_schema, indent_level + 1)?;

                    // Safe identifier escaping
                    let formatted_name =
                        if prop_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            prop_name.clone()
                        } else {
                            format!("\"{prop_name}\"")
                        };

                    lines.push(format!(
                        "{inner_indent}{formatted_name}{opt_mark}: {prop_cue}"
                    ));
                }

                if lines.is_empty() {
                    Ok("{}".to_string())
                } else {
                    Ok(format!("{{\n{}\n{indent}}}", lines.join("\n")))
                }
            } else if val.is_object() && val.as_object().unwrap().is_empty() {
                Ok("{}".to_string())
            } else {
                Ok("_".to_string())
            }
        }
        Some(other) => Err(format!("Unsupported JSON Schema type: {other}")),
    }
}

fn add_numeric_bounds(val: &Value, parts: &mut Vec<String>) {
    if let Some(min) = val.get("minimum").and_then(|v| v.as_f64()) {
        parts.push(format!(">={min}"));
    }
    if let Some(min) = val.get("exclusiveMinimum").and_then(|v| v.as_f64()) {
        parts.push(format!(">{min}"));
    }
    if let Some(max) = val.get("maximum").and_then(|v| v.as_f64()) {
        parts.push(format!("<={max}"));
    }
    if let Some(max) = val.get("exclusiveMaximum").and_then(|v| v.as_f64()) {
        parts.push(format!("<{max}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_simple_json_schema() {
        let schema = json!({
            "type": "object",
            "required": ["name", "age"],
            "properties": {
                "name": { "type": "string", "minLength": 2 },
                "age": { "type": "integer", "minimum": 0, "maximum": 120 },
                "email": { "type": "string", "pattern": "^[a-z]+@[a-z]+\\.[a-z]+$" },
                "role": { "enum": ["admin", "user", "guest"] }
            }
        });

        let cue = json_schema_to_cue(&schema, Some("User")).unwrap();
        assert!(cue.contains("#User: {"));
        assert!(cue.contains("name: string & strings.MinRunes(2)"));
        assert!(cue.contains("age: int & >=0 & <=120"));
        assert!(cue.contains("email?: string & =~\"^[a-z]+@[a-z]+\\.[a-z]+$\""));
        assert!(cue.contains("role?: \"admin\" | \"user\" | \"guest\""));
    }

    #[test]
    fn test_convert_definitions_and_references() {
        let schema = json!({
            "$defs": {
                "Address": {
                    "type": "object",
                    "required": ["city"],
                    "properties": {
                        "city": { "type": "string" },
                        "zip": { "type": "string" }
                    }
                }
            },
            "type": "object",
            "required": ["address"],
            "properties": {
                "address": { "$ref": "#/$defs/Address" }
            }
        });

        let cue = json_schema_to_cue(&schema, Some("Company")).unwrap();
        assert!(cue.contains("#Address: {"));
        assert!(cue.contains("address: #Address"));
    }
}
