//! Regressions from the standalone Odoo pipeline snapshot. Invalid self-
//! references must settle as errors; ambiguous defaults must remain ambiguous.

use cue_eval::{Evaluator, eval_to_json};
use serde_json::json;

#[test]
fn a_field_shadows_the_loop_variable_in_direct_and_nested_bodies() {
    for source in [
        r#"out: {for name, v in {a: 1} {name: "\(name)", n: v}}"#,
        r#"out: {for name, v in {a: 1} {(name): {name: "\(name)", n: v}}}"#,
    ] {
        let error = eval_to_json(source).unwrap_err().to_string();
        assert!(error.contains("cycle with field: name"), "{error}");
    }
    assert_eq!(
        eval_to_json(r#"out: {for key, v in {a: 1} {(key): {name: "\(key)", n: v}}}"#).unwrap(),
        json!({"out": {"a": {"name": "a", "n": 1}}}),
    );
}

#[test]
fn a_nested_forward_reference_shadows_an_outer_binding() {
    assert_eq!(
        eval_to_json(
            r#"name: "outer"
x: {nested: {got: name}, name: "inner"}"#
        )
        .unwrap(),
        json!({"name": "outer", "x": {"nested": {"got": "inner"}, "name": "inner"}}),
    );
    assert_eq!(
        eval_to_json(
            r#"out: {for name, v in {a: 1} {(name): {nested: {got: name}, name: "inner"}}}"#
        )
        .unwrap(),
        json!({"out": {"a": {"nested": {"got": "inner"}, "name": "inner"}}}),
    );
}

#[test]
fn a_bare_self_reference_can_be_constrained_without_accepting_expression_cycles() {
    for expression in ["x & 1", "1 & x", "(x & int) & 1", "x & (x & 1)"] {
        assert_eq!(
            eval_to_json(&format!("x: {expression}")).unwrap(),
            json!({"x": 1})
        );
    }
    for expression in [r#""\(x)""#, "(x + 1) & 1", "x"] {
        assert!(
            eval_to_json(&format!("x: {expression}")).is_err(),
            "{expression}"
        );
    }
}

#[test]
fn a_merged_interpolation_cycle_settles_without_growing_its_error() {
    for count in [1, 100] {
        let mut source = String::from("#C: {name?: string, title: string}\nitems: {\n");
        for i in 0..count {
            source.push_str(&format!("a{i}: {i}\n"));
        }
        source.push_str(
            r#"}
out: {for name, v in items {(name): #C & {name: "\(name)", title: "Title \(name)"}}}
"#,
        );
        let mut evaluator = Evaluator::new();
        let root = evaluator
            .eval_file(&cue_syntax::parse_file(&source).unwrap())
            .unwrap();
        let error = evaluator.to_json(root).unwrap_err();
        assert!(error.contains("out.a0.name"), "{error}");
        assert!(error.contains("cycle"), "{error}");
        assert!(
            error.len() < 256,
            "diagnostic grew to {} bytes",
            error.len()
        );
        assert_eq!(evaluator.unsettled, 0);
        assert!(
            evaluator.derivations < count * 16,
            "{count} components took {} derivations",
            evaluator.derivations
        );
    }
}

#[test]
fn ambiguous_disjunctions_do_not_export_or_interpolate_the_first_branch() {
    for expression in [
        r#"(string | *"a") & (string | *"b")"#,
        r#"(*"a" | "b") & ("a" | *"b")"#,
        r#"*"a" | *"b""#,
        r#""a" | "b""#,
    ] {
        let error = eval_to_json(&format!("x: {expression}")).unwrap_err();
        assert!(error.to_string().contains("'x'"), "{expression}: {error}");
        let source = format!("_choice: {expression}\nx: \"\\(_choice)\"");
        assert!(eval_to_json(&source).is_err(), "{source}");
    }
}

#[test]
fn defaults_that_have_one_winner_still_export() {
    for expression in [r#""a" | "a""#, r#"*"a" | *"a""#] {
        assert_eq!(
            eval_to_json(&format!("x: {expression}")).unwrap(),
            json!({"x": "a"})
        );
        assert_eq!(
            eval_to_json(&format!("_choice: {expression}\nx: \"\\(_choice)\"")).unwrap(),
            json!({"x": "a"}),
        );
    }
    for (left, right, expected) in [
        (r#"string | *"a""#, r#"string | *"a""#, "a"),
        (r#"string | *"a""#, r#""a" | "b""#, "a"),
        (r#"string | *"a""#, r#""b""#, "b"),
    ] {
        for expression in [
            format!("({left}) & ({right})"),
            format!("({right}) & ({left})"),
        ] {
            assert_eq!(
                eval_to_json(&format!("x: {expression}")).unwrap(),
                json!({"x": expected})
            );
        }
    }
}
