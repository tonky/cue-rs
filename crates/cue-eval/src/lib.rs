pub mod closedness;
pub mod deps;
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
pub use package::{LoadOptions, OriginAnnotation, PackageLoader};
pub use stdlib::{StdlibFn, StdlibValidator, is_known_package};
pub use unify::unify;
pub use value::{
    BottomKind, BottomReason, BoundOp, StructValue, TypeKind, Value, ValueArena, ValueId,
};

/// Convenience function to evaluate a CUE string and export as JSON.
pub fn eval_to_json(source: &str) -> Result<serde_json::Value, EvalError> {
    let file = cue_syntax::parse_file(source)?;
    let mut evaluator = Evaluator::new();
    let root_id = evaluator.eval_file(&file)?;

    evaluator.to_json(root_id).map_err(EvalError::Evaluation)
}

/// Trait for types that can validate themselves against a CUE schema.
pub trait CueValidate {
    fn cue_validate(&self) -> Result<(), String>;
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
            s.definitions
                .values()
                .next()
                .map(|entry| entry.val)
                .unwrap_or(schema_id)
        } else {
            schema_id
        }
    } else {
        schema_id
    };

    let data_cue_str = data.to_string();
    let data_file = cue_syntax::parse_file(&data_cue_str)?;
    let data_id = evaluator.eval_file(&data_file)?;

    let res_id = evaluator.unify_and_rederive(target_schema_id, data_id)?;
    if let Some(Value::Bottom(b)) = evaluator.arena.get(res_id) {
        return Err(EvalError::Evaluation(format!("Validation failed: {b}")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::*;

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
        assert!(matches!(
            evaluator.arena.get(unified),
            Some(Value::Bottom(_))
        ));
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
    fn test_recursive_service_dependency_cycle_graceful_resolution() {
        // Reproduces the exact cycle scenario reported in enve dev:
        // #Service references dependsOn?: [...#DependencyRef]
        // #DependencyRef references #ServiceDependency
        // #ServiceDependency references #Service via service: string | #Service
        let cue_src = r#"
            #DependencyConditionMode: "ready" | "healthy"
            #DependencyRef: string | #ServiceDependency
            #ServiceDependency: {
                service: string | #Service
                condition: #DependencyConditionMode
            }
            #Service: {
                name?: string
                dependsOn?: [...#DependencyRef]
            }

            backend: #Service & {
                name: "backend"
                dependsOn: [
                    #ServiceDependency & {
                        service: "postgres"
                        condition: "ready"
                    }
                ]
            }
        "#;
        let json = eval_to_json(cue_src).expect("should evaluate without stack overflow");
        assert_eq!(json["backend"]["name"], "backend");
        assert_eq!(json["backend"]["dependsOn"][0]["service"], "postgres");
        assert_eq!(json["backend"]["dependsOn"][0]["condition"], "ready");
    }

    #[test]
    fn test_unify_struct_cycle_detection() {
        let mut arena = ValueArena::new();
        let s1_id = arena.alloc(Value::Struct(StructValue::new(false)));
        let s2_id = arena.alloc(Value::Struct(StructValue::new(false)));

        // Mutually recursive structs: s1.next = s2, s2.next = s1
        if let Some(Value::Struct(s1)) = arena.get_mut(s1_id) {
            s1.insert_field("next".to_string(), s2_id, false);
        }
        if let Some(Value::Struct(s2)) = arena.get_mut(s2_id) {
            s2.insert_field("next".to_string(), s1_id, false);
        }

        // Unifying s1 with s2 would cause infinite recursion without cycle detection
        let result_id = unify(&mut arena, s1_id, s2_id);
        let result_val = arena.get(result_id);
        assert!(
            matches!(result_val, Some(Value::Bottom(b)) if b.message.contains("cycle error: cyclic")),
            "expected cyclic unification detected, got: {:?}",
            result_val
        );
    }

    #[test]
    fn test_unify_struct_recursion_depth_limit() {
        let mut arena = ValueArena::new();
        // Create twin chains of nested structs deeper than MAX_STRUCT_DEPTH (64)
        let mut chain1 = arena.alloc(Value::Struct(StructValue::new(false)));
        let mut chain2 = arena.alloc(Value::Struct(StructValue::new(false)));
        for i in 0..70 {
            let mut s1 = StructValue::new(false);
            s1.insert_field(format!("f{i}"), chain1, false);
            chain1 = arena.alloc(Value::Struct(s1));

            let mut s2 = StructValue::new(false);
            s2.insert_field(format!("f{i}"), chain2, false);
            chain2 = arena.alloc(Value::Struct(s2));
        }

        let res_id = unify(&mut arena, chain1, chain2);
        let res_val = arena.get(res_id);
        assert!(
            matches!(res_val, Some(Value::Bottom(b)) if b.message.contains("struct recursion depth limit exceeded")),
            "expected struct recursion depth limit exceeded, got: {:?}",
            res_val
        );
    }

    #[test]
    fn test_unify_disjunction_cycle_detection() {
        let mut arena = ValueArena::new();
        // Disjunction containing itself as a branch
        let d_id = arena.alloc(Value::Disjunction { branches: vec![] });
        if let Some(Value::Disjunction { branches }) = arena.get_mut(d_id) {
            branches.push(DisjunctionBranch {
                default: false,
                val: d_id,
            });
        }
        let forty_two = arena.int(42);
        let res_id = unify(&mut arena, d_id, forty_two);
        let res_val = arena.get(res_id);
        assert!(
            matches!(res_val, Some(Value::Bottom(b)) if b.message.contains("cyclic disjunction unification detected") || b.message.contains("no matching disjunction branch")),
            "expected graceful failure on cyclic disjunction, got: {:?}",
            res_val
        );
    }

    #[test]
    fn test_recursive_disjunction_cycle_fails_gracefully() {
        let cue_src = r#"
            #CycleDisj: int | #CycleDisj
            val: #CycleDisj & "not_an_int"
        "#;
        let res = eval_to_json(cue_src);
        assert!(res.is_err());
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
    fn test_vendored_package_import_with_qualifier() {
        let temp_dir =
            std::env::temp_dir().join(format!("cue_test_vendored_{}", std::process::id()));
        let mod_dir = temp_dir.join("cue.mod");
        let vendored_pkg_dir = mod_dir.join("pkg").join("github.com/tonky/enve/schema/v1");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::create_dir_all(&vendored_pkg_dir).unwrap();

        let mod_cue = r#"
            module: "example.com/myapp"
            language: version: "v0.12.0"
        "#;
        std::fs::write(mod_dir.join("module.cue"), mod_cue).unwrap();

        let schema_cue = r#"
            package devshell

            #ShellSpec: {
                name: string
                packages: [...string]
            }
        "#;
        std::fs::write(vendored_pkg_dir.join("devshell.cue"), schema_cue).unwrap();

        let app_cue = r#"
            package main
            import "github.com/tonky/enve/schema/v1:devshell"

            env: devshell.#ShellSpec & {
                name: "rust-dev"
                packages: ["cargo", "rustc"]
            }
        "#;
        let app_file = temp_dir.join("enve.cue");
        std::fs::write(&app_file, app_cue).unwrap();

        let loaded = crate::package::PackageLoader::load_file(&app_file);
        assert!(loaded.is_ok());
        let (eval, root_id) = loaded.unwrap();
        let json_val = eval.to_json(root_id).unwrap();
        assert_eq!(json_val["env"]["name"], "rust-dev");
        assert_eq!(json_val["env"]["packages"][0], "cargo");
        assert_eq!(json_val["env"]["packages"][1], "rustc");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_cross_file_definitions_and_hierarchical_module() {
        let temp_dir = std::env::temp_dir().join(format!(
            "cue_hierarchical_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mod_dir = temp_dir.join("cue.mod");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("module.cue"),
            "module: \"example.com/platform@v0\"\nlanguage: { version: \"v0.16.1\" }\n",
        )
        .unwrap();

        let schema_dir = temp_dir.join("schema");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(
            schema_dir.join("a.cue"),
            "package schema\n#Resource: { cpu: int }\n",
        )
        .unwrap();
        std::fs::write(
            schema_dir.join("b.cue"),
            "package schema\n#Service: { name: string, res: #Resource }\n",
        )
        .unwrap();

        let sub_dir = temp_dir.join("subprojects").join("app");
        let sub_mod = sub_dir.join("cue.mod");
        std::fs::create_dir_all(&sub_mod).unwrap();
        std::fs::write(
            sub_mod.join("module.cue"),
            "module: \"example.com/platform@v0\"\nlanguage: { version: \"v0.16.1\" }\n",
        )
        .unwrap();

        let app_cue = "package app\nimport \"example.com/platform/schema\"\nservice: schema.#Service & { name: \"web\", res: { cpu: 4 } }\n";
        let app_file = sub_dir.join("app.cue");
        std::fs::write(&app_file, app_cue).unwrap();

        let loaded = crate::package::PackageLoader::load_file(&app_file);
        assert!(loaded.is_ok(), "Failed to load: {:?}", loaded.err());
        let (eval, root_id) = loaded.unwrap();
        let json_val = eval.to_json(root_id).unwrap();
        assert_eq!(json_val["service"]["name"], "web");
        assert_eq!(json_val["service"]["res"]["cpu"], 4);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_disjunction_branch_and_path_error_diagnostics() {
        let cue_src = r#"
            #Dep: string | { name: string, port: int }
            app: {
                dep: #Dep & { name: "db", port: "not-an-int" }
            }
        "#;
        let err = eval_to_json(cue_src).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("app.dep"),
            "Expected path 'app.dep' in error: {err_str}"
        );
        assert!(
            err_str.contains("no matching disjunction branch"),
            "Expected disjunction failure in error: {err_str}"
        );
    }
}
