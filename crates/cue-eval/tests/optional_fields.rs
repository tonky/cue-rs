//! An optional field is a constraint on a field that may appear, not a field.
//!
//! `cue export` v0.16.1 emits none of them, whatever they hold - `a?: 1` exports `{}`.
//! cue-rs used to ask instead whether the constraint *looked* concrete, and a struct
//! whose own fields are all optional looks like `{}` while an open list looks like `[]`,
//! so a schema's shape leaked into every export made against it: `enact`'s own example
//! exported `ci: {}`, `jobs: {}` and `workers: {}` for fields nothing had declared. The
//! scalar cases were correctly dropped, which is why the bug read as arbitrary.
//!
//! Every expectation below is upstream v0.16.1's output, recorded on 2026-09-22.

use cue_eval::eval_to_json;
use serde_json::json;

/// The four shapes side by side. Before this, only `e` was dropped.
#[test]
fn an_optional_field_is_dropped_whatever_shape_its_constraint_has() {
    assert_eq!(
        eval_to_json(
            r#"
            #S: {a?: string, b?: {x?: int}, c?: [...string], d?: [...#S], e?: int}
            v: #S & {a: "hi"}
            "#
        )
        .unwrap(),
        json!({"v": {"a": "hi"}})
    );
}

/// Not only in a definition, and not only for a non-concrete constraint. A
/// concrete optional field is the case the old shape test got most obviously
/// wrong, and the one upstream documents.
#[test]
fn a_concrete_optional_field_is_dropped_too() {
    assert_eq!(
        eval_to_json("v: {p?: 1, q?: {r: 2}, s?: [3], t: 4}").unwrap(),
        json!({"v": {"t": 4}})
    );
}

/// What makes an optional field appear is a conjunct that specifies it, and then
/// it exports like any other field - including the empty struct and empty list
/// that used to appear on their own.
#[test]
fn a_conjunct_that_specifies_one_brings_it_back() {
    assert_eq!(
        eval_to_json(
            r#"
            #S: {a?: string, b?: {x?: int}, c?: [...string]}
            v: #S & {a: "hi", b: {}, c: []}
            "#
        )
        .unwrap(),
        json!({"v": {"a": "hi", "b": {}, "c": []}})
    );
}

/// Optionality is not inherited: a required field holding a struct exports it,
/// and the optional fields *inside* that struct are dropped by the same rule.
#[test]
fn a_required_field_exports_its_struct_without_that_struct_s_optional_fields() {
    assert_eq!(
        eval_to_json("v: {keep: {here: 1, gone?: 2}, also?: {x: 3}}").unwrap(),
        json!({"v": {"keep": {"here": 1}}})
    );
}

/// A nested schema, which is the shape a user meets: a pipeline declaring three
/// optional sections and filling one.
#[test]
fn only_the_sections_a_config_fills_are_exported() {
    assert_eq!(
        eval_to_json(
            r#"
            #Pipeline: {
                name:     string
                ci?:      {on?: [...string]}
                jobs?:    [string]: {run: string}
                workers?: [...{id: int}]
            }
            p: #Pipeline & {name: "build", jobs: {compile: {run: "make"}}}
            "#
        )
        .unwrap(),
        json!({"p": {"name": "build", "jobs": {"compile": {"run": "make"}}}})
    );
}
