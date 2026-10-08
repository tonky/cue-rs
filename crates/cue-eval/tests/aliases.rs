//! Aliases, as the pinned `cue` (v0.18.0) evaluates them. A module's
//! `language: version` decides which spelling its files use: postfix `~X`,
//! `~(K,V)` from v0.18.0, prefix `X=` before it. A file nobody gave a version
//! accepts both.

use cue_eval::{LoadOptions, PackageLoader, eval_to_json};
use serde_json::json;

fn module(version: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        format!("module: \"example.com/m@v0\"\nlanguage: version: \"{version}\"\n"),
    )
    .unwrap();
    for (path, content) in files {
        std::fs::write(root.join(path), content).unwrap();
    }
    module
}

fn load(dir: &std::path::Path, options: &LoadOptions) -> Result<serde_json::Value, String> {
    let (evaluator, root) =
        PackageLoader::load_dir_with(dir, None, options).map_err(|err| err.to_string())?;
    evaluator.to_json(root).map_err(|err| err.to_string())
}

const PREFIX: &str = "S=src: {v: 2}\ndst: S.v\np: {[N=string]: {name: N}, a: _}\n";
const POSTFIX: &str = "src~S: {v: 2}\ndst: S.v\np: {[string]~(N,_): {name: N}, a: _}\n";

#[test]
fn a_module_parses_its_files_at_its_language_version() {
    let want = json!({"src": {"v": 2}, "dst": 2, "p": {"a": {"name": "a"}}});
    let old = module("v0.17.0", &[("a.cue", PREFIX)]);
    assert_eq!(load(old.path(), &LoadOptions::default()), Ok(want.clone()));
    let new = module("v0.18.0", &[("a.cue", POSTFIX)]);
    assert_eq!(load(new.path(), &LoadOptions::default()), Ok(want));

    let err = load(
        module("v0.17.0", &[("a.cue", POSTFIX)]).path(),
        &LoadOptions::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("postfix alias syntax requires @experiment(aliasv2)"),
        "{err}"
    );
    let err = load(
        module("v0.18.0", &[("a.cue", PREFIX)]).path(),
        &LoadOptions::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("old-style alias syntax (=) is not allowed"),
        "{err}"
    );
}

#[test]
fn without_a_module_the_default_version_decides() {
    // Under the target directory: a temp directory may sit below some
    // other module, which the loader finds as upstream does.
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::write(dir.path().join("a.cue"), PREFIX).unwrap();
    assert!(load(dir.path(), &LoadOptions::default()).is_ok());
    let at_oracle = LoadOptions {
        default_language_version: Some("v0.18.0".to_string()),
        ..LoadOptions::default()
    };
    let err = load(dir.path(), &at_oracle).unwrap_err();
    assert!(
        err.contains("old-style alias syntax (=) is not allowed"),
        "{err}"
    );
}

#[test]
fn a_pattern_binds_each_label_and_field() {
    let json = eval_to_json(
        "pattern: {[string]~(K, V): {key: K, n: V.a + 1}, x: a: 1}\n\
         self: {[string]~X: {a: 1, b: X.a + 1}, x: {}}\n",
    )
    .unwrap();
    assert_eq!(
        json,
        json!({
            "pattern": {"x": {"a": 1, "key": "x", "n": 2}},
            "self": {"x": {"a": 1, "b": 2}},
        })
    );
}

/// A definition's pattern closes each field it matches over what it adds for
/// that label, also when a merge derives the field again.
#[test]
fn a_definitions_aliased_pattern_closes_over_what_it_adds() {
    let json = eval_to_json(
        "#T: [string]~(N, _): {path: string, content: string, n: N}\n\
         #Mid: #T & {o: path: \"/o\"}\n\
         val: #Mid & {o: content: \"foo\"}\n",
    )
    .unwrap();
    assert_eq!(
        json,
        json!({"val": {"o": {"path": "/o", "content": "foo", "n": "o"}}})
    );
    let err = eval_to_json(
        "#T: [string]~(N, _): {path: string, n: N}\nbad: #T & {p: {path: \"a\", extra: 1}}\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("field not allowed"), "{err}");
}

#[test]
fn field_quoted_and_dynamic_labels_bind_their_aliases() {
    let json = eval_to_json(
        "src~(K, V): {k: K}\nkey: K\nv: V.k\n\
         \"a-b\"~(Q, W): 3\nq: Q\nw: W\n\
         _name: \"dyn\"\n(_name)~(_, D): 4\nd: D\n",
    )
    .unwrap();
    assert_eq!(
        json,
        json!({
            "src": {"k": "src"}, "key": "src", "v": "src",
            "a-b": 3, "q": "a-b", "w": 3,
            "dyn": 4, "d": 4,
        })
    );
}

#[test]
fn an_unreferenced_alias_is_an_error_as_upstream() {
    // A field alias nobody reads is refused; a label alias is never checked.
    let err = eval_to_json("a~X: 1\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("unreferenced alias or let clause X"),
        "{err}"
    );
    assert!(eval_to_json("a~(K, _): 1\n").is_ok());
    assert!(eval_to_json("p: [string]~(K, _): int\n").is_ok());
}

#[test]
fn an_alias_and_a_field_of_one_name_may_not_see_each_other() {
    for source in [
        "X: 5\no: {let X = 1, b: X}\n",
        "o: {let X = 1, b: X}\nX: 5\n",
        "o: {let X = 1, b: X, p: {X: 2}}\n",
        "o: {X: 5, a~X: 1, b: X}\n",
        "X: 5\no: {[string]~(X, _): {n: X}, b: {}}\n",
        "X: 5\no: {if true {let X = 1, b: X}}\n",
        "X: 5\no: [{let X = 1, b: X}]\n",
    ] {
        let err = eval_to_json(source).unwrap_err().to_string();
        assert!(
            err.contains("cannot have both alias and field with name \"X\" in same scope"),
            "{source}: {err}"
        );
    }
    // A quoted label declares no name; a comprehension's variables and `let`
    // clauses are no aliases; a sibling's scope is not this one's.
    for source in [
        "\"X\": 5\no: {let X = 1, b: X}\n",
        "X: 5\no: [for x in [1] let X = x {b: X}]\n",
        "X: 5\no: {for k, X in {a: 1} {(k): X}}\n",
        "o: {a~X: 1, b: X}\np: {X: 5}\n",
        // A pattern's aliases name something only inside its value.
        "t: {[string]~(r): {b: r.r + 1}, a: {r: 0}}\n",
    ] {
        assert!(eval_to_json(source).is_ok(), "{source}");
    }
}
