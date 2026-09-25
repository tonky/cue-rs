use cue_eval::{BottomKind, Evaluator, PackageLoader, TypeKind, Value, eval_to_json};
use serde_json::json;

#[test]
fn roots_and_nested_blocks_keep_their_value_kind() {
    for (source, expected) in [
        ("3", json!(3)),
        ("null", json!(null)),
        ("true", json!(true)),
        (r#""hello""#, json!("hello")),
        ("'hi'", json!("aGk=")),
        ("[1, 2]", json!([1, 2])),
        ("*1 | int", json!(1)),
        ("let value = 3\nvalue", json!(3)),
        ("value\nlet value = 4", json!(4)),
    ] {
        assert_eq!(eval_to_json(source).unwrap(), expected, "{source}");
        let nested = format!("value: {{{source}}}");
        assert_eq!(
            eval_to_json(&nested).unwrap()["value"],
            expected,
            "{nested}"
        );
    }
}

#[test]
fn empty_struct_top_and_abstract_embeddings_stay_distinct() {
    for (source, kind) in [
        ("{}", "struct"),
        ("{_}", "top"),
        ("{int}", "int"),
        ("{!=null}", "bounds"),
    ] {
        let mut evaluator = Evaluator::new();
        let id = evaluator
            .eval_file(&cue_syntax::parse_file(source).unwrap())
            .unwrap();
        match (kind, evaluator.arena.get(id).unwrap()) {
            ("struct", Value::Struct(_))
            | ("top", Value::Top)
            | ("int", Value::Type(TypeKind::Int))
            | ("bounds", Value::Bounds { .. }) => {}
            (_, value) => panic!("{source}: {value:?}"),
        }
    }
    assert_eq!(eval_to_json("{_} & 3").unwrap(), 3);
    assert!(eval_to_json("{} & 3").is_err());
}

#[test]
fn embedding_conflicts_are_local_and_keep_their_error_category() {
    let source = include_str!("../../../tests/conformance/regressions/embeddings/errors.txtar")
        .strip_prefix("-- in.cue --\n")
        .unwrap();
    let mut evaluator = Evaluator::new();
    let id = evaluator
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let Value::Struct(value) = evaluator.arena.get(id).unwrap() else {
        panic!()
    };
    for name in ["bad", "object", "reverse", "empty", "null"] {
        assert!(
            matches!(evaluator.arena.get(value.fields[name].val), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Conflict),
            "{name}"
        );
    }
    assert_eq!(evaluator.to_json(value.fields["good"].val).unwrap(), 3);
}

#[test]
fn multiple_embeddings_and_non_null_bounds_match_concrete_values() {
    for source in ["{int\n4}", "{4\nint}", "{{int}\n{4}}"] {
        assert_eq!(eval_to_json(source).unwrap(), 4, "{source}");
    }
    let source = include_str!("../../../tests/conformance/regressions/embeddings/bounds.txtar")
        .strip_prefix("-- in.cue --\n")
        .unwrap();
    assert_eq!(
        eval_to_json(source).unwrap(),
        json!({
            "object":{"x":1}, "reverse":{"x":1}, "list":[1,2],
            "number":2, "boolean":true, "text":"text", "bytes":"aGk="
        })
    );
    for source in [
        "{!=null\nnull}",
        "{int & !=null\ntrue}",
        "{!=null & >0\n-1}",
    ] {
        assert!(eval_to_json(source).is_err(), "{source}");
    }
}

#[test]
fn package_loaders_accept_scalar_roots_and_meet_all_files() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a.cue");
    std::fs::write(&first, "package example\nlet value = 4\nvalue").unwrap();
    let (evaluator, id) = PackageLoader::load_file(&first).unwrap();
    assert_eq!(evaluator.to_json(id).unwrap(), 4);
    std::fs::write(dir.path().join("b.cue"), "package example\nint").unwrap();
    let (evaluator, id) = PackageLoader::load_dir(dir.path()).unwrap();
    assert_eq!(evaluator.to_json(id).unwrap(), 4);
    std::fs::write(dir.path().join("c.cue"), "package example\n5").unwrap();
    let (evaluator, id) = PackageLoader::load_dir(dir.path()).unwrap();
    assert!(
        matches!(evaluator.arena.get(id), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Conflict)
    );
}

#[test]
fn comprehensions_keep_constraints_and_retry_forward_inputs() {
    let source =
        include_str!("../../../tests/conformance/regressions/embeddings/comprehensions.txtar")
            .strip_prefix("-- in.cue --\n")
            .unwrap();
    assert_eq!(
        eval_to_json(source).unwrap(),
        json!({
            "empty": {}, "object": {"y": 1, "a": 2}, "scalar": 3,
            "falseBranch": {}, "list": [1,2], "forwardLet": 7
        })
    );
}

#[test]
fn nested_embedding_waits_for_an_outer_definition() {
    let value = eval_to_json("a: {#Base, x: 1}\n#Base: {y: 2}").unwrap();
    assert_eq!(value, json!({"a": {"x":1, "y":2}}));
}
