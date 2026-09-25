//! A `for` over a struct and `len` of a struct see only the fields that are set.
//!
//! An optional field nothing set is a constraint, not a field, and upstream skips
//! it in both places. cue-rs iterated it: `for k, v in #Def & {…}` visited every
//! optional field of the schema, with its constraint as the value, so whatever
//! the loop generated grew a key per declared-but-unset field.
//!
//! Every expectation is upstream `cue export` v0.16.1's, recorded on 2026-09-25.

use cue_eval::eval_to_json;
use serde_json::json;

#[test]
fn a_for_skips_the_optional_fields_a_definition_did_not_set() {
    assert_eq!(
        eval_to_json(
            r#"
            #C: {a?: string, b?: string, c: string}
            v: #C & {a: "x", c: "y"}
            keys: [for k, _ in v {k}]
            copy: {for k, x in v {(k): x}}
            fromDef: [for k, _ in #C {k}]
            "#
        )
        .unwrap(),
        json!({
            "v": {"a": "x", "c": "y"},
            "keys": ["a", "c"],
            "copy": {"a": "x", "c": "y"},
            "fromDef": ["c"],
        })
    );
}

#[test]
fn len_counts_the_fields_that_are_set() {
    assert_eq!(
        eval_to_json(
            r#"
            #C: {a?: string, b?: string, c: string}
            n: len(#C & {a: "x", c: "y"})
            literal: len({p: 1, q?: 2, r?: 3})
            "#
        )
        .unwrap(),
        json!({"n": 2, "literal": 1})
    );
}
