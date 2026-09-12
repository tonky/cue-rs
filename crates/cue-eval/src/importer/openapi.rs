use super::json_schema::json_schema_to_cue;
use serde_json::Value;

/// Convert an OpenAPI v3 specification JSON value into idiomatic CUE schemas.
pub fn openapi_to_cue(spec: &Value) -> Result<String, String> {
    let mut out = String::new();

    if let Some(title) = spec
        .get("info")
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
    {
        out.push_str(&format!("// OpenAPI Specification for: {title}\n\n"));
    }

    // Convert components.schemas
    if let Some(components) = spec.get("components")
        && let Some(schemas) = components.get("schemas").and_then(|s| s.as_object())
    {
        for (schema_name, schema_val) in schemas {
            let cue_block = json_schema_to_cue(schema_val, Some(schema_name))?;
            out.push_str(&cue_block);
            out.push('\n');
        }
    }

    // Convert definitions if OpenAPI 2.0 (Swagger)
    if let Some(definitions) = spec.get("definitions").and_then(|d| d.as_object()) {
        for (def_name, def_val) in definitions {
            let cue_block = json_schema_to_cue(def_val, Some(def_name))?;
            out.push_str(&cue_block);
            out.push('\n');
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_openapi_spec() {
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Pet Store API",
                "version": "1.0.0"
            },
            "components": {
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "required": ["id", "name"],
                        "properties": {
                            "id": { "type": "integer", "minimum": 1 },
                            "name": { "type": "string" },
                            "tag": { "type": "string" }
                        }
                    },
                    "Error": {
                        "type": "object",
                        "required": ["code", "message"],
                        "properties": {
                            "code": { "type": "integer" },
                            "message": { "type": "string" }
                        }
                    }
                }
            }
        });

        let cue = openapi_to_cue(&spec).unwrap();
        assert!(cue.contains("#Pet: {"));
        assert!(cue.contains("id: int & >=1"));
        assert!(cue.contains("name: string"));
        assert!(cue.contains("tag?: string"));
        assert!(cue.contains("#Error: {"));
        assert!(cue.contains("code: int"));
        assert!(cue.contains("message: string"));
    }
}
