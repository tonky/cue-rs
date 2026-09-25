use cue_eval::{BottomKind, Evaluator, Value, eval_to_json};
use serde_json::json;

fn evaluate(source: &str) -> (Evaluator, cue_eval::ValueId) {
    let mut evaluator = Evaluator::new();
    let id = evaluator
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    (evaluator, id)
}

#[test]
fn scalar_fields_remain_selectable_while_operations_read_the_payload() {
    let source = include_str!("../../../tests/conformance/regressions/metadata/values.txtar")
        .strip_prefix("-- in.cue --\n")
        .unwrap();
    assert_eq!(
        eval_to_json(source).unwrap(),
        json!({
            "a":3,"unit":"kg","sum":5,"b":[1,2],"index":2,"concat":[1,2,1,2],
            "s":"foo","join":"foo--bar","h":["foo"]
        })
    );
    let (evaluator, id) = evaluate("{3, #unit: \"kg\"}");
    assert!(matches!(evaluator.arena.get(id), Some(Value::Int(_))));
    let fields = evaluator.arena.fields(id).unwrap();
    assert_eq!(
        evaluator.to_json(fields.definitions["#unit"].val).unwrap(),
        "kg"
    );
}

#[test]
fn specialized_recipes_recompute_without_changing_their_source() {
    let value = eval_to_json(
        r#"
#Sum: {#a+#b, #a:int, #b:int}
#Default: {#x+1, #x:*1|int}
a: (#Sum & {_, #a:1, #b:3})+2
b: ({_, #a:1, #b:3} & #Sum)+2
base: #Default+0
changed: (#Default & {_, #x:5}) & 6
again: #Default+0
nested: {value:#Default} & {value:{_, #x:5}&6}
"#,
    )
    .unwrap();
    assert_eq!(
        value,
        json!({"a":6,"b":6,"base":2,"changed":6,"again":2,"nested":{"value":6}})
    );
    assert!(eval_to_json("#D: {#x+1,#x:*1|int}\n(#D & {_,#x:5}) & 2").is_err());
}

#[test]
fn disjunction_trials_keep_distinct_metadata_and_recompute_recipes() {
    let source = include_str!("../../../tests/conformance/regressions/metadata/choices.txtar")
        .strip_prefix("-- in.cue --\n")
        .unwrap();
    let value = eval_to_json(source).unwrap();
    assert_eq!(value["field"], 2);
    assert_eq!(value["value"], 3);
    assert_eq!(value["nestedValue"], 6);
    assert_eq!(value["recipeValue"], 7);
}

#[test]
fn scalar_recipes_preserve_lexical_scope_and_partial_self_references() {
    let source = include_str!("../../../tests/conformance/regressions/metadata/scopes.txtar")
        .strip_prefix("-- in.cue --\n")
        .unwrap();
    let value = eval_to_json(source).unwrap();
    assert_eq!(value["aValue"], 8);
    assert_eq!(value["bValue"], 9);
    assert_eq!(value["next"], 4);
    assert_eq!(value["cValue"], 11);
}

#[test]
fn contradictory_metadata_is_rejected_but_absent_optional_fields_are_allowed() {
    for source in [
        "{3,x:1}",
        "{3,#bad:1&2}",
        "{3,_bad:1&2}",
        "{3,#x:1}&{_,#x:2}",
        "{3,#x:1}&{#x:1}",
    ] {
        let (evaluator, id) = evaluate(source);
        assert!(
            matches!(evaluator.arena.get(id), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Conflict),
            "{source}"
        );
    }
    for source in ["{3,bad?:1&2}", "{3,[string]:int}", "{3,#x:1}&{_,#x:1}"] {
        assert_eq!(eval_to_json(source).unwrap(), 3, "{source}");
    }
}

#[test]
fn rollback_removes_scalar_fields_with_their_owner() {
    let mut evaluator = Evaluator::new();
    let before = evaluator.arena.checkpoint();
    let old = evaluator
        .eval_file(&cue_syntax::parse_file("{3,#unit:1}").unwrap())
        .unwrap();
    assert!(evaluator.arena.fields(old).is_some());
    evaluator.arena.rollback(before);
    assert!(evaluator.arena.get(old).is_none());
    assert!(evaluator.arena.fields(old).is_none());
    let new = evaluator.arena.int(4);
    assert_ne!(old, new);
    assert!(evaluator.arena.fields(new).is_none());
}

#[test]
fn mixed_choices_close_each_branch_together_with_its_common_fields() {
    assert_eq!(
        eval_to_json(
            r#"
#Mixed: {*1 | {f:int} | {g:string}, #x:3, common?:bool}
a: #Mixed & {f:3,common:true}
b: {g:"ok",common:false} & #Mixed
tag: #Mixed.#x
#Extended: {#Mixed, extra?:bool}
c: #Extended & {f:4,extra:true}
#Nested: {*1 | {f:{n:int}}, #x:3}
d: #Nested & {f:{n:3}}
"#
        )
        .unwrap(),
        json!({"a":{"f":3,"common":true},"b":{"g":"ok","common":false},
            "tag":3,"c":{"f":4,"extra":true},"d":{"f":{"n":3}}})
    );
    for source in [
        "#D: {*1|{f:int}|{g:int},#x:3}\nb: #D & {f:3,g:4}",
        "#D: {*1|{f:int},#x:3}\nb: #D & {f:3,z:4}",
        "#D: {*1|{f:{n:int}},#x:3}\nb: #D & {f:{n:3,z:4}}",
        "#D: {_ | {f:int},#x:3}\nb: #D & {z:4}",
    ] {
        let (evaluator, id) = evaluate(source);
        let id = evaluator
            .arena
            .fields(id)
            .map_or(id, |fields| fields.fields["b"].val);
        assert!(
            matches!(evaluator.arena.get(id), Some(Value::Bottom(reason))
            if reason.kind == BottomKind::Conflict),
            "{source}"
        );
    }
}

#[test]
fn choice_field_views_survive_narrowing_and_follow_arena_rollback() {
    assert_eq!(
        eval_to_json(
            r#"
#D: {*1|2|{f:int},#x:int}
a: #D & {_,#x:3}
tag: a.#x
value: a+0
narrowed: ((1|2) & #D & {_,#x:4}).#x
"#
        )
        .unwrap(),
        json!({"a":1,"tag":3,"value":1,"narrowed":4})
    );
    let mut evaluator = Evaluator::new();
    let checkpoint = evaluator.arena.checkpoint();
    let id = evaluator
        .eval_file(&cue_syntax::parse_file("{*1|2|{f:int},#x:3}").unwrap())
        .unwrap();
    assert!(matches!(
        evaluator.arena.get(id),
        Some(Value::Disjunction { .. })
    ));
    assert_eq!(
        evaluator
            .to_json(evaluator.arena.fields(id).unwrap().definitions["#x"].val)
            .unwrap(),
        3
    );
    evaluator.arena.rollback(checkpoint);
    assert!(evaluator.arena.get(id).is_none());
    assert!(evaluator.arena.fields(id).is_none());
}

#[test]
fn dynamic_choices_refresh_struct_branches_without_changing_their_source() {
    assert_eq!(
        eval_to_json(
            r#"
#D: {*(#x+1)|{f:#x},#x:*1|int}
pair: {left:#D,right:#D} & {left:{f:3,#x:3},right:{_,#x:5}}
original: #D+0
#Named: {*(#x+1)|{"\(#key)":#x},#x:*1|int,#key:*"old"|string}
named: #Named & {new:3,#x:3,#key:"new"}
"#
        )
        .unwrap(),
        json!({"pair":{"left":{"f":3},"right":6},"original":2,"named":{"new":3}})
    );
}

#[test]
fn dynamic_branch_selectors_preserve_private_fields_and_struct_constraints() {
    for (source, expected) in [
        (
            include_str!(
                "../../../tests/conformance/regressions/dynamic-choice/branch-fields.txtar"
            ),
            json!({"s":4,"sv":"scalar","t":{"f":3},"tv":"struct","open":2,"a":{"f":3},"av":3}),
        ),
        (
            include_str!("../../../tests/conformance/regressions/dynamic-choice/required.txtar"),
            json!({"a":{"f":3},"av":3,"base":1,"b":{"f":4},"bv":4}),
        ),
    ] {
        assert_eq!(
            eval_to_json(source.strip_prefix("-- in.cue --\n").unwrap()).unwrap(),
            expected
        );
    }
}
