use wasm_bindgen::prelude::*;

/// Evaluate CUE source code and return the output serialized in the requested format (json or yaml).
#[wasm_bindgen]
pub fn eval_cue(source: &str, format: Option<String>) -> Result<String, String> {
    let json_val = cue_eval::eval_to_json(source).map_err(|e| format!("Evaluation error: {e}"))?;

    let fmt = format.as_deref().unwrap_or("json").to_lowercase();
    match fmt.as_str() {
        "yaml" | "yml" => {
            serde_yaml_ng::to_string(&json_val).map_err(|e| format!("YAML serialization error: {e}"))
        }
        _ => serde_json::to_string_pretty(&json_val)
            .map_err(|e| format!("JSON serialization error: {e}")),
    }
}

/// Validate a JSON string payload against a CUE schema.
#[wasm_bindgen]
pub fn validate_json(schema_source: &str, json_payload: &str) -> Result<bool, String> {
    let json_data: serde_json::Value =
        serde_json::from_str(json_payload).map_err(|e| format!("Invalid JSON payload: {e}"))?;

    cue_eval::validate_json(schema_source, &json_data)
        .map(|_| true)
        .map_err(|e| format!("Validation failed: {e}"))
}

/// Format CUE source code using the AST pretty-printer.
#[wasm_bindgen]
pub fn format_cue(source: &str) -> Result<String, String> {
    let ast = cue_syntax::parse_file(source).map_err(|e| format!("Parse error: {e}"))?;

    Ok(cue_syntax::format_file(&ast))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_cue_json() {
        let source = "a: 1, b: 2, c: a + b";
        let res = eval_cue(source, None).unwrap();
        assert!(res.contains("\"c\": 3"));
    }

    #[test]
    fn test_validate_json() {
        let schema = "#User: { name: string, age: int & >=0 }\n#User";
        let valid = r#"{"name": "Alice", "age": 30}"#;
        assert!(validate_json(schema, valid).unwrap());

        let invalid = r#"{"name": "Alice", "age": -5}"#;
        assert!(validate_json(schema, invalid).is_err());
    }

    #[test]
    fn test_format_cue() {
        let source = "x:1\ny:2";
        let formatted = format_cue(source).unwrap();
        assert!(formatted.contains("x: 1"));
    }
}
