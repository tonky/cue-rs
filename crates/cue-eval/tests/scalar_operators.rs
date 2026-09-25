use cue_eval::{BottomKind, Evaluator, Value, eval_to_json};
use serde_json::json;

#[test]
fn default_operands_are_selected_without_freezing_later_overrides() {
    let source = r#"
#Base: {input: *1 | int, derived: input + 1, negated: -input, positive: +input}
first: #Base
second: #Base & {input: 3}
third: {input: 4} & #Base
truth: *true | bool
negated: !truth
and: truth && true
or: truth || false
comparison: (*1 | int) < 2
"#;
    let value = eval_to_json(source).unwrap();
    assert_eq!(
        value["first"],
        json!({"input":1,"derived":2,"negated":-1,"positive":1})
    );
    assert_eq!(
        value["second"],
        json!({"input":3,"derived":4,"negated":-3,"positive":3})
    );
    assert_eq!(
        value["third"],
        json!({"input":4,"derived":5,"negated":-4,"positive":4})
    );
    assert_eq!(value["negated"], false);
    assert_eq!(value["and"], true);
    assert_eq!(value["or"], true);
    assert_eq!(value["comparison"], true);
}

#[test]
fn integer_division_variants_preserve_sign_invariants_and_big_values() {
    for a in [-7, 0, 7] {
        for b in [-3, 3] {
            let source =
                format!("d: div({a},{b})\nm: mod({a},{b})\nq: quo({a},{b})\nr: rem({a},{b})");
            let value = eval_to_json(&source).unwrap();
            let read = |key| value[key].as_i64().unwrap();
            assert_eq!(a, b * read("d") + read("m"));
            assert!((0..b.abs()).contains(&read("m")));
            assert_eq!(a, b * read("q") + read("r"));
            assert_eq!(read("q"), a / b);
            assert_eq!(read("r"), a % b);
        }
    }
    let value =
        eval_to_json("a: quo(18446744073709551617, 2)\nb: rem(18446744073709551617, 2)").unwrap();
    assert_eq!(value["a"].to_string(), "9223372036854775808");
    assert_eq!(value["b"], 1);
}

#[test]
fn numeric_literals_and_comparisons_do_not_round_big_integers() {
    let value = eval_to_json(
        r#"
hex: 0x10000000000000001
binary: 0b10000000000000000000000000000000000000000000000000000000000000001
octal: 0o2000000000000000000001
scaled: 10000000000000000K
fraction: 1.5Ki
equal: 2 == 2.0
reverse: 2.0 == 2
unequal: 9007199254740993 != 9007199254740992.0
less: 9007199254740992.0 < 9007199254740993
negative: -2 > -2.5
"#,
    )
    .unwrap();
    for key in ["hex", "binary", "octal"] {
        assert_eq!(value[key].to_string(), "18446744073709551617");
    }
    assert_eq!(value["scaled"].to_string(), "10000000000000000000");
    assert_eq!(value["fraction"], 1536);
    for key in ["equal", "reverse", "unequal", "less", "negative"] {
        assert_eq!(value[key], true, "{key}");
    }
    assert!(eval_to_json("a: 1.1Ki").is_err());
}

#[test]
fn division_is_floating_and_invalid_operations_have_typed_errors() {
    let mut eval = Evaluator::new();
    let root = eval
        .eval_file(
            &cue_syntax::parse_file(
                r#"
value: 2 / 3 * 6
zero: 1.0 / 0
undefined: 0 / 0
positive: +"bad"
ambiguous: (*1 | *2) + 1
plain: (1 | 2) + 1
wrongKind: div(1.0, 2)
"#,
            )
            .unwrap(),
        )
        .unwrap();
    let Value::Struct(value) = eval.arena.get(root).unwrap() else {
        panic!()
    };
    assert!(
        matches!(eval.arena.get(value.fields["value"].val), Some(Value::Float(n)) if *n == 4.0)
    );
    for name in ["zero", "undefined", "positive", "wrongKind"] {
        assert!(
            matches!(eval.arena.get(value.fields[name].val), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Conflict),
            "{name}"
        );
    }
    for name in ["ambiguous", "plain"] {
        assert!(
            matches!(eval.arena.get(value.fields[name].val), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Incomplete),
            "{name}"
        );
    }
    for op in ["div", "mod", "quo", "rem"] {
        assert!(eval_to_json(&format!("a: {op}(1, 0)")).is_err());
    }
}

#[test]
fn following_a_cyclic_default_from_the_public_arena_terminates() {
    use cue_eval::value::DisjunctionBranch;
    use cue_syntax::ast::{Expr, UnaryOp};
    let mut eval = Evaluator::new();
    let id = eval.arena.top();
    *eval.arena.get_mut(id).unwrap() = Value::Disjunction {
        branches: vec![DisjunctionBranch {
            default: true,
            val: id,
        }],
    };
    eval.insert_binding("cycle", id);
    let result = eval
        .eval_expr(&Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Ident("cycle".into())),
        })
        .unwrap();
    assert!(
        matches!(eval.arena.get(result), Some(Value::Bottom(reason)) if reason.kind == BottomKind::Cycle)
    );
}

#[test]
fn incomplete_arithmetic_can_be_resolved_by_a_later_constraint() {
    let output = eval_to_json(
        r#"
#Schema: {input: int, output: input + 1}
a: #Schema & {input: 5}
b: {input: 7} & #Schema
"#,
    )
    .unwrap();
    assert_eq!(
        output,
        json!({"a":{"input":5,"output":6},"b":{"input":7,"output":8}})
    );
}
