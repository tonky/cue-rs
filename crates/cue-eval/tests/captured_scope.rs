//! A field that reads its own struct through the struct's name, and a recipe
//! that reads a name bound after its literal was evaluated.
//!
//! `_L: {"a": [...], "b": _L["a"]}` never resolved: only a regular field bound
//! the partial value its pending pass produced, so a hidden field (and a
//! definition, read through its placeholder) could not read itself. And a
//! definition's comprehension whose body was not run when the definition was
//! evaluated (`for f in features` over `[...string]`) kept the scope as it stood
//! then: merged with `{features: ["b"]}`, the body ran without the table it
//! iterates - declared further down, or still partial - and generated nothing.
//! enve's `pkgs.#PlaywrightBrowsers` lost its Linux libraries that way. A recipe
//! derived again now reads each frame of its scope as it stands
//! (`ScopeFrame::current`).
//!
//! A recipe derived again that still waits on a reference no longer keeps what
//! it generated before the merge: while a loop around can retry, the field the
//! merge is part of waits too (`Evaluator::waiting_reruns`); after that, a
//! field's recipe exports its error (`r.fetch.z: undefined field: z`, where the
//! stale `fetch: {}` was exported) and a struct's comprehension leaves the
//! struct incomplete. The goldens come from cue v0.17.1; regenerate them with
//! `regen.py` in `fixtures/captured_scope`.

mod support;

#[test]
fn captured_scope_matches_cue_export() {
    support::match_cue_export("captured_scope", 29, &[]);
}

/// The enve shape across a package boundary: the package is evaluated on its
/// own first, and the importer's merge derives the definition's comprehension
/// again in the package's scope.
#[test]
fn a_package_definition_reads_a_table_declared_below_it() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::create_dir_all(root.join("pkgs")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/m@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("pkgs/browsers.cue"),
        r#"package pkgs

#Browsers: {
	features: [...string]
	let linux = {for browser in features for name in _libs[browser] {(name): true}}
	buildInputs: [for name, _ in linux {name}]
}
_libs: {
	"chromium-headless-shell": ["glibc", "nss"]
	"chromium": _libs["chromium-headless-shell"]
	"ffmpeg": ["glibc"]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("main.cue"),
        r#"package main

import "example.com/m/pkgs"

r: pkgs.#Browsers & {features: ["chromium", "ffmpeg"]}
"#,
    )
    .unwrap();
    let (evaluator, id) = cue_eval::PackageLoader::load_file(root.join("main.cue")).unwrap();
    assert_eq!(
        evaluator.to_json(id).unwrap()["r"]["buildInputs"],
        serde_json::json!(["glibc", "nss"])
    );
}
