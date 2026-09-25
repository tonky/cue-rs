//! A definition is closed: unifying it with a field it does not declare is an error.
//!
//! cue-rs had the flag and the check but nothing set the flag, so `#A & {bogus: 1}`
//! exported happily and every schema built on definitions let typos through. A
//! definition is now closed where it is *read* - recursively, through fields,
//! pattern constraints, disjunctions and list elements - and `...` reopens a struct.
//!
//! Every expectation below is upstream `cue export` v0.16.1's verdict, recorded on
//! 2026-09-25. cue-rs reports the leaf label (`bogus: field not allowed`) where
//! upstream reports the path (`a.bogus: ...`); the tests check the leaf.
//!
//! The package-loader tests at the end cover the two loader changes made alongside:
//! the opt-in origin annotation that replaced the hard-coded `originDir` injection,
//! and import errors that are reported instead of surfacing as a missing reference.

use cue_eval::{LoadOptions, OriginAnnotation, PackageLoader, eval_to_json};
use serde_json::json;
use std::path::Path;

/// Each source is rejected, naming the field that is not allowed.
#[test]
fn a_field_a_closed_struct_does_not_declare_is_rejected() {
    for (case, source, field) in [
        ("basic", "#A: {x: int}\na: #A & {x: 1, bogus: 2}", "bogus"),
        ("close builtin", "a: close({x: 1}) & {y: 2}", "y"),
        ("field ref", "#A: {x: int}\na: #A\na: {x: 1, y: 2}", "y"),
        (
            "comprehension",
            "#A: {x: int}\na: #A\na: {for k in [\"q\"] {(k): 1}}",
            "q",
        ),
        (
            "nested",
            "#A: {b: {c: int}}\na: #A & {b: {c: 1, d: 2}}",
            "d",
        ),
        (
            "definition field",
            "#S: {v: int}\n#A: {s: #S}\na: #A & {s: {v: 1, w: 2}}",
            "w",
        ),
        (
            "definition in definition",
            "#A: {#Sub: {v: int}, s: #Sub}\na: #A & {s: {v: 1, w: 2}}",
            "w",
        ),
        (
            "dynamic label",
            "#A: {(\"x\"): int}\na: #A & {x: 1, y: 2}",
            "y",
        ),
        (
            "list element",
            "#A: {x: int}\nl: [...#A]\nl: [{x: 1, y: 2}]",
            "y",
        ),
        (
            "pattern target",
            "#C: {root: string}\n#P: {components: [string]: #C}\n\
             p: #P & {components: a: {root: \".\", bogus: 1}}",
            "bogus",
        ),
        (
            "pattern mismatch",
            "#A: {[=~\"^x\"]: int}\na: #A & {xa: 1, ya: 2}",
            "ya",
        ),
        (
            "recursive",
            "#N: {v: int, next?: #N}\nn: #N & {v: 1, next: {v: 2, bad: 3}}",
            "bad",
        ),
        (
            "embedded in a definition",
            "#A: {x: int}\n#B: {#A, y: int}\nb: #B & {x: 1, z: 2}",
            "z",
        ),
        (
            "embedded in a literal",
            "#A: {x: int}\nb: {#A, y: 2}\nb: z: 1",
            "z",
        ),
        (
            "embedded via a field",
            "#A: {x?: int}\nr: #A\nb: {r, y: 1}\nb: z: 1",
            "z",
        ),
        (
            "embedded via hidden",
            "#A: {x?: int}\n_s: #A\nb: {_s, y: 1}\nb: z: 1",
            "z",
        ),
        (
            "embedded via hidden in a definition",
            "#A: {x?: int}\n_s: #A\n#B: {_s, y?: 1}\nb: #B & {z: 1}",
            "z",
        ),
        (
            "embedded via a selector",
            "#A: {x?: int}\nh: {#B: #A}\nb: {h.#B, y: 1}\nb: z: 1",
            "z",
        ),
        // Upstream names the field; cue-rs reports it per branch.
        (
            "disjunction",
            "#A: {x: int} | {y: int}\na: #A & {z: 1}",
            "z",
        ),
    ] {
        let error = eval_to_json(source).expect_err(case).to_string();
        assert!(
            error.contains(&format!("{field}: field not allowed")),
            "{case}: {error}"
        );
    }
}

/// Each source exports what upstream exports.
#[test]
fn what_a_closed_struct_allows_is_accepted() {
    for (case, source, expected) in [
        (
            "comprehension",
            "#A: {x: int}\na: #A\na: {for k in [\"x\"] {(k): 1}}",
            json!({"a": {"x": 1}}),
        ),
        (
            "default branch",
            "#A: *{x: 1} | {y: int}\na: #A & {x: 1}",
            json!({"a": {"x": 1}}),
        ),
        (
            "disjunction",
            "#A: {x: int} | {y: int}\na: #A & {y: 1}",
            json!({"a": {"y": 1}}),
        ),
        (
            "ellipsis",
            "#A: {x: int, ...}\na: #A & {x: 1, y: 2}",
            json!({"a": {"x": 1, "y": 2}}),
        ),
        (
            "nested ellipsis",
            "#A: {b: {c: int, ...}}\na: #A & {b: {c: 1, d: 2}}",
            json!({"a": {"b": {"c": 1, "d": 2}}}),
        ),
        (
            "open struct",
            "#A: {x: {...}}\na: #A & {x: {anything: 1}}",
            json!({"a": {"x": {"anything": 1}}}),
        ),
        (
            "top",
            "#A: {x: _}\na: #A & {x: {deep: 1}}",
            json!({"a": {"x": {"deep": 1}}}),
        ),
        (
            "embedding adds fields",
            "#A: {x: int}\n#B: {#A, y: int}\nb: #B & {x: 1, y: 2}",
            json!({"b": {"x": 1, "y": 2}}),
        ),
        (
            "embedding last",
            "#A: {x: int}\n#B: {y: int, #A}\nb: #B & {x: 1, y: 2}",
            json!({"b": {"x": 1, "y": 2}}),
        ),
        (
            "literal's own fields",
            "#A: {x: int}\nb: {#A, y: 2}\nb: x: 1",
            json!({"b": {"x": 1, "y": 2}}),
        ),
        (
            "hidden and definitions are exempt",
            "#A: {x: int}\na: #A & {x: 1, _h: 2, #d: 3}",
            json!({"a": {"x": 1}}),
        ),
        (
            "an optional field on the other side",
            "#A: {x: int}\na: #A & {y?: int}\na: x: 1",
            json!({"a": {"x": 1}}),
        ),
        (
            "pattern",
            "#A: {[string]: int}\na: #A & {anything: 1}",
            json!({"a": {"anything": 1}}),
        ),
        (
            "two definitions",
            "#A: {x?: int}\n#B: {y?: int}\na: #A & #B",
            json!({"a": {}}),
        ),
        // The file root is never closed, so a definition embedded there constrains
        // nothing the file declares.
        (
            "embedded at the root",
            "#A: {x?: int}\n#A\ny: 1",
            json!({"y": 1}),
        ),
        (
            "schema embedded at the root",
            "_schema\n_schema: #Schema\n#Schema: {a?: b?: string}\na: b: \"foo\"\nc: \"foo\"",
            json!({"a": {"b": "foo"}, "c": "foo"}),
        ),
        (
            "definition selected at the root",
            "root: input: #deps: root: {}\n{root.input.#deps}",
            json!({"root": {"input": {}}}),
        ),
        (
            "schema embedded in a struct",
            "x: {_s: #S, #S: {a?: int}, c: 1, _s}",
            json!({"x": {"c": 1}}),
        ),
    ] {
        assert_eq!(eval_to_json(source).expect(case), expected, "{case}");
    }
}

/// Known divergence: a merge of closed structs allows the union of their fields.
/// Upstream closes per conjunct and rejects `x`, which `#B` does not declare.
#[test]
fn merged_definitions_allow_the_union_of_their_fields() {
    assert_eq!(
        eval_to_json("#A: {x?: int}\n#B: {y?: int}\na: #A & #B & {x: 1}").unwrap(),
        json!({"a": {"x": 1}})
    );
}

/// Known divergence: a definition is not exported, and an error inside one is not
/// reported. Upstream fails with `#B.y: field not allowed`.
#[test]
fn an_error_inside_an_unused_definition_is_not_reported() {
    assert_eq!(
        eval_to_json("#A: {x: int}\n#B: #A & {y: int}").unwrap(),
        json!({})
    );
}

fn module(files: &[(&str, &str)]) -> tempfile::TempDir {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/m@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    module
}

fn origin_options() -> LoadOptions {
    LoadOptions {
        origin: Some(OriginAnnotation {
            marker: "_service".to_string(),
            field: "originDir".to_string(),
        }),
    }
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap().display().to_string()
}

const SCHEMA: &str = "package schema\n\n#Service: {\n\t_service: true\n\tname: string\n\t\
                      originDir?: string\n\tcommand?: string\n}\n";

/// A struct carrying the marker gets the directory of the package that declared it,
/// even through a schema's optional `originDir?` declaration. Nothing else does:
/// a job with a `command` is not a service.
#[test]
fn the_origin_annotation_marks_only_structs_carrying_the_marker() {
    let module = module(&[
        ("schema/schema.cue", SCHEMA),
        (
            "app/app.cue",
            "package app\n\nimport \"example.com/m/schema\"\n\n\
             service: schema.#Service & {name: \"api\", command: \"run\"}\n\
             job: {command: \"test\", name: \"unit\"}\n\
             pinned: schema.#Service & {name: \"db\", originDir: \"/srv\"}\n",
        ),
    ]);
    let (evaluator, root) =
        PackageLoader::load_dir_with(module.path().join("app"), Some("app"), &origin_options())
            .unwrap();
    let json = evaluator.to_json(root).unwrap();

    assert_eq!(
        json["service"]["originDir"],
        json!(canonical(&module.path().join("app")))
    );
    assert_eq!(json["job"], json!({"command": "test", "name": "unit"}));
    assert_eq!(
        json["pinned"]["originDir"],
        json!("/srv"),
        "a set field wins"
    );
}

/// A service declared in an imported package keeps that package's directory.
#[test]
fn an_imported_service_keeps_its_own_package_directory() {
    let module = module(&[
        ("schema/schema.cue", SCHEMA),
        (
            "backend/backend.cue",
            "package backend\n\nimport \"example.com/m/schema\"\n\n\
             api: schema.#Service & {name: \"api\"}\n",
        ),
        (
            "root.cue",
            "package root\n\nimport \"example.com/m/backend\"\n\nservices: [backend.api]\n",
        ),
    ]);
    let (evaluator, root) =
        PackageLoader::load_file_with(module.path().join("root.cue"), &origin_options()).unwrap();
    let json = evaluator.to_json(root).unwrap();

    assert_eq!(
        json["services"][0]["originDir"],
        json!(canonical(&module.path().join("backend")))
    );
}

/// Without the option, nothing is annotated.
#[test]
fn the_origin_annotation_is_opt_in() {
    let module = module(&[
        ("schema/schema.cue", SCHEMA),
        (
            "app.cue",
            "package app\n\nimport \"example.com/m/schema\"\n\n\
             service: schema.#Service & {name: \"api\", command: \"run\"}\n",
        ),
    ]);
    let (evaluator, root) = PackageLoader::load_file(module.path().join("app.cue")).unwrap();
    assert_eq!(
        evaluator.to_json(root).unwrap(),
        json!({"service": {"name": "api", "command": "run"}})
    );
}

/// An import that resolves but fails to load names the import and its error,
/// instead of failing later as a reference that was never found.
#[test]
fn a_broken_import_is_reported_as_the_import() {
    let module = module(&[
        ("lib/lib.cue", "package lib\n\nc: {\n"),
        (
            "app.cue",
            "package app\n\nimport \"example.com/m/lib\"\n\nv: lib.c\n",
        ),
    ]);
    let error = match PackageLoader::load_file(module.path().join("app.cue")) {
        Ok((evaluator, root)) => evaluator.to_json(root).unwrap_err().to_string(),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("import \"example.com/m/lib\""), "{error}");
    assert!(!error.contains("not found"), "{error}");
}
