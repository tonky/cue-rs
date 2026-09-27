//! The `error("message")` builtin: a custom error branch for disjunctions.
//!
//! It loses to any succeeding sibling, lends its message to a lone surviving
//! failure while adopting that failure's code, and settles definite on its
//! own. Expectations mirror upstream `cue export` on the
//! `builtins_error.txtar` shapes.

use cue_eval::{BottomKind, Evaluator, Value, eval_to_json};
use serde_json::json;

fn bottom_kind_of(source: &str, field: &str) -> BottomKind {
    let mut eval = Evaluator::new();
    let root = eval
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let Value::Struct(root) = eval.arena.get(root).unwrap() else {
        panic!("root is not a struct")
    };
    match eval.arena.get(root.fields[field].val) {
        Some(Value::Bottom(reason)) => reason.kind,
        other => panic!("{field} is not bottom: {other:?}"),
    }
}

#[test]
fn error_branch_loses_to_a_succeeding_sibling() {
    assert_eq!(
        eval_to_json(r#"a: 1 | error("drop me")"#).unwrap(),
        json!({"a": 1})
    );
}

#[test]
fn error_branch_names_a_conflicting_disjunction() {
    let error = eval_to_json(r#"b: 1 & 2 | error("use me")"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("use me"), "{error}");
    assert_eq!(
        bottom_kind_of(r#"b: 1 & 2 | error("use me")"#, "b"),
        BottomKind::Conflict
    );
}

#[test]
fn error_branch_adopts_an_incomplete_sibling_code() {
    let source = "#S: {x: int}\ny: #S.x + 1 | error(\"user msg\")";
    let error = eval_to_json(source).unwrap_err().to_string();
    assert!(error.contains("user msg"), "{error}");
    assert_eq!(bottom_kind_of(source, "y"), BottomKind::Incomplete);
}

#[test]
fn lone_error_is_definite() {
    let error = eval_to_json(r#"a: error("boom")"#).unwrap_err().to_string();
    assert!(error.contains("boom"), "{error}");
    assert_eq!(
        bottom_kind_of(r#"a: error("boom")"#, "a"),
        BottomKind::Custom
    );
}

/// All branches pending beside a custom one: the disjunction reports the
/// custom message with the most-decided pending code instead of waiting
/// forever. Mirrors the `substituteFail` shape.
#[test]
fn all_pending_branches_beside_custom_collapse_to_incomplete() {
    let source = "a: {b: int}\ns: a.b + 1 | a.notExist | error(\"reference failed\")";
    let mut eval = Evaluator::new();
    let root = eval
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let Value::Struct(root) = eval.arena.get(root).unwrap() else {
        panic!("root is not a struct")
    };
    match eval.arena.get(root.fields["s"].val) {
        Some(Value::Bottom(reason)) => {
            assert_eq!(reason.kind, BottomKind::Incomplete);
            assert!(reason.message.contains("reference failed"), "{reason:?}");
        }
        other => panic!("s is not bottom: {other:?}"),
    }
}
