//! Capture retention follows the inputs a recipe can read.
use cue_eval::{Evaluator, Value, eval_to_json};
use serde_json::json;

#[test]
fn constants_keep_constraints_without_retaining_lexical_environments() {
    let source = r#"
base: {
    constant: {a: 1, values: ["yes", 2 + 3], optional?: 7}
    choice: *1 | 2
    input: *1 | int
    derived: input
}
result: base & {input: 2, choice: 2, constant: {optional: 7}}
"#;
    let mut eval = Evaluator::new();
    let root = eval
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let Value::Struct(root_value) = eval.arena.get(root).unwrap() else {
        panic!()
    };
    let Value::Struct(base) = eval.arena.get(root_value.fields["base"].val).unwrap() else {
        panic!()
    };
    assert!(!base.fields["constant"].has_thunk());
    assert!(!base.fields["choice"].has_thunk());
    assert!(base.fields["derived"].has_thunk());
    assert_eq!(
        eval.to_json(root).unwrap()["result"],
        json!({
            "constant":{"a":1,"values":["yes",5],"optional":7},
            "choice":2,"input":2,"derived":2
        })
    );
    assert!(eval_to_json(&format!("{source}\nresult: constant: a: 2")).is_err());
}

#[test]
fn dependent_nested_fields_keep_lexical_captures_after_merges() {
    let source = r#"
outer: 7
base: {
    input: *1 | int
    derived: {lexical: outer, sibling: input}
}
result: base & {input: 2, outer: 99}
"#;
    assert_eq!(
        eval_to_json(source).unwrap()["result"]["derived"],
        json!({"lexical":7,"sibling":2})
    );
}

#[test]
fn identical_recipe_syntax_shares_storage_but_keeps_distinct_bindings() {
    use cue_eval::value::Conjunct;
    use std::rc::Rc;
    let mut eval = Evaluator::new();
    let source = r#"
first: {outer: 1, value: outer}
second: {outer: 2, value: outer}
result: first & {value: 1}
"#;
    let root = eval
        .eval_file(&cue_syntax::parse_file(source).unwrap())
        .unwrap();
    let Value::Struct(root_value) = eval.arena.get(root).unwrap() else {
        panic!()
    };
    let recipe = |name: &str| {
        let Value::Struct(value) = eval.arena.get(root_value.fields[name].val).unwrap() else {
            panic!()
        };
        let Conjunct::Thunk(thunk) = &value.fields["value"].conjuncts[0] else {
            panic!()
        };
        thunk
    };
    let first = recipe("first");
    let second = recipe("second");
    assert!(Rc::ptr_eq(&first.expr, &second.expr));
    assert!(Rc::ptr_eq(&first.deps, &second.deps));
    assert!(!Rc::ptr_eq(&first.env, &second.env));
    let output = eval.to_json(root).unwrap();
    assert_eq!(output["first"]["value"], 1);
    assert_eq!(output["second"]["value"], 2);
    assert_eq!(output["result"]["value"], 1);
}

#[test]
fn shared_syntax_expands_let_dependencies_in_each_lexical_scope() {
    let source = r#"
#Left: {left: *1 | int, let binding = left, let alias = binding, output: alias}
#Right: {right: *2 | int, let binding = right, let alias = binding, output: alias}
a: #Left & {left: 10}
b: #Right & {right: 20}
c: #Left & {left: 30}
"#;
    assert_eq!(
        eval_to_json(source).unwrap(),
        json!({
            "a":{"left":10,"output":10},"b":{"right":20,"output":20},"c":{"left":30,"output":30}
        })
    );
}
