//! A field reading the field that encloses it, and the verdict when it cannot.
//!
//! ```cue
//! stages: {
//!     build: {name: "build"}
//!     test: {name: "test", needs: [stages.build]}
//! }
//! ```
//!
//! Referring to a sibling through the enclosing field is how every `depends_on` / `needs`
//! graph is written, and cue-rs answered `_|_ (unresolved reference 'stages')` for all of
//! them: a static field's name was bound only *after* its value was evaluated, so while
//! `stages` was being built its own name was not in scope, and no later pass changed that
//! because nothing had ever bound it. A pending declaration now binds the partial value
//! its pass produced, and a pass that refines a partial counts as progress, which is what
//! lets a chain of them resolve a link at a time.
//!
//! Every expectation below is upstream `cue` v0.16.1's output, recorded on 2026-09-22.

use cue_eval::{PackageLoader, eval_to_json};
use serde_json::json;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/self_reference")
        .join(name)
}

/// The shape from the report, written as one struct literal.
#[test]
fn a_field_reads_a_sibling_through_the_field_that_encloses_them() {
    assert_eq!(
        eval_to_json(
            r#"
            stages: {
                build: {name: "build"}
                test: {name: "test", needs: [stages.build]}
            }
            "#
        )
        .unwrap(),
        json!({"stages": {
            "build": {"name": "build"},
            "test": {"name": "test", "needs": [{"name": "build"}]},
        }})
    );
}

/// The same graph written as repeated paths, which is how a package of several
/// files produces it. These fold into one `&` chain before evaluation, so the
/// struct is built by a single declaration and the pass that resolves it has to
/// resolve the whole thing.
#[test]
fn the_same_graph_written_as_repeated_paths() {
    assert_eq!(
        eval_to_json(
            r#"
            stages: build: name: "build"
            stages: test: {name: "test", needs: [stages.build]}
            "#
        )
        .unwrap(),
        json!({"stages": {
            "build": {"name": "build"},
            "test": {"name": "test", "needs": [{"name": "build"}]},
        }})
    );
}

/// Under a pattern constraint and a definition, which is what a schema adds. The
/// constraint is what used to make this the hard case: an unresolved reference in
/// one field collapsed the whole struct to bottom, taking with it the sibling the
/// reference was waiting for.
#[test]
fn the_same_graph_under_a_pattern_constraint() {
    assert_eq!(
        eval_to_json(
            r#"
            #Stage: {name: string, needs?: [...#Stage]}
            stages: [string]: #Stage
            stages: build: name: "build"
            stages: test: {name: "test", needs: [stages.build]}
            "#
        )
        .unwrap(),
        json!({"stages": {
            "build": {"name": "build"},
            "test": {"name": "test", "needs": [{"name": "build"}]},
        }})
    );
}

/// A chain: `c` needs `b`, which needs `a`. One pass resolves one link, so this
/// is the case that needs a refined partial to count as progress - the literal
/// is one declaration, and no pass of it finishes until every link has.
#[test]
fn a_chain_of_them_resolves_a_link_per_pass() {
    assert_eq!(
        eval_to_json(
            r#"
            stages: {
                a: {name: "a"}
                b: {name: "b", needs: [stages.a]}
                c: {name: "c", needs: [stages.b]}
            }
            "#
        )
        .unwrap(),
        json!({"stages": {
            "a": {"name": "a"},
            "b": {"name": "b", "needs": [{"name": "a"}]},
            "c": {"name": "c", "needs": [{"name": "b", "needs": [{"name": "a"}]}]},
        }})
    );
}

/// Four links deep, to pin that the chain is not a special case of two.
#[test]
fn and_keeps_resolving_past_the_second_link() {
    assert_eq!(
        eval_to_json(
            r#"
            s: {
                one:   s.two
                two:   s.three
                three: s.four
                four:  4
            }
            "#
        )
        .unwrap(),
        json!({"s": {"one": 4, "two": 4, "three": 4, "four": 4}})
    );
}

/// The partial bound for one pass must not survive into the value if a later pass
/// changes what it reads. Here the default is overridden by the conjunct, and a
/// self-reference that kept its pass-one reading would export `q: 1`.
#[test]
fn a_partial_bound_for_one_pass_does_not_go_stale() {
    assert_eq!(
        eval_to_json("u: {p: int | *1, q: u.p} & {p: 9}").unwrap(),
        json!({"u": {"p": 9, "q": 9}})
    );
}

/// Through an import: the enclosing field's type comes from another package, so
/// the pattern constraint and the definition are resolved across the boundary
/// while the reference is still pending.
#[test]
fn a_sibling_is_reachable_when_the_schema_is_imported() {
    let (evaluator, root) = PackageLoader::load_dir(fixture("app")).unwrap();
    assert_eq!(
        evaluator.to_json(root).unwrap(),
        json!({"pipeline": {"stages": {
            "build": {"name": "build"},
            "test": {"name": "test", "needs": [{"name": "build"}]},
        }}})
    );
}

/// Input CUE refuses, and the path and wording it must be refused with. Upstream
/// v0.16.1 rejects every one of these; the wording is its own, the paths are the
/// paths it reports.
const REJECTED: &[(&str, &str, &str)] = &[
    // a name nothing declares
    ("a: {x: nope}", "a.x", r#"reference "nope" not found"#),
    // a base that resolves, without that field
    ("x: {p: 1}\nx: {r: x.zzz}", "x.r", "undefined field: zzz"),
    // a field that is its own value
    ("a: a", "a", "incomplete value"),
    // a struct that contains itself: the value is infinite
    ("a: {x: a}", "a.x", r#"reference "a" not found"#),
    // and two of them that contain each other
    ("p: {q: r}\nr: {s: p}", "p.q", r#"reference "r" not found"#),
];

#[test]
fn a_reference_that_never_resolves_is_refused_where_it_is_written() {
    for (source, path, message) in REJECTED {
        let error = eval_to_json(source)
            .err()
            .unwrap_or_else(|| panic!("{source:?} was accepted"))
            .to_string();
        assert!(
            error.contains(&format!("'{path}'")) && error.contains(message),
            "{source:?} was refused, but not as {path}: {message}\n  got: {error}"
        );
    }
}

/// Keeping a pending reference at its own field must not keep a *conflict* there:
/// two values that cannot both hold make the struct bottom, which every caller
/// relies on.
#[test]
fn a_conflict_still_collapses_the_struct_that_holds_it() {
    let error = eval_to_json("a: {b: 1}\na: {b: 2}")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("conflicting values") && error.contains("'a'"),
        "{error}"
    );
}

/// A required field that is still the lazy node a recursive definition expands
/// through is a structural cycle - the value is infinite. It used to be exported
/// as the internal placeholder string `"<ref:#Stage>"`.
#[test]
fn a_structural_cycle_is_refused_rather_than_exported_as_a_placeholder() {
    let error = eval_to_json("#Stage: {name: string, parent: #Stage}\ns: #Stage & {name: \"a\"}")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("structural cycle") && error.contains("s.parent"),
        "{error}"
    );
}

/// The same definition with the recursive field optional is not a cycle: nothing
/// requires the infinite part, and upstream exports it.
#[test]
fn the_same_definition_with_the_recursion_optional_exports() {
    assert_eq!(
        eval_to_json("#Stage: {name: string, needs?: [...#Stage]}\ns: #Stage & {name: \"build\"}")
            .unwrap(),
        json!({"s": {"name": "build"}})
    );
}
