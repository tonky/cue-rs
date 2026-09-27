//! Missing-field selects through definitions are definite errors.
//!
//! A struct may gain fields on a later pass, so `a.b` on a regular struct is
//! `incomplete`. A definition is a template, not a growing value, and
//! everything evaluated under one is decided: `#a.b` on `#a: {}` and
//! `arg.y` inside `#T` are `UndefinedFieldDefinite` (observed as `eval`,
//! still retried while passes run). Expectations mirror upstream `cue`
//! on the `references_errors` and `issue318` shapes.

use cue_eval::{BottomKind, Evaluator, Value};

fn bottom_kind_at(source: &str, path: &[&str]) -> BottomKind {
    let mut eval = Evaluator::new();
    let root = eval
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let mut current = root;
    for name in path {
        let Value::Struct(s) = eval.arena.get(current).unwrap() else {
            panic!("{name} parent is not a struct");
        };
        current = s
            .fields
            .get(*name)
            .or_else(|| s.definitions.get(*name))
            .unwrap_or_else(|| panic!("no field {name}"))
            .val;
    }
    match eval.arena.get(current) {
        Some(Value::Bottom(reason)) => reason.kind,
        other => panic!("{} is not bottom: {other:?}", path.last().unwrap()),
    }
}

#[test]
fn select_through_a_regular_struct_stays_incomplete() {
    assert_eq!(
        bottom_kind_at("m: {a: {}, r: a.b}", &["m", "r"]),
        BottomKind::UndefinedField
    );
}

#[test]
fn select_through_a_definition_is_definite() {
    assert_eq!(
        bottom_kind_at("m: {#a: {}, r: #a.b}", &["m", "r"]),
        BottomKind::UndefinedFieldDefinite
    );
}

#[test]
fn nested_select_through_a_definition_is_definite() {
    assert_eq!(
        bottom_kind_at("m: {#a: {}, r: #a.d.c}", &["m", "r"]),
        BottomKind::UndefinedFieldDefinite
    );
}

#[test]
fn select_under_a_definition_is_definite() {
    assert_eq!(
        bottom_kind_at("#T: {arg: x: string, vy: arg.y}", &["#T", "vy"]),
        BottomKind::UndefinedFieldDefinite
    );
}
