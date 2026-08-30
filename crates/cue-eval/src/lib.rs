pub mod eval;
pub mod importer;
pub mod manifest;
pub mod package;
pub mod stdlib;
pub mod unify;
pub mod value;

pub use eval::{EvalError, Evaluator};
pub use importer::{json_schema_to_cue, openapi_to_cue};
pub use manifest::{DependencyInfo, ModuleManifest};
pub use package::PackageLoader;
pub use stdlib::{is_known_package, StdlibFn, StdlibValidator};
pub use unify::unify;
pub use value::{BottomReason, BoundOp, StructValue, TypeKind, Value, ValueArena, ValueId};

/// Convenience function to evaluate a CUE string and export as JSON.
pub fn eval_to_json(source: &str) -> Result<serde_json::Value, EvalError> {
    let file = cue_syntax::parse_file(source)?;
    let mut evaluator = Evaluator::new();
    let root_id = evaluator.eval_file(&file)?;

    evaluator
        .to_json(root_id)
        .map_err(EvalError::Evaluation)
}

/// Validate a JSON value against a CUE schema string.
pub fn validate_json(schema_source: &str, data: &serde_json::Value) -> Result<(), EvalError> {
    let schema_file = cue_syntax::parse_file(schema_source)?;
    let mut evaluator = Evaluator::new();
    let schema_id = evaluator.eval_file(&schema_file)?;

    // If the schema file defines a `#Definition` without top-level regular fields,
    // validate against that `#Definition`.
    let target_schema_id = if let Some(Value::Struct(s)) = evaluator.arena.get(schema_id) {
        if s.fields.is_empty() && !s.definitions.is_empty() {
            s.definitions.values().next().unwrap().val
        } else {
            schema_id
        }
    } else {
        schema_id
    };

    let data_cue_str = data.to_string();
    let data_file = cue_syntax::parse_file(&data_cue_str)?;
    let data_id = evaluator.eval_file(&data_file)?;

    let res_id = unify(&mut evaluator.arena, target_schema_id, data_id);
    if let Some(Value::Bottom(b)) = evaluator.arena.get(res_id) {
        return Err(EvalError::Evaluation(format!("Validation failed: {b}")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unify_scalars_and_types() {
        let mut evaluator = Evaluator::new();
        let int_t = evaluator.arena.alloc(Value::Type(TypeKind::Int));
        let forty_two = evaluator.arena.int(42);

        let res = unify(&mut evaluator.arena, int_t, forty_two);
        assert_eq!(evaluator.arena.get(res), Some(&Value::Int(42.into())));
    }

    #[test]
    fn test_unify_type_mismatch_produces_bottom() {
        let mut evaluator = Evaluator::new();
        let str_t = evaluator.arena.alloc(Value::Type(TypeKind::String));
        let forty_two = evaluator.arena.int(42);

        let res = unify(&mut evaluator.arena, str_t, forty_two);
        assert!(matches!(evaluator.arena.get(res), Some(Value::Bottom(_))));
    }

    #[test]
    fn test_evaluate_and_export_json() {
        let cue_src = r#"
            package app

            name: "Antigravity"
            version: 2
            enabled: true
            tags: ["cue", "rust", "2024"]
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["name"], "Antigravity");
        assert_eq!(json["version"], 2);
        assert_eq!(json["enabled"], true);
        assert_eq!(json["tags"][0], "cue");
        assert_eq!(json["tags"][2], "2024");
    }

    #[test]
    fn test_disjunction_resolution() {
        let cue_src = r#"
            role: *"viewer" | "admin"
            role: "admin"
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["role"], "admin");
    }

    #[test]
    fn test_bounds_validation() {
        let cue_src = r#"
            port: int & >1024 & <65535
            port: 8080
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["port"], 8080);
    }

    #[test]
    fn test_closed_definition_rejection() {
        let mut evaluator = Evaluator::new();
        let mut def = StructValue::new(true); // closed
        let str_t = evaluator.arena.alloc(Value::Type(TypeKind::String));
        def.insert_field("allowed".to_string(), str_t, false);
        let def_id = evaluator.arena.alloc(Value::Struct(def));

        let mut concrete = StructValue::new(false);
        let val_id = evaluator.arena.string("extra");
        concrete.insert_field("forbidden".to_string(), val_id, false);
        let concrete_id = evaluator.arena.alloc(Value::Struct(concrete));

        let unified = unify(&mut evaluator.arena, def_id, concrete_id);
        assert!(matches!(evaluator.arena.get(unified), Some(Value::Bottom(_))));
    }

    #[test]
    fn test_pattern_constraint_validation() {
        let cue_src = r#"
            #EnvConfig: {
                [=~"^PORT_"]: int & >0 & <65535
                [=~"^HOST_"]: string
            }

            app: #EnvConfig & {
                PORT_HTTP: 8080
                PORT_GRPC: 9090
                HOST_API:  "api.internal"
            }
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["app"]["PORT_HTTP"], 8080);
        assert_eq!(json["app"]["PORT_GRPC"], 9090);
        assert_eq!(json["app"]["HOST_API"], "api.internal");
    }

    #[test]
    fn test_pattern_constraint_rejection() {
        let cue_src = r#"
            #EnvConfig: {
                [=~"^PORT_"]: int & >0 & <65535
            }

            app: #EnvConfig & {
                PORT_HTTP: 70000 // exceeds bound <65535
            }
        "#;
        assert!(eval_to_json(cue_src).is_err());
    }

    #[test]
    fn test_for_comprehension() {
        let cue_src = r#"
            items: ["a", "b", "c"]
            mapped: {
                for i, v in items {
                    (v): i
                }
            }
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["mapped"]["a"], 0);
        assert_eq!(json["mapped"]["b"], 1);
        assert_eq!(json["mapped"]["c"], 2);
    }

    #[test]
    fn test_if_comprehension() {
        let cue_src = r#"
            enabled: true
            config: {
                if enabled {
                    feature_flag: "on"
                }
            }
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["config"]["feature_flag"], "on");
    }

    #[test]
    fn test_builtin_functions() {
        let cue_src = r#"
            upper: strings.ToUpper("antigravity")
            floored: math.Floor(3.7)
            joined: strings.Join(["a", "b", "c"], "-")
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["upper"], "ANTIGRAVITY");
        assert_eq!(json["floored"], 3.0);
        assert_eq!(json["joined"], "a-b-c");
    }

    #[test]
    fn test_string_interpolation() {
        let cue_src = r#"
            name: "Alice"
            greeting: "Hello, \(name)!"
            calc: "Result is \(10 + 20)"
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["greeting"], "Hello, Alice!");
        assert_eq!(json["calc"], "Result is 30");
    }

    #[test]
    fn test_stdlib_validators() {
        let cue_src = r#"
            #User: {
                username: string & strings.MinRunes(3) & strings.MaxRunes(20)
                tags: list.MinItems(1) & list.UniqueItems()
                port: math.MultipleOf(10)
            }

            user1: #User & {
                username: "tonky"
                tags: ["admin", "dev"]
                port: 8080
            }
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["user1"]["username"], "tonky");
        assert_eq!(json["user1"]["port"], 8080);
    }

    #[test]
    fn test_stdlib_validator_rejection() {
        let cue_src = r#"
            #User: {
                tags: list.UniqueItems()
            }

            user1: #User & {
                tags: ["duplicate", "duplicate"]
            }
        "#;
        assert!(eval_to_json(cue_src).is_err());
    }

    #[test]
    fn test_recursive_definition() {
        let cue_src = r#"
            #Node: {
                val: int
                next?: #Node
            }

            root: #Node & {
                val: 1
                next: {
                    val: 2
                }
            }
        "#;
        let json = eval_to_json(cue_src).unwrap();
        assert_eq!(json["root"]["val"], 1);
        assert_eq!(json["root"]["next"]["val"], 2);
    }

    #[test]
    fn test_validate_json_api() {
        let schema = r#"
            #Config: {
                host: string
                port: int & >0 & <65535
            }
            payload: #Config
        "#;
        let valid_data = serde_json::json!({
            "payload": {
                "host": "localhost",
                "port": 8080
            }
        });
        assert!(validate_json(schema, &valid_data).is_ok());

        let invalid_data = serde_json::json!({
            "payload": {
                "host": "localhost",
                "port": 90000 // violates < 65535
            }
        });
        assert!(validate_json(schema, &invalid_data).is_err());
    }

    #[test]
    fn test_module_root_discovery() {
        let temp_dir = std::env::temp_dir().join(format!("cue_test_mod_{}", std::process::id()));
        let mod_dir = temp_dir.join("cue.mod");
        let sub_dir = temp_dir.join("pkg").join("service");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::create_dir_all(&sub_dir).unwrap();

        let mod_cue = r#"
            module: "example.com/kepler@v0"
            language: {
                version: "v0.9.0"
            }
        "#;
        std::fs::write(mod_dir.join("module.cue"), mod_cue).unwrap();

        let discovered = crate::package::PackageLoader::find_module_root(&sub_dir);
        assert!(discovered.is_some());
        let (root, info) = discovered.unwrap();
        assert_eq!(root, temp_dir);
        assert_eq!(info.module, "example.com/kepler@v0");
        assert_eq!(info.language_version.as_deref(), Some("v0.9.0"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_module_package_import_resolution() {
        let temp_dir = std::env::temp_dir().join(format!("cue_test_pkg_{}", std::process::id()));
        let mod_dir = temp_dir.join("cue.mod");
        let schema_dir = temp_dir.join("schema");
        let app_dir = temp_dir.join("app");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::create_dir_all(&app_dir).unwrap();

        let mod_cue = r#"module: "example.com/myapp""#;
        std::fs::write(mod_dir.join("module.cue"), mod_cue).unwrap();

        let schema_cue = r#"
            package schema
            #Config: {
                name: string
                port: int & >0
            }
        "#;
        std::fs::write(schema_dir.join("schema.cue"), schema_cue).unwrap();

        let app_cue = r#"
            package app
            import "example.com/myapp/schema"

            server: schema.#Config & {
                name: "api-server"
                port: 8080
            }
        "#;
        std::fs::write(app_dir.join("app.cue"), app_cue).unwrap();

        let loaded = crate::package::PackageLoader::load_dir(&app_dir);
        assert!(loaded.is_ok());
        let (eval, root_id) = loaded.unwrap();
        let json_val = eval.to_json(root_id).unwrap();
        assert_eq!(json_val["server"]["name"], "api-server");
        assert_eq!(json_val["server"]["port"], 8080);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_stdlib_package_registry() {
        assert!(is_known_package("strings"));
        assert!(is_known_package("crypto/sha256"));
        assert!(is_known_package("encoding/json"));
        assert!(is_known_package("math/bits"));
        assert!(!is_known_package("nonexistent/pkg"));
    }
}
