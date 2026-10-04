use super::*;

#[test]
fn repeated_fields_fold_into_unification() {
    let file = cue_syntax::parse_file("a: 1\na: 2\nb: 3\n").unwrap();
    let folded = collect_field_declarations(&file.decls);
    let fields: Vec<_> = folded
        .iter()
        .filter_map(|decl| match decl {
            Decl::Field(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 2);
    assert!(matches!(fields[0].value, Expr::Binary { .. }));
    assert_eq!(collect_field_names(&file.decls).len(), 2);
}

#[test]
fn unrepeated_fields_borrow_without_cloning() {
    let file = cue_syntax::parse_file("a: 1\nb: 2\nc: {d: 3}\n").unwrap();
    let folded = collect_field_declarations(&file.decls);
    assert!(
        matches!(folded, Cow::Borrowed(_)),
        "a literal with no repeated name must not clone its declarations"
    );
    assert_eq!(folded.len(), 3);
}

#[test]
fn derivation_orders_readers_after_what_they_read() {
    use crate::value::{Conjunct, FieldEntry, Imports, Thunk, ThunkEnv, ValueArena};
    use std::rc::Rc;

    let mut arena = ValueArena::new();
    let mut s = StructValue::new(false);
    for (name, deps) in [("c", &["b"][..]), ("b", &["a"][..]), ("a", &[][..])] {
        let thunk = Thunk {
            expr: Rc::new(Expr::Top),
            env: Rc::new(ThunkEnv::new(
                Vec::new(),
                Vec::new(),
                HashSet::new(),
                Rc::new(Imports::default()),
            )),
            deps: Rc::new(deps.iter().map(|s| s.to_string()).collect()),
        };
        let val = arena.top();
        let mut entry = FieldEntry::value(val, false);
        entry.extend_conjuncts([Conjunct::Thunk(thunk)]);
        s.fields.insert(name.to_string(), entry);
    }
    let order = derivation_order(&s);
    let names: Vec<&str> = order.iter().map(|(_, name)| name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
}

#[test]
fn refinement_detects_new_and_changed_bindings() {
    let mut arena = ValueArena::new();
    let one = arena.int(1);
    let two = arena.int(2);
    let file = cue_syntax::parse_file("a: 1\n#D: 2\n").unwrap();
    let pending: Vec<&Decl> = file.decls.iter().collect();
    let field = &pending[..1];
    // Newly bound.
    assert!(refined_any(&arena, &|_| Some(one), field, &[None]));
    // Same value id: not a change.
    assert!(!refined_any(&arena, &|_| Some(one), field, &[Some(one)]));
    // Different value: a change.
    assert!(refined_any(&arena, &|_| Some(two), field, &[Some(one)]));
    // Lost binding is not refinement.
    assert!(!refined_any(&arena, &|_| None, field, &[Some(one)]));
    // Definitions never count, bound or not.
    assert!(!refined_any(
        &arena,
        &|_| Some(one),
        &pending[1..2],
        &[None]
    ));
}

#[test]
fn moved_seeds_merges_and_pattern_matches_only() {
    use crate::value::{Conjunct, FieldEntry, PatternConstraint};
    let mut arena = ValueArena::new();
    let pattern = arena.string("a1");
    let target = arena.top();
    let mut s = StructValue::new(false);
    // Two conjuncts: merged, always seeded.
    s.fields.insert(
        "merged".to_string(),
        FieldEntry::with_conjuncts(
            arena.top(),
            false,
            vec![Conjunct::Value(arena.top()), Conjunct::Value(arena.top())],
        ),
    );
    // Single conjunct, no pattern match: settled, never seeded.
    s.fields
        .insert("settled".to_string(), FieldEntry::value(arena.top(), false));
    // Single conjunct matching the pattern: seeded.
    s.fields
        .insert("a1".to_string(), FieldEntry::value(arena.top(), false));
    s.pattern_constraints.push(PatternConstraint {
        pattern_val: pattern,
        target_val: target,
    });
    let moved = seed_moved(&arena, &s);
    assert!(moved.contains("merged"), "{moved:?}");
    assert!(moved.contains("a1"), "{moved:?}");
    assert!(!moved.contains("settled"), "{moved:?}");
}

#[test]
fn arriving_at_a_cycle_is_not_refinement() {
    let mut arena = ValueArena::new();
    let unresolved = arena.bottom_of(BottomKind::Unresolved, "incomplete value");
    let cycle = arena.bottom_of(BottomKind::Cycle, "cycle with field: a");
    let file = cue_syntax::parse_file("a: 1\n").unwrap();
    let pending: Vec<&Decl> = file.decls.iter().collect();
    // Newly bound to a cycle: terminal, no further pass is earned.
    assert!(!refined_any(&arena, &|_| Some(cycle), &pending, &[None]));
    // Newly bound to anything still retryable: progress as before.
    assert!(refined_any(
        &arena,
        &|_| Some(unresolved),
        &pending,
        &[None]
    ));
    // Discovering the cycle on a later pass ends the retrying too.
    assert!(!refined_any(
        &arena,
        &|_| Some(cycle),
        &pending,
        &[Some(unresolved)]
    ));
}

#[test]
fn only_ordinary_fields_bind_scope_names() {
    let file = cue_syntax::parse_file("a: 1\n#D: 2\n_h: 3\n").unwrap();
    let names: Vec<_> = file.decls.iter().filter_map(pending_binding_name).collect();
    assert_eq!(names, ["a"]);
}
