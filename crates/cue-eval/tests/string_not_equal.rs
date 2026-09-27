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

/// Bound targets constrain the base kind, including `!=`: upstream rejects
/// `!="a" & <5` (string meets number) and `"foo" & !=5` / `"foo" & >5`
/// (string meets a numeric target) instead of keeping an unsatisfiable
/// constraint or silently accepting.
#[test]
fn bound_targets_constrain_base_kind() {
    for (expression, diagnostic) in [
        (r#"!="a" & <5"#, "conflicting bound base types"),
        (
            r#""foo" & !=5"#,
            "type mismatch: expected string, found int",
        ),
        (r#""foo" & >5"#, "type mismatch: expected string, found int"),
        (r#"string & !="a" & <5"#, "conflicting bound base types"),
    ] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(diagnostic), "{expression}: {error}");
    }
    for (expression, expected) in [
        (r#"!="a" & "b""#, serde_json::json!("b")),
        (r#">5 & <10 & 7"#, serde_json::json!(7)),
        (r#"!="a" & !="b" & "c""#, serde_json::json!("c")),
    ] {
        let json = eval_to_json(&format!("value: {expression}")).unwrap();
        assert_eq!(json["value"], expected, "{expression}");
    }
}

/// A bound over a value that can never satisfy it is decided at
/// construction: bool is not an ordered type, and a bound, basic type, or
/// top is never a concrete value to bound (upstream: `incomplete`).
/// Retryable targets and `!=null` stay lazy.
#[test]
fn non_concrete_bound_targets_decided_at_construction() {
    for (expression, diagnostic) in [
        (r#"<(<3)"#, "non-concrete value <3 for bound <"),
        (r#"!=(!=3)"#, "non-concrete value !=3 for bound !="),
        (r#"<(int)"#, "non-concrete value int for bound <"),
        (r#">true"#, "cannot use bool for bound >"),
    ] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(diagnostic), "{expression}: {error}");
    }
    // `!=null` still filters downstream instead of deciding eagerly.
    let json = eval_to_json(r#"value: !=null & <5 & 3"#).unwrap();
    assert_eq!(json["value"], 3);
}

/// Merged ranges must overlap: a lower bound past the upper bound (or
/// touching it exclusively) is eval-bottom, over numbers and strings.
/// Compatible ranges stay constraints.
#[test]
fn incompatible_byte_ranges_conflict() {
    for expression in ["<'a' & >'b'", ">'b' & <='a'", ">='b' & <'a'"] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("incompatible bytes bounds"),
            "{expression}: {error}"
        );
    }
}

#[test]
fn incompatible_ranges_conflict() {
    for (expression, diagnostic) in [
        (r#"<1 & >2"#, "incompatible number bounds <1 and >2"),
        (r#">2 & <=1"#, "incompatible number bounds"),
        (r#">=2 & <2"#, "incompatible number bounds <2 and >=2"),
        (r#"<"a" & >"b""#, "incompatible string bounds"),
    ] {
        let error = eval_to_json(&format!("value: {expression}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(diagnostic), "{expression}: {error}");
    }
    // Compatible ranges stay bounds (JSON export cannot render a bare
    // constraint, but the meet itself must not fail).
    let error = eval_to_json(r#"value: >2 & <=3"#).unwrap_err().to_string();
    assert!(error.contains("cannot export bound constraint"), "{error}");
    let json = eval_to_json(r#"value: "b" & >"a" & <"c""#).unwrap();
    assert_eq!(json["value"], "b");
    // An integer base snaps both sides: no integer fits between `>1`
    // and `<2`, but 2 fits between `>1` and `<3`.
    let error = eval_to_json(r#"value: >1 & <2 & int"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("incompatible integer bounds"), "{error}");
    // Null is not orderable.
    let error = eval_to_json(r#"value: >(null)"#).unwrap_err().to_string();
    assert!(error.contains("cannot use null for bound"), "{error}");
}
