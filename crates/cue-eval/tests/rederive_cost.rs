//! Re-derivation costs what the merge changed, not what the struct holds.
//!
//! These assert on the evaluator's own counters rather than on wall time, which
//! would be flaky. `derivations` counts the field recipes re-derivation ran;
//! `unsettled` counts the structs it gave up on at the sweep cap, and a value
//! returned from there is one the engine knows it has not finished deriving.

use cue_eval::Evaluator;
use cue_syntax::parse_file;

fn eval(source: &str) -> (serde_json::Value, usize, usize) {
    let file = parse_file(source).expect("parses");
    let mut evaluator = Evaluator::new();
    let root = evaluator.eval_file(&file).expect("evaluates");
    let json = evaluator.to_json(root).expect("exports");
    (json, evaluator.derivations, evaluator.unsettled)
}

/// A large literal beside one overridden field. Nothing in it reads `p`, so
/// nothing in it is derived again - and comparing it against itself would
/// exhaust the equivalence budget, which is what used to turn this shape into
/// 256 sweeps over the whole subtree.
#[test]
fn a_large_struct_beside_an_override_is_not_rederived() {
    let mut source = String::from("base: {\n\tp: int | *1\n\tc: \"v\\(p)\"\n\tdata: {\n");
    for i in 0..600 {
        source.push_str(&format!(
            "\t\tf{i}: {{a: {i}, b: \"x{i}\", c: [{i}, {i}, {i}], d: {{e: {i}}}}}\n"
        ));
    }
    source.push_str("\t}\n}\nout: base & {p: 9}\n");

    let (json, derivations, unsettled) = eval(&source);
    assert_eq!(json["out"]["p"], serde_json::json!(9));
    assert_eq!(json["out"]["c"], serde_json::json!("v9"));
    assert_eq!(json["out"]["data"]["f599"]["a"], serde_json::json!(599));
    assert_eq!(unsettled, 0);
    assert!(
        derivations < 32,
        "one override re-derived {derivations} recipes"
    );
}

/// A chain of references settles in one sweep whichever way its names sort, so
/// its length does not decide whether the far end sees the override.
#[test]
fn a_reference_chain_settles_whichever_way_it_sorts() {
    for ascending in [true, false] {
        let links = 300;
        let mut source = String::from("base: {\n\tp: int | *1\n");
        let name = |i: usize| {
            if ascending {
                format!("c{i:03}")
            } else {
                format!("c{:03}", links - 1 - i)
            }
        };
        source.push_str(&format!("\t{}: p\n", name(0)));
        for i in 1..links {
            source.push_str(&format!("\t{}: {}\n", name(i), name(i - 1)));
        }
        source.push_str("}\nout: base & {p: 9}\n");

        let (json, _, unsettled) = eval(&source);
        assert_eq!(
            json["out"][name(links - 1)],
            serde_json::json!(9),
            "ascending={ascending}"
        );
        assert_eq!(unsettled, 0, "ascending={ascending}");
    }
}

/// The same, one import away. A package is now evaluated into the importer's
/// arena so its fields keep their recipes, which is what lets the override reach
/// `c` at all - and which also makes every one of them a candidate to derive
/// again. The gate has to survive the boundary: only what reads `p` is derived.
#[test]
fn a_large_imported_definition_beside_an_override_is_not_rederived() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::create_dir_all(root.join("big")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/cost@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();

    let mut big =
        String::from("package big\n\n#Big: {\n\tp: int | *1\n\tc: \"v\\(p)\"\n\tdata: {\n");
    for i in 0..600 {
        big.push_str(&format!(
            "\t\tf{i}: {{a: {i}, b: \"x{i}\", c: [{i}, {i}, {i}], d: {{e: {i}}}}}\n"
        ));
    }
    big.push_str("\t}\n}\n");
    std::fs::write(root.join("big/big.cue"), big).unwrap();
    std::fs::write(
        root.join("use.cue"),
        "package use\n\nimport \"example.com/cost/big\"\n\nout: big.#Big & {p: 9}\n",
    )
    .unwrap();

    let (evaluator, value) =
        cue_eval::PackageLoader::load_file(root.join("use.cue")).expect("loads");
    let json = evaluator.to_json(value).expect("exports");

    assert_eq!(json["out"]["p"], serde_json::json!(9));
    assert_eq!(json["out"]["c"], serde_json::json!("v9"));
    assert_eq!(json["out"]["data"]["f599"]["a"], serde_json::json!(599));
    assert_eq!(evaluator.unsettled, 0);
    assert!(
        evaluator.derivations < 32,
        "one override re-derived {} recipes across an import",
        evaluator.derivations
    );
}
