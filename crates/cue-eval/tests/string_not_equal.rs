use cue_eval::{Evaluator, PackageLoader, TypeKind, Value, eval_to_json, unify};
use std::path::Path;

#[test]
fn string_not_equal_candidates() {
    for (expression, expected) in [
        (r#"string & !="" & "team""#, "team"),
        (r#""team" & (string & !="")"#, "team"),
        (r#"string & !="admin" & "user""#, "user"),
        (r#"string & !="" & !="admin" & "user""#, "user"),
        (r#"string & !="" & =~"^team-" & "team-api""#, "team-api"),
        (r#"string & !="" & !~"^admin" & "team-api""#, "team-api"),
        (r#"string & !="a.*" & "abc""#, "abc"),
        (r#"string & !="Admin" & "admin""#, "admin"),
    ] {
        let json = eval_to_json(&format!("value: {expression}")).unwrap();
        assert_eq!(json["value"], expected, "{expression}");
    }
}

#[test]
fn string_not_equal_violations() {
    for expression in [
        r#"string & !="" & """#,
        r#""" & (string & !="")"#,
        r#"string & !="admin" & "admin""#,
        r#"string & !="" & !="admin" & """#,
        r#"string & !="" & !="admin" & "admin""#,
        r#"string & !="a.*" & "a.*""#,
    ] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not satisfy bound !="),
            "{expression}: {error}"
        );
    }
    for (expression, diagnostic) in [
        (
            r#"string & !="" & =~"^team-" & "user""#,
            "does not match regex",
        ),
        (
            r#"string & !="" & !~"^team-" & "team-api""#,
            "matches regex",
        ),
        (r#"string & !="" & =~"[" & "team""#, "invalid regex"),
        (r#"string & !="" & !~"[" & "team""#, "invalid regex"),
    ] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(diagnostic), "{expression}: {error}");
    }
}

#[test]
fn string_not_equal_remains_incomplete_until_unified() {
    let file = cue_syntax::parse_file(r#"name: string & !="" & !="admin""#).unwrap();
    let mut evaluator = Evaluator::new();
    let root = evaluator.eval_file(&file).unwrap();
    let Some(Value::Struct(fields)) = evaluator.arena.get(root) else {
        panic!("expected struct");
    };
    let bounds = fields.fields["name"].val;
    assert!(matches!(
        evaluator.arena.get(bounds),
        Some(Value::Bounds { base_type: Some(TypeKind::String), constraints })
            if constraints.len() == 2
    ));
    assert!(
        evaluator
            .to_json(root)
            .unwrap_err()
            .contains("cannot export bound constraint")
    );
    let candidate = evaluator.arena.string("team");
    let result = unify(&mut evaluator.arena, bounds, candidate);
    assert_eq!(result, candidate);
    let excluded = evaluator.arena.string("admin");
    let result = unify(&mut evaluator.arena, bounds, excluded);
    assert!(matches!(
        evaluator.arena.get(result),
        Some(Value::Bottom(_))
    ));
}

#[test]
fn string_not_equal_package_exports() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/string_not_equal");
    for (file, diagnostic) in [
        ("positive.cue", None),
        ("negative_local.cue", Some("does not satisfy bound !=")),
        ("negative_import.cue", Some("does not satisfy bound !=")),
        ("incomplete.cue", Some("cannot export bound constraint")),
    ] {
        let (evaluator, root) = PackageLoader::load_file(fixtures.join(file)).unwrap();
        let result = evaluator.to_json(root);
        if let Some(diagnostic) = diagnostic {
            let error = result.unwrap_err();
            assert!(error.contains(diagnostic), "{file}: {error}");
        } else {
            assert_eq!(
                result.unwrap(),
                serde_json::json!({
                    "policy": {"name": "team-policy"},
                    "imported": {"name": "team-api"}
                })
            );
        }
    }
}
