//! A format may move text, never meaning: what it writes parses back to the tree it was
//! given.
//!
//! The parser keeps no parentheses, so the formatter derives every pair from the tree —
//! for binary and unary operands, and for the operand of every postfix operator (selector,
//! index, slice, call, spread), which binds tighter than any operator. Missing that last
//! group wrote `(#A & {raw: "a"}).out` as `#A & {raw: "a"}.out`, which selects `out` from
//! the literal instead of from the unification.
//!
//! The tables pin outputs against `cue fmt` v0.17.1; the generated cases check the
//! round trip over every operator nested in every other, which is what catches the class
//! rather than the instances.

use cue_syntax::ast::{BinaryOp, Decl, Expr};
use cue_syntax::{SourceFile, format_file, parse_file};

fn parse(source: &str) -> SourceFile {
    parse_file(source).unwrap_or_else(|error| panic!("{source:?} does not parse: {error}"))
}

/// Formats `source`, checks that the output parses back to the same tree and is a fixed
/// point, and returns it.
fn round_trip(source: &str) -> String {
    let tree = parse(source);
    let formatted = format_file(&tree);
    let reparsed = parse_file(&formatted).unwrap_or_else(|error| {
        panic!("{source:?} formats to {formatted:?}, which does not parse: {error}")
    });
    assert_eq!(
        reparsed, tree,
        "{source:?} formats to {formatted:?}, which parses to another tree"
    );
    assert_eq!(
        format_file(&reparsed),
        formatted,
        "{source:?}: not idempotent"
    );
    formatted
}

/// Sources `cue fmt` writes back unchanged, and so must this formatter.
const AS_UPSTREAM: &[&str] = &[
    r#"(#A & {raw: "a"}).out"#,
    "(a | b).c",
    "(a + b).c",
    "(-a).b",
    "(a & b)[0]",
    "(a | b)[1:2]",
    "(a + b)[:2]",
    "(a & b)(1)",
    "(f | g)(x)",
    "(-f)(x)",
    "(a & b).c[0](1)",
    "(a & b)...",
    "-(a + b)",
    "!(a && b)",
    "!(a == b)",
    "<(a + 1)",
    "- -1",
    "+ +1",
    ">=-1",
    // Joined, these read as other tokens: `<-` is upstream's arrow, `>==` is `>=` `=`.
    "<(-1)",
    ">(==1)",
    r#"!(=~"a")"#,
    "*-1 | 2",
    r#"a."b-c""#,
    r#"a."true""#,
    r#"a."""#,
    "a.in",
    "true || (false & false)",
    "(a | b) & c",
    "a || (b | c)",
    "a && (b || c)",
    "(a & b) && c",
    "a - (b - c)",
    "a / (b * c)",
    "a == (b == c)",
    "(a < b) + c",
    "(a + b) * c",
    "a * (b + c)",
    "x & (*a)",
    "(*a | b) & c",
    "a | (b | c)",
    "[for x in s {y: x}]",
    "[for x in s {x}]",
    "[for x in s {(a | b).c}]",
    r#""\((a | b).c)""#,
];

#[test]
fn parentheses_the_tree_needs_are_written_as_upstream_writes_them() {
    for source in AS_UPSTREAM {
        let source = format!("v: {source}\n");
        assert_eq!(round_trip(&source), source);
    }
}

/// Where the output differs from `cue fmt`'s, and why each difference is safe.
const DIVERGENT: &[(&str, &str, &str)] = &[
    (
        "a pair the tree does not need is not remembered",
        "(true || false) & false",
        "true || false & false",
    ),
    ("so neither is this one", "(a && b) || c", "a && b || c"),
    ("left association", "(a - b) - c", "a - b - c"),
    ("a selector chain", "(a.b).c", "a.b.c"),
    // Upstream reads `1.f` as the number `1.` and a stray `f`.
    ("a selector on a number", "1.f", "(1).f"),
    ("a selector under a unary", "-(a.b)", "-a.b"),
    ("a quoted selector that is an identifier", r#"a."d""#, "a.d"),
    ("a double negation", "-(-1)", "- -1"),
    ("a double not", "!(!a)", "!!a"),
];

#[test]
fn where_the_output_differs_from_upstream_it_differs_safely() {
    for (what, source, expected) in DIVERGENT {
        let formatted = round_trip(&format!("v: {source}\n"));
        assert_eq!(formatted, format!("v: {expected}\n"), "{what}");
    }
}

#[test]
fn logical_operators_bind_tighter_than_unification_and_disjunction() {
    // The spec's ladder, loosest first: `|`, `&`, `||`, `&&`. `true || false & false` is
    // `(true || false) & false`, a conflict upstream; ranking `||` above `|` read it as
    // `true || (false & false)`, which is `true`.
    let value = |source: &str| match parse(&format!("v: {source}\n")).decls.as_slice() {
        [Decl::Field(field)] => field.value.clone(),
        decls => panic!("{decls:?}"),
    };
    let top = |source: &str| match value(source) {
        Expr::Binary { op, .. } => Some(op),
        Expr::Disjunction { .. } => None,
        other => panic!("{other:?}"),
    };
    assert_eq!(top("true || false & false"), Some(BinaryOp::Unify));
    assert_eq!(top("a && b & c"), Some(BinaryOp::Unify));
    assert_eq!(top("a || b | c"), None);
    assert_eq!(top("a && b || c"), Some(BinaryOp::LogicalOr));
    assert_eq!(top("a == b && c"), Some(BinaryOp::LogicalAnd));
}

/// A deterministic xorshift, so a failure names a case that reproduces.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }
}

const ATOMS: &[&str] = &[
    "a", "b", "#A", "_h", "1", "2.5", r#""s""#, "null", "true", "_", "{x: 1}", "[1, 2]", "[...int]",
];
const BINARY: &[&str] = &[
    "|", "&", "||", "&&", "==", "!=", "<", "<=", ">", ">=", "=~", "!~", "+", "-", "*", "/",
];
const UNARY: &[&str] = &[
    "+", "-", "!", "*", "<", "<=", ">", ">=", "==", "!=", "=~", "!~",
];
const POSTFIX: &[&str] = &[
    ".f",
    r#"."b-c""#,
    ".in",
    "[0]",
    "[1:2]",
    "[:x]",
    "(1, y)",
    "()",
    "...",
];

/// An operand: an atom, or an expression in parentheses — which the tree forgets, so the
/// formatter has to find again every pair that mattered.
fn operand(rng: &mut Rng, depth: usize) -> String {
    match depth == 0 || rng.below(3) == 0 {
        true => rng.pick(ATOMS).to_string(),
        false => format!("({})", expr(rng, depth - 1)),
    }
}

fn expr(rng: &mut Rng, depth: usize) -> String {
    if depth == 0 {
        return rng.pick(ATOMS).to_string();
    }
    match rng.below(5) {
        0 | 1 => {
            let (left, right) = (operand(rng, depth), operand(rng, depth));
            format!("{left} {} {right}", rng.pick(BINARY))
        }
        2 => format!("{}{}", rng.pick(UNARY), operand(rng, depth)),
        3 => format!("{}{}", operand(rng, depth), rng.pick(POSTFIX)),
        _ => {
            let (left, right) = (operand(rng, depth), operand(rng, depth));
            format!("*{left} | {right}")
        }
    }
}

#[test]
fn every_operator_nested_in_every_other_survives_a_format() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let source = format!("v: {}\n", expr(&mut rng, 4));
        round_trip(&source);
    }
}
