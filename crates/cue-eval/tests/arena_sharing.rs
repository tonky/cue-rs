use cue_eval::{Evaluator, Value, ValueArena};
use std::rc::Rc;

#[test]
fn boolean_cache_respects_rollback_generations_and_graph_mutation() {
    let mut arena = ValueArena::new();
    let before = arena.checkpoint();
    let yes = arena.bool(true);
    let after_yes = arena.checkpoint();
    let no = arena.bool(false);
    assert_eq!(arena.bool(true), yes);
    assert_eq!(arena.bool(false), no);

    arena.rollback(after_yes);
    let replacement = arena.string("reused slot");
    let new_no = arena.bool(false);
    assert_ne!(new_no, no);
    assert_ne!(new_no, replacement);
    assert_eq!(arena.get(new_no), Some(&Value::Bool(false)));
    assert_eq!(arena.bool(true), yes);

    *arena.get_mut(yes).unwrap() = Value::Bool(false);
    let new_yes = arena.bool(true);
    assert_ne!(new_yes, yes);
    assert_eq!(arena.get(new_yes), Some(&Value::Bool(true)));

    arena.rollback(before);
    let fresh = arena.bool(true);
    assert_ne!(fresh, new_yes);
    assert_eq!(arena.get(fresh), Some(&Value::Bool(true)));
    assert_ne!(arena.alloc(Value::Bool(true)), fresh);
}

#[test]
fn unchanged_scalar_meets_survive_speculative_rollback() {
    let mut arena = ValueArena::new();
    for value in [
        Value::Null,
        Value::Bool(true),
        Value::Int(123.into()),
        Value::Float(1.5),
        Value::String("text".repeat(100)),
        Value::Bytes(vec![1, 2, 3]),
    ] {
        let left = arena.alloc(value.clone());
        let right = arena.alloc(value.clone());
        let checkpoint = arena.checkpoint();
        let result = cue_eval::unify::unify(&mut arena, left, right);
        assert_eq!(result, left);
        arena.rollback(checkpoint);
        assert_eq!(arena.get(result), Some(&value));
    }
}

#[test]
fn string_cache_tracks_mutation_rollback_and_cloned_arenas() {
    let mut arena = ValueArena::default();
    let original = arena.string("original");
    let checkpoint = arena.checkpoint();
    assert_eq!(arena.string("original"), original);
    let abandoned = arena.string("abandoned");
    arena.rollback(checkpoint);
    // Force slot reuse with a different payload before retrying the string.
    let other = arena.int(42);
    let retry = arena.string("abandoned");
    assert_ne!(retry, abandoned);
    assert_ne!(retry, other);
    assert_eq!(arena.get(retry), Some(&Value::String("abandoned".into())));

    let mut copy = arena.clone();
    *arena.get_mut(original).unwrap() = Value::String("changed".into());
    let new_original = arena.string("original");
    assert_ne!(new_original, original);
    assert_eq!(
        arena.get(new_original),
        Some(&Value::String("original".into()))
    );
    assert_eq!(copy.string("original"), original);
    assert_eq!(copy.get(original), Some(&Value::String("original".into())));

    // Rolling back a fresh equal string must not evict the older cached owner.
    let checkpoint = arena.checkpoint();
    arena.alloc(Value::String("original".into()));
    arena.rollback(checkpoint);
    assert_eq!(arena.string("original"), new_original);
}

#[test]
#[cfg(feature = "memory-profile")]
fn abandoned_branch_text_does_not_accumulate_in_the_string_index() {
    let mut arena = ValueArena::new();
    let kept = arena.string("kept");
    for branch in 0..128 {
        let checkpoint = arena.checkpoint();
        for field in 0..64 {
            arena.string(format!("branch {branch}, field {field}"));
            assert_eq!(arena.string("kept"), kept);
        }
        arena.rollback(checkpoint);
    }
    let profile = arena.memory_profile();
    assert_eq!(profile["slots"], 1);
    assert_eq!(profile["string_cache_entries"], 1);
    assert_eq!(profile["string_cache_text_bytes"], 4);
    assert!(profile["slot_capacity"].as_u64().unwrap() < 256);
}

#[test]
fn field_recipes_share_storage_without_sharing_later_constraints() {
    let mut eval = Evaluator::new();
    let file = cue_syntax::parse_file(
        "outer: 7\nbase: {x: outer}\ncopy: base & {y: 2}\nconstrained: base & {x: 7}",
    )
    .unwrap();
    let root = eval.eval_file(&file).unwrap();
    let fields = eval.arena.fields(root).unwrap();
    let recipe =
        |name: &str| &eval.arena.fields(fields.fields[name].val).unwrap().fields["x"].conjuncts;
    assert!(Rc::ptr_eq(recipe("base"), recipe("copy")));
    assert!(!Rc::ptr_eq(recipe("base"), recipe("constrained")));
    assert_eq!(recipe("base").len(), 1);
    assert_eq!(recipe("constrained").len(), 2);
    assert_eq!(eval.to_json(root).unwrap()["constrained"]["x"], 7);
}

#[test]
fn shared_scalar_payloads_keep_distinct_metadata_owners() {
    let value = cue_eval::eval_to_json(
        "a: {true, #unit: 1}\nb: {true, #unit: 2}\nx: a.#unit\ny: b.#unit\nz: true\n\
         s: {\"same\", #unit: 3}\nt: {\"same\", #unit: 4}\nu: s.#unit\nv: t.#unit",
    )
    .unwrap();
    assert_eq!(
        value,
        serde_json::json!({"a":true,"b":true,"x":1,"y":2,"z":true,
        "s":"same", "t":"same", "u":3, "v":4})
    );
}
