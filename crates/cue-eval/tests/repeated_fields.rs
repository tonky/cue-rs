use cue_eval::eval_to_json;
use serde_json::json;

fn both_orders(first: &str, second: &str) -> [String; 2] {
    [format!("{first}\n{second}"), format!("{second}\n{first}")]
}

#[test]
fn repeated_fields_preserve_service_policy_and_command() {
    for source in both_orders(
        r#"services: worker: {environmentPolicy: {mode: "restricted"}}"#,
        r#"services: worker: {command: "sleep 60"}"#,
    ) {
        assert_eq!(
            eval_to_json(&source).unwrap(),
            json!({"services": {"worker": {
                "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
            }}})
        );
    }
}

#[test]
fn repeated_fields_preserve_distinct_services() {
    for source in both_orders(
        r#"services: worker: {environmentPolicy: {mode: "restricted"}}"#,
        r#"services: api: {command: "serve"}"#,
    ) {
        assert_eq!(
            eval_to_json(&source).unwrap(),
            json!({"services": {
                "worker": {"environmentPolicy": {"mode": "restricted"}},
                "api": {"command": "serve"}
            }})
        );
    }
}

#[test]
fn repeated_fields_reject_policy_conflicts_permanently() {
    for source in both_orders(
        r#"services: worker: environmentPolicy: mode: "restricted""#,
        r#"services: worker: environmentPolicy: mode: "inherit""#,
    ) {
        for source in [
            source.clone(),
            format!("{source}\nservices: worker: command: \"sleep 60\""),
        ] {
            let error = eval_to_json(&source).unwrap_err().to_string();
            assert!(error.contains("conflict"), "{error}");
        }
    }
}

#[test]
fn repeated_fields_preserve_scalar_constraints_and_optionality() {
    for source in both_orders("port: int & >0", "port: 8080") {
        assert_eq!(eval_to_json(&source).unwrap(), json!({"port": 8080}));
    }
    for source in both_orders("port: int & >0", "port: -1") {
        assert!(eval_to_json(&source).is_err(), "{source}");
    }
    for source in both_orders("port?: int", "port: 8080") {
        assert_eq!(eval_to_json(&source).unwrap(), json!({"port": 8080}));
    }
}

#[test]
fn repeated_fields_unify_all_label_kinds_and_bindings() {
    for label in ["value", "\"value\"", "(\"value\")", "#Value", "_value"] {
        for source in both_orders(
            &format!("{label}: {{policy: \"restricted\"}}"),
            &format!("{label}: {{command: \"sleep 60\"}}"),
        ) {
            let binding = match label {
                "#Value" | "_value" => label,
                _ => "value",
            };
            let result = eval_to_json(&format!("{source}\noutput: {binding}")).unwrap();
            assert_eq!(
                result["output"],
                json!({"policy": "restricted", "command": "sleep 60"})
            );
        }
    }
}

#[test]
fn repeated_fields_retry_forward_references_without_losing_constraints() {
    for source in both_orders(
        "services: worker: environmentPolicy: mode: policy",
        r#"services: worker: command: "sleep 60""#,
    ) {
        let source = format!("{source}\npolicy: \"restricted\"");
        let result = eval_to_json(&source).unwrap();
        assert_eq!(
            result["services"]["worker"],
            json!({
                "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
            })
        );
    }
    assert!(eval_to_json("value: missing\nvalue: 1").is_err());
}

#[test]
fn repeated_fields_references_see_all_declarations() {
    for source in both_orders(
        r#"service: {environmentPolicy: {mode: "restricted"}}"#,
        r#"service: {command: "sleep 60"}"#,
    ) {
        let mut declarations = source.lines();
        let first = declarations.next().unwrap();
        let second = declarations.next().unwrap();
        for source in [
            format!("{first}\nservices: worker: service\n{second}"),
            format!("services: worker: service\n{source}"),
        ] {
            let result = eval_to_json(&source).unwrap();
            assert_eq!(
                result["services"]["worker"],
                json!({
                    "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
                })
            );
        }
    }
    for source in both_orders("value: {x: y}", "value: {y: 1}") {
        assert!(eval_to_json(&source).is_err(), "{source}");
    }
}

#[test]
fn repeated_fields_dynamic_labels_retry_in_enclosing_scope() {
    let result = eval_to_json(
        r#"
        services: { (name): {environmentPolicy: {mode: "restricted"}} }
        services: { worker: {command: "sleep 60"} }
        name: "worker"
    "#,
    )
    .unwrap();
    assert_eq!(
        result["services"]["worker"],
        json!({
            "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
        })
    );
    assert!(eval_to_json("services: {(missing): {command: \"sleep 60\"}}").is_err());
}

#[test]
fn repeated_fields_dynamic_references_see_all_declarations() {
    for label in [r#"("service")"#, "(name)", r#"(name + "vice")"#] {
        let name = if label.contains('+') {
            "ser"
        } else {
            "service"
        };
        let source = format!(
            r#"
            service: {{command: "sleep 60"}}
            services: worker: service
            {label}: {{environmentPolicy: {{mode: "restricted"}}}}
            name: "{name}"
        "#
        );
        let result = eval_to_json(&source).unwrap();
        assert_eq!(
            result["services"]["worker"],
            json!({
                "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
            })
        );
    }
}

#[test]
fn repeated_fields_preserve_struct_constraints_and_optional_conflicts() {
    for source in both_orders("a: {}", "a: 1") {
        assert!(eval_to_json(&source).is_err(), "{source}");
    }
    assert!(eval_to_json("a: {x: y}\na: _\na: {y: 1}").is_err());
    assert_eq!(
        eval_to_json("a: {x?: 1}\na: {x?: 2}").unwrap(),
        json!({"a": {}})
    );
    assert!(eval_to_json("a: {x?: 1}\na: {x?: 2}\na: {x: 1}").is_err());
}

#[test]
fn repeated_fields_preserve_each_declarations_lexical_scope() {
    for source in both_orders(
        r#"service: {policy: "inherit"}"#,
        "service: {environmentPolicy: {mode: policy}}",
    ) {
        let result = eval_to_json(&format!("policy: \"restricted\"\n{source}")).unwrap();
        assert_eq!(result["service"]["environmentPolicy"]["mode"], "restricted");
    }
}

#[test]
fn generated_repeated_fields_cannot_export_stale_service_references() {
    let error = eval_to_json(
        r#"
        service: {command: "sleep 60"}
        services: worker: service
        for x in [1] {
            {service: {environmentPolicy: {mode: "restricted"}}}
        }
    "#,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("embedding modifies previously referenced field"),
        "{error}"
    );
    for label in ["service", "#Service", "_service"] {
        let source = format!(
            r#"
            {label}: {{command: "sleep 60"}}
            services: worker: {label}
            for x in [1] {{
                {label}: {{environmentPolicy: {{mode: "restricted"}}}}
            }}
        "#
        );
        let error = eval_to_json(&source).unwrap_err().to_string();
        assert!(
            error.contains("comprehension modifies previously referenced field"),
            "{error}"
        );
    }
    let result = eval_to_json(
        r#"
        service: {command: "sleep 60"}
        for x in [1] {
            service: {environmentPolicy: {mode: "restricted"}}
        }
        services: worker: service
    "#,
    )
    .unwrap();
    assert_eq!(
        result["service"],
        json!({
            "environmentPolicy": {"mode": "restricted"}, "command": "sleep 60"
        })
    );
    assert_eq!(result["services"]["worker"], result["service"]);
}
