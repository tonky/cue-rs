//! A disjunction keeps its width when it meets a copy of itself.
//!
//! `unify_disjunction_inner` unifies every branch of one side against every branch of the
//! other and keeps each result that is not bottom. Unifying `*"sql" | "tcp"` with a
//! structurally equal copy examines four pairs, two of which succeed - producing `"sql"`
//! and `"tcp"` again. Keeping those without asking whether they are already there doubles
//! the width for no change in meaning, and a package whose four files each unify one
//! service disjunction reached 2^23 branches and 3.1 GB before the allocator refused.
//!
//! The counts below are what makes these tests worth having: an assertion on the exported
//! value alone passes just as happily at 2^23 branches, a moment before the abort.
//!
//! Three of these fail if the deduplication is removed - at 7, 7 and 1024 branches. The
//! other two guard the opposite mistake, a deduplication that drops a branch it should
//! have kept or loses which branch was the default, and they pass either way.

use cue_eval::{Evaluator, PackageLoader, Value, ValueArena, ValueId};
use cue_syntax::parse_file;
use serde_json::json;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/disjunction_width/app")
}

/// How many branches the value at `path` holds. One for a value that is not a
/// disjunction, which is what a fully resolved field looks like.
fn width(arena: &ValueArena, root: ValueId, path: &[&str]) -> usize {
    let mut id = root;
    for step in path {
        let Some(Value::Struct(s)) = arena.get(id) else {
            panic!("{path:?}: '{step}' is not under a struct");
        };
        id = s
            .fields
            .get(*step)
            .unwrap_or_else(|| panic!("{path:?}: no field '{step}'"))
            .val;
    }
    match arena.get(id) {
        Some(Value::Disjunction { branches }) => branches.len(),
        _ => 1,
    }
}

fn eval(source: &str) -> (Evaluator, ValueId) {
    let file = parse_file(source).expect("parses");
    let mut evaluator = Evaluator::new();
    let root = evaluator.eval_file(&file).expect("evaluates");
    (evaluator, root)
}

/// Each `#Dn` is written out separately, so the copies are equal in content and not in
/// identity - which is the case `v1_id == v2_id` at the top of `unify_internal` does not
/// catch, and the one several files in a package produce.
fn copies(n: usize, body: &str) -> String {
    let defs: String = (0..n).map(|i| format!("#D{i}: {body}\n")).collect();
    let conjuncts: Vec<String> = (0..n).map(|i| format!("#D{i}")).collect();
    format!("{defs}x: {}\n", conjuncts.join(" & "))
}

/// Branches that all conflict with each other cannot double: every cross pair is bottom,
/// so two branches meeting two branches leaves two. It takes a branch that unifies with
/// more than one branch of the other side - here `*"sql" | string`, where `string`
/// accepts what `"sql"` accepts - which is exactly the permissive alternative a schema
/// grows when it wants to allow a value it has not enumerated.
const OVERLAPPING: &str = r#"*"sql" | "tcp" | string"#;

#[test]
fn a_disjunction_meeting_copies_of_itself_keeps_its_width() {
    for n in 1..=8 {
        let (evaluator, root) = eval(&copies(n, OVERLAPPING));
        assert_eq!(
            width(&evaluator.arena, root, &["x"]),
            3,
            "{n} copies widened the disjunction"
        );
    }
}

#[test]
fn the_same_holds_for_a_disjunction_nested_in_a_struct_field() {
    // The abort's actual shape: the doubling was inside a field of a struct being
    // unified, not at the top level of the value.
    let body = format!("{{proto: {OVERLAPPING}, host: *\"127.0.0.1\" | string}}");
    for n in 1..=8 {
        let (evaluator, root) = eval(&copies(n, &body));
        assert_eq!(
            width(&evaluator.arena, root, &["x", "proto"]),
            3,
            "{n} copies"
        );
        assert_eq!(
            width(&evaluator.arena, root, &["x", "host"]),
            2,
            "{n} copies"
        );
    }
}

#[test]
fn a_branch_that_differs_is_kept() {
    // Deduplication must not be the accidental kind that drops anything it cannot tell
    // apart: three branches unified with two gives the pairs that actually unify.
    let source = concat!(
        "#A: {t: \"a\"} | {t: \"b\"} | {t: \"c\"}\n",
        "#B: {t: \"a\"} | {t: \"c\"}\n",
        "x: #A & #B\n"
    );
    let (evaluator, root) = eval(source);
    assert_eq!(width(&evaluator.arena, root, &["x"]), 2);
    assert_eq!(
        cue_eval::eval_to_json(&format!("{source}y: x & {{t: \"c\"}}\n")).unwrap()["y"],
        json!({"t": "c"})
    );
}

#[test]
fn the_default_mark_survives_from_either_side() {
    // A survivor inherits the default of every copy it absorbs. Losing it would change
    // which branch an ambiguous disjunction exports, which is a wrong value rather than
    // a wide one.
    for source in [
        "#A: *\"x\" | \"y\"\n#B: \"x\" | \"y\"\nv: #A & #B\n",
        "#A: \"x\" | \"y\"\n#B: *\"x\" | \"y\"\nv: #A & #B\n",
        "#A: *\"x\" | \"y\"\n#B: *\"x\" | \"y\"\nv: #A & #B\n",
    ] {
        assert_eq!(
            cue_eval::eval_to_json(source).unwrap()["v"],
            json!("x"),
            "{source}"
        );
    }
}

#[test]
fn a_package_whose_files_each_unify_the_same_disjunction_stays_narrow() {
    // The reproducer, shrunk: five files in one package, each contributing its own
    // conjunct to `pipeline`. `cue export` v0.16.1 leaves this service two branches wide
    // and says so - `incomplete value {…} | {…}` - because the schema's last branch
    // accepts any kind. Two is therefore the right width, and before deduplication this
    // fixture reached 1024: one doubling per unification, which at the scale of the
    // package this was found in is 2^23 branches and 3.1 GB.
    let (evaluator, root) = PackageLoader::load_dir(fixture()).expect("loads");
    assert_eq!(
        width(&evaluator.arena, root, &["pipeline", "services", "db"]),
        2,
        "the service disjunction did not keep the width upstream gives it"
    );

    // The unambiguous part of the same value still exports, and every file's conjunct
    // reached it.
    let json = evaluator.to_json(root).expect("exports");
    assert_eq!(json["pipeline"]["name"], json!("width"));
    for name in ["one", "two", "three", "four"] {
        assert_eq!(json["pipeline"]["tasks"][name], json!(name));
    }
}
