//! A merged struct derives its recipes again, and each one runs in the scope it
//! was written in.
//!
//! Every expectation here is upstream `cue` v0.16.1's output. The shapes are the
//! ones an independent review found broken when a recipe was re-run against a
//! position - how many literals out a name was found - rather than against the
//! scope that answered for it.

use cue_eval::eval_to_json;
use serde_json::json;

/// The merged struct sits one vertex deeper than the literal was written in.
/// Its reader means the `top` it could see where it was written, not the one
/// the struct now sits beside.
#[test]
fn a_struct_merged_into_a_deeper_vertex_keeps_its_own_scope() {
    assert_eq!(
        eval_to_json(
            r#"
top: "outer"
base: {inner: {q: top}, p: int | *1}
out: {top: "shadow", d: base & {p: 7}}
"#
        )
        .unwrap()["out"]["d"],
        json!({"inner": {"q": "outer"}, "p": 7})
    );
}

/// A comprehension body declares into the struct around it, so a field it
/// generates is derived again by the override like any other.
#[test]
fn a_generated_field_follows_the_override() {
    assert_eq!(
        eval_to_json(
            r#"
base: {p: int | *1, gen: {for i in [1, 2] {"k\(i)": p}}}
out: base & {p: 7}
"#
        )
        .unwrap(),
        json!({
            "base": {"p": 1, "gen": {"k1": 1, "k2": 1}},
            "out": {"p": 7, "gen": {"k1": 7, "k2": 7}},
        })
    );
}

/// A name read where a literal is *entered* - its dynamic label, the source of
/// its comprehension, its `if` condition, what it embeds - belongs to the scope
/// outside it, even when the literal declares that name itself.
#[test]
fn a_name_read_on_the_way_into_a_literal_is_read_outside_it() {
    for (source, expected) in [
        (
            r#"a: {k: "x", b: {k: "y", "\(k)": 1}}"#,
            json!({"k": "y", "y": 1}),
        ),
        (
            r#"a: {k: ["x"], b: {k: ["y"], for v in k {"\(v)": 1}}}"#,
            json!({"k": ["y"], "y": 1}),
        ),
        (
            r#"a: {k: {p: 1}, b: {k: {p: 2}, k}}"#,
            json!({"k": {"p": 2}, "p": 2}),
        ),
        (
            r#"a: {k: true, b: {k: false, if k {x: 1}}}"#,
            json!({"k": false}),
        ),
    ] {
        assert_eq!(
            eval_to_json(source).unwrap()["a"]["b"],
            expected,
            "{source}"
        );
    }
}

/// A struct that carries no recipe - from a builtin here, an imported package
/// elsewhere - is still one side of the merge, so its value survives the
/// derivation of the side that has one. Upstream reports the conflict; this
/// asserts the rejection, since only the message differs.
#[test]
fn a_struct_with_no_recipe_keeps_its_value_through_a_merge() {
    let source = r#"
import "encoding/yaml"
cfg: yaml.Unmarshal("limit: 100\n")
out: cfg & {p: int | *200, limit: p}
"#;
    assert_eq!(
        eval_to_json(source).unwrap()["out"],
        json!({"limit": 100, "p": 200})
    );

    let error = eval_to_json(&format!("{source}o2: out & {{p: 5}}\n"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("100") && error.contains('5'), "{error}");
}

/// Each link of a reference chain is one derivation, so the chain settles
/// however long it is. Seventeen links is past the fixed pass count an earlier
/// version gave up at, which turned an acyclic file into a cycle error.
#[test]
fn a_long_reference_chain_settles() {
    let links = (0..17)
        .rev()
        .map(|i| match i {
            16 => "c16: p".to_string(),
            _ => format!("c{i:02}: c{:02}", i + 1),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let json = eval_to_json(&format!(
        "base: {{p: int | *1, {links}}}\nout: base & {{p: 9}}"
    ))
    .unwrap();

    for i in 0..17 {
        let name = format!("c{i:02}");
        assert_eq!(json["out"][&name], json!(9), "out.{name}");
        assert_eq!(json["base"][&name], json!(1), "base.{name}");
    }
}

/// Deriving a field again must not answer a name the literal never wrote. An
/// embedding contributes `top` to the struct, but `c: top` was written where the
/// outer `top` was the only one in scope, and a merge does not change that.
#[test]
fn a_merge_leaves_a_name_the_literal_did_not_declare_alone() {
    let out = eval_to_json(
        r#"
top: "outer"
mix: {top: "inner"}
base: {c: top, mix, p: int | *1}
out: base & {p: 9}
"#,
    )
    .unwrap();
    assert_eq!(out["base"]["c"], json!("outer"));
    assert_eq!(out["out"]["c"], json!("outer"));
}

/// A pattern target that carries its own pattern constrains through the merge:
/// `[_]` matches every name at declaration and at merge time alike.
#[test]
fn a_nested_pattern_target_applies_through_a_merge() {
    let error = eval_to_json("x: {[_]: [_]: int, e: {p: \"s\"}}\n")
        .unwrap_err()
        .to_string();
    assert!(error.contains("expected int"), "unexpected: {error}");

    let out = eval_to_json("x: {[_]: [_]: int, e: {p: 1}}\n").unwrap();
    assert_eq!(out["x"]["e"]["p"], json!(1));
}

/// Same for a name a comprehension generated: unifying `{p: 9}` has no business
/// changing `c`.
#[test]
fn a_merge_leaves_a_generated_name_alone() {
    let out = eval_to_json(
        r#"
k1: "outer"
base: {c: k1, for i in [1] {"k\(i)": "generated"}, p: int | *1}
out: base & {p: 9}
"#,
    )
    .unwrap();
    assert_eq!(out["base"]["c"], json!("outer"));
    assert_eq!(out["out"]["c"], json!("outer"));
}

/// An imported preset whose comprehensions read its own fields - the `if`
/// body a sibling and a literal `let`, the `for` body its loop value - follows
/// the importer's overrides through one merge and the next. Expectations are
/// cue v0.17.1's output for the same module.
#[test]
fn an_imported_preset_generates_from_the_importers_overrides() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::create_dir_all(root.join("presets")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/m@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("presets/presets.cue"),
        r#"package presets

#Postgres: {
	database: string | *"postgres"
	user:     string | *"admin"
	let owner = "owner \(user)"
	if database != "" {
		postInit: "create \(database) \(owner)"
	}
	ports: [...int] | *[5432]
	for i, p in ports {"port\(i)": p}
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("use.cue"),
        r#"package use

import "example.com/m/presets"

db:    presets.#Postgres & {database: "app"}
other: db & {user: "bob", ports: [1, 2]}
"#,
    )
    .unwrap();

    let (evaluator, value) =
        cue_eval::PackageLoader::load_file(root.join("use.cue")).expect("loads");
    assert_eq!(
        evaluator.to_json(value).expect("exports"),
        json!({
            "db": {
                "database": "app",
                "user": "admin",
                "postInit": "create app owner admin",
                "ports": [5432],
                "port0": 5432,
            },
            "other": {
                "database": "app",
                "user": "bob",
                "postInit": "create app owner bob",
                "ports": [1, 2],
                "port0": 1,
                "port1": 2,
            },
        })
    );
}
