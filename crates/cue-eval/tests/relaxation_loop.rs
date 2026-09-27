//! The declaration loop settles order-independently and refines partials.
//!
//! Characterization for the RelaxationLoop extraction: pass counts and
//! outcomes pinned here so moving the driver cannot silently change them.
//! `refinements` counts passes that resolved nothing but grew a partial;
//! `unsettled` counts structs abandoned at the sweep cap.

use cue_eval::Evaluator;
use cue_syntax::parse_file;

fn eval(source: &str) -> (serde_json::Value, usize, usize) {
    let file = parse_file(source).expect("parses");
    let mut evaluator = Evaluator::new();
    let root = evaluator.eval_file(&file).expect("evaluates");
    let json = evaluator.to_json(root).expect("exports");
    (json, evaluator.refinements, evaluator.unsettled)
}

/// A use before its definition resolves: declaration order is not
/// evaluation order. Progress, not refinement, settles it.
#[test]
fn forward_references_resolve_without_refinement() {
    let (json, refinements, unsettled) = eval("out: later\nlater: 42\n");
    assert_eq!(json["out"], serde_json::json!(42));
    assert_eq!(refinements, 0);
    assert_eq!(unsettled, 0);
}

#[test]
fn backward_references_resolve_without_refinement() {
    let (json, refinements, unsettled) = eval("earlier: 1\nout: earlier + 1\n");
    assert_eq!(json["out"], serde_json::json!(2));
    assert_eq!(refinements, 0);
    assert_eq!(unsettled, 0);
}

/// A nested forward reference refines: passes resolve nothing while the
/// partial grows, then the chain settles once the anchor binds.
#[test]
fn nested_forward_reference_refines_then_settles() {
    let source = "top: {m: {v: top.n}, n: 2}\nout: top.m.v\n";
    let (json, refinements, unsettled) = eval(source);
    assert_eq!(json["out"], serde_json::json!(2));
    assert_eq!(refinements, 4);
    assert_eq!(unsettled, 0);
}

/// Cycles terminate with bottom at export, never with a hang. Each shape
/// below spends a different amount of refinement allowance first.
#[test]
fn cycles_terminate_with_bottom_at_export() {
    let shapes = [
        // Mutual scalar references.
        ("a: b\nb: a\nout: a\n", 1),
        // A struct containing itself.
        ("a: {x: a}\n", 18),
        // A list containing its own struct.
        ("a: {x: [a]}\nout: 1\n", 19),
    ];
    for (source, refinements) in shapes {
        let file = parse_file(source).expect("parses");
        let mut evaluator = Evaluator::new();
        let root = evaluator.eval_file(&file).expect("evaluates");
        assert!(
            evaluator.to_json(root).is_err(),
            "expected export bottom for {source:?}"
        );
        assert_eq!(evaluator.refinements, refinements, "for {source:?}");
        assert_eq!(evaluator.unsettled, 0, "for {source:?}");
    }
}
