//! `x != _|_` and `x == _|_` ask whether `x` exists; they are not arithmetic on
//! bottom.
//!
//! Both used to be false whatever `x` was: `_|_` went through the comparison like
//! any operand, the comparison came out bottom, and a comprehension's `if` only
//! fires on `true`. So `if c.jobs != _|_ {…}` never fired. A check on a field the
//! evaluator has not reached yet - declared further down, or in another file -
//! must not decide on that either: it stays pending until nothing can retry.
//!
//! Every expectation is upstream `cue export` v0.16.1's, recorded on 2026-09-25.

use cue_eval::eval_to_json;
use serde_json::json;

#[test]
fn a_check_on_a_present_field_holds() {
    assert_eq!(
        eval_to_json("s: a: 1\nhas: s.a != _|_\nnot: s.a == _|_").unwrap(),
        json!({"s": {"a": 1}, "has": true, "not": false})
    );
}

#[test]
fn a_check_on_a_missing_field_does_not() {
    assert_eq!(
        eval_to_json("s: a: 1\nlacks: s.b != _|_\nmissing: s.b == _|_").unwrap(),
        json!({"s": {"a": 1}, "lacks": false, "missing": true})
    );
}

/// A selector reads an unset optional field's constraint, but the field is not
/// there.
#[test]
fn an_unset_optional_field_does_not_exist() {
    assert_eq!(
        eval_to_json(
            r#"
            #C: {a?: string, b?: string}
            v: #C & {a: "x"}
            a: v.a != _|_
            b: v.b != _|_
            "#
        )
        .unwrap(),
        json!({"v": {"a": "x"}, "a": true, "b": false})
    );
}

#[test]
fn a_struct_comprehension_fires_on_a_present_field_only() {
    assert_eq!(
        eval_to_json(
            r#"
            s: a: 1
            present: {if s.a != _|_ {a: s.a}}
            absent: {if s.b != _|_ {b: s.b}}
            "#
        )
        .unwrap(),
        json!({"s": {"a": 1}, "present": {"a": 1}, "absent": {}})
    );
}

#[test]
fn a_list_comprehension_fires_on_a_present_field_only() {
    assert_eq!(
        eval_to_json("s: a: 1\nl: [if s.a != _|_ {s.a}, if s.b != _|_ {s.b}, if s.b == _|_ {0}]")
            .unwrap(),
        json!({"s": {"a": 1}, "l": [1, 0]})
    );
}

/// `later` is not bound when `z` is first evaluated. Deciding then would read it
/// as missing for good.
#[test]
fn a_check_on_a_field_declared_later_waits_for_it() {
    assert_eq!(
        eval_to_json(
            r#"
            z: {if later.v != _|_ {v: later.v}, if later.w == _|_ {w: "none"}}
            zl: [if later.v != _|_ {later.v}, if later.w != _|_ {later.w}]
            later: v: 1
            "#
        )
        .unwrap(),
        json!({"z": {"v": 1, "w": "none"}, "zl": [1], "later": {"v": 1}})
    );
}

/// The same at a file's top level, where the comprehension is the root's own
/// declaration.
#[test]
fn a_top_level_comprehension_waits_too() {
    assert_eq!(
        eval_to_json("if later.v != _|_ {fromLater: true}\nlater: v: 1").unwrap(),
        json!({"fromLater": true, "later": {"v": 1}})
    );
}
