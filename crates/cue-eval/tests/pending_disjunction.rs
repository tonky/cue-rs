//! A disjunction whose branches only failed on a reference that is not resolved yet
//! is still pending, not an error.
//!
//! `unify_disjunction_inner` turned "no branch survived" into a conflict, whatever the
//! branches failed on. A branch that met `[a.b]` before `a` was evaluated failed with
//! an unresolved reference, the conflict hid that, and the relaxation loop never came
//! back to it. So `(int | [...]) & [a.b]` failed whenever `a` came later: further down
//! the file, or in another file of the package. enact's `services?: {...} | [...]`
//! met exactly this with a component listing services declared in another file.
//!
//! Every expectation is upstream `cue export` v0.16.1's, recorded on 2026-09-25.

use cue_eval::{PackageLoader, eval_to_json};
use serde_json::json;

#[test]
fn a_forward_reference_inside_a_disjunction_resolves() {
    for (case, source, expected) in [
        (
            "list branch",
            "c: (int | [..._]) & [top.pg]\ntop: pg: port: 1",
            json!([{"port": 1}]),
        ),
        (
            "struct or list",
            "c: ({[string]: _} | [..._]) & [top.pg]\ntop: pg: port: 1",
            json!([{"port": 1}]),
        ),
        (
            "definition field",
            "#C: {s?: {[string]: _} | [..._]}\nc: #C & {s: [top.pg]}\ntop: pg: port: 1",
            json!({"s": [{"port": 1}]}),
        ),
        (
            "disjunction on both sides",
            "c: (int | [..._]) & (string | [top.pg])\ntop: pg: port: 1",
            json!([{"port": 1}]),
        ),
    ] {
        let exported = eval_to_json(source).unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(exported["c"], expected, "{case}");
    }
}

#[test]
fn a_disjunction_no_branch_can_satisfy_still_fails() {
    for (case, source) in [
        ("resolved", "top: pg: port: 1\nc: (int | string) & [top.pg]"),
        ("forward", "c: (int | string) & [top.pg]\ntop: pg: port: 1"),
        ("never resolves", "c: (int | [..._]) & [missing.pg]"),
    ] {
        let result = eval_to_json(source);
        assert!(result.is_err(), "{case}: exported {result:?}");
    }
}

/// The shape that surfaced it: the list's element lives in another file of the package.
#[test]
fn a_reference_to_another_file_inside_a_disjunction_resolves() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/m@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("c.cue"),
        "package a\n\nc: {services?: {[string]: _} | [..._]} & {services: [top.pg]}\n",
    )
    .unwrap();
    std::fs::write(root.join("s.cue"), "package a\n\ntop: pg: port: 1\n").unwrap();

    let (evaluator, value) = PackageLoader::load_dir(root).unwrap();
    let exported = evaluator.to_json(value).unwrap();
    assert_eq!(exported["c"], json!({"services": [{"port": 1}]}));
}
