//! The shape a file was written in, and the columns a block of fields aligns in.
//!
//! `cue fmt` is position-preserving: a composite written on one line stays on it, one
//! written over lines stays over them, and a run of simple fields has its values aligned
//! in a column. None of that is in the value, so the parser records it and the formatter
//! gives it back — the recipe [`StringForm`] established for string spellings.
//!
//! Every expectation below is the byte-for-byte output of `cue fmt` v0.16.1 on the input
//! beside it. A case that stops matching is a case where the two formatters have parted,
//! which is a decision to take, not a test to adjust.
//!
//! [`StringForm`]: cue_syntax::StringForm

use cue_syntax::{format_file, parse_file};

/// What is pinned, the source, and what `cue fmt` writes for it.
const CASES: &[(&str, &str, &str)] = &[
    (
        "a struct written on one line stays on it",
        "package p\n\na: {b: 1, c: 2}\n",
        "package p\n\na: {b: 1, c: 2}\n",
    ),
    (
        "a struct written over lines stays over them",
        "package p\n\na: {\n\tb: 1\n\tc: 2\n}\n",
        "package p\n\na: {\n\tb: 1\n\tc: 2\n}\n",
    ),
    (
        "a nested struct decides for itself",
        "package p\n\na: {b: {\n\tc: 1\n}}\n",
        "package p\n\na: {b: {\n\tc: 1\n}}\n",
    ),
    (
        "the brace after a unification is a brace like any other",
        "package p\n\na: b & {c: 1}\n",
        "package p\n\na: b & {c: 1}\n",
    ),
    (
        "a list written on one line stays on it",
        "package p\n\na: [1, 2, 3]\n",
        "package p\n\na: [1, 2, 3]\n",
    ),
    (
        "a list written over lines stays over them",
        "package p\n\na: [\n\t1,\n\t2,\n]\n",
        "package p\n\na: [\n\t1,\n\t2,\n]\n",
    ),
    (
        "a path is written as a path",
        "package p\n\nx: {\n\ta: b: 1\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\ta: b: 1\n\tlongerName: 2\n}\n",
    ),
    (
        "consecutive fields share a column",
        "package p\n\nx: {\n\tshort: 1\n\tlongerName: 2\n\tmid: 3\n}\n",
        "package p\n\nx: {\n\tshort:      1\n\tlongerName: 2\n\tmid:        3\n}\n",
    ),
    (
        "a blank line ends the block",
        "package p\n\nx: {\n\tshort: 1\n\tlongerName: 2\n\n\tmid: 3\n}\n",
        "package p\n\nx: {\n\tshort:      1\n\tlongerName: 2\n\n\tmid: 3\n}\n",
    ),
    (
        "a comment ends the block",
        "package p\n\nx: {\n\tshort: 1\n\t// why\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: 1\n\t// why\n\tlongerName: 2\n}\n",
    ),
    (
        "an embedding ends the block",
        "package p\n\nx: {\n\ta: 1\n\tfoo\n\tbbbb: 2\n}\n",
        "package p\n\nx: {\n\ta: 1\n\tfoo\n\tbbbb: 2\n}\n",
    ),
    (
        "a let ends the block",
        "package p\n\nx: {\n\tlet q = 1\n\tbbbb: 2\n}\n",
        "package p\n\nx: {\n\tlet q = 1\n\tbbbb: 2\n}\n",
    ),
    (
        "an ellipsis ends the block",
        "package p\n\nx: {\n\tshort: 1\n\t...\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: 1\n\t...\n\tlongerName: 2\n}\n",
    ),
    (
        "a value written over lines ends the block",
        "package p\n\nx: {\n\tshort: 1\n\tnested: {\n\t\tq: 1\n\t}\n\tmid: 3\n}\n",
        "package p\n\nx: {\n\tshort: 1\n\tnested: {\n\t\tq: 1\n\t}\n\tmid: 3\n}\n",
    ),
    (
        "a field holding a struct takes no part, wherever it sits",
        "package p\n\nx: {\n\tshort: 1\n\tc: {d: 1}\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: 1\n\tc: {d: 1}\n\tlongerName: 2\n}\n",
    ),
    (
        "and the fields around it still align",
        "package p\n\nx: {\n\tshort: 1\n\tlongerName: 2\n\tc: {d: 1}\n}\n",
        "package p\n\nx: {\n\tshort:      1\n\tlongerName: 2\n\tc: {d: 1}\n}\n",
    ),
    (
        "a field holding a list takes no part either",
        "package p\n\nx: {\n\tshort: 1\n\tlongerName: 2\n\tc: [1]\n}\n",
        "package p\n\nx: {\n\tshort:      1\n\tlongerName: 2\n\tc: [1]\n}\n",
    ),
    (
        "nor does one holding a composite inside a call",
        "package p\n\nx: {\n\tshort: f({d: 1})\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: f({d: 1})\n\tlongerName: 2\n}\n",
    ),
    (
        "nor one holding an empty struct",
        "package p\n\nx: {\n\tshort: {}\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: {}\n\tlongerName: 2\n}\n",
    ),
    (
        "a multi-line string is not a cell",
        "package p\n\nx: {\n\tshort: \"\"\"\n\t\tx\n\t\t\"\"\"\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\tshort: \"\"\"\n\t\tx\n\t\t\"\"\"\n\tlongerName: 2\n}\n",
    ),
    (
        "each nesting level has its own columns",
        "package p\n\nx: {\n\ta: 1\n\tbb: {\n\t\tc: 1\n\t\tdddd: 2\n\t}\n\teee: 3\n}\n",
        "package p\n\nx: {\n\ta: 1\n\tbb: {\n\t\tc:    1\n\t\tdddd: 2\n\t}\n\teee: 3\n}\n",
    ),
    (
        "an optional marker belongs to the label",
        "package p\n\nx: {\n\ta?: 1\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\ta?:         1\n\tlongerName: 2\n}\n",
    ),
    (
        "so do the quotes of a string label",
        "package p\n\nx: {\n\t\"a b\": 1\n\tbbbb: 2\n}\n",
        "package p\n\nx: {\n\t\"a b\": 1\n\tbbbb:  2\n}\n",
    ),
    (
        "and the hash of a definition",
        "package p\n\n#A: 1\n#BBBB: 2\n",
        "package p\n\n#A:    1\n#BBBB: 2\n",
    ),
    (
        "one space is the narrowest a column gets",
        "package p\n\nx: {\n\ta: 1\n\tbb: 2\n}\n",
        "package p\n\nx: {\n\ta:  1\n\tbb: 2\n}\n",
    ),
    (
        "attributes take a column of their own",
        "package p\n\nx: {\n\tshort: 1000 @go(A)\n\tlongerName: 2 @go(B)\n}\n",
        "package p\n\nx: {\n\tshort:      1000 @go(A)\n\tlongerName: 2    @go(B)\n}\n",
    ),
    (
        "a column ends where the cell after it does",
        "package p\n\nx: {\n\tshort: 1 @go(A)\n\tlongerName: 10000\n}\n",
        "package p\n\nx: {\n\tshort:      1 @go(A)\n\tlongerName: 10000\n}\n",
    ),
    (
        "two blocks either side of a blank line size separately",
        "package p\n\nx: {\n\ta: 1\n\tbbbb: 2\n\t\n\tcc: 3\n\tddddddd: 4\n}\n",
        "package p\n\nx: {\n\ta:    1\n\tbbbb: 2\n\n\tcc:      3\n\tddddddd: 4\n}\n",
    ),
    (
        "a path aligns against paths of its own depth",
        "package p\n\nx: {\n\ta: b: 1\n\tccccc: dd: 2\n}\n",
        "package p\n\nx: {\n\ta: b:      1\n\tccccc: dd: 2\n}\n",
    ),
    (
        "a deeper path does not",
        "package p\n\nx: {\n\ta: b: c: 1\n\tdd: ee: 2\n}\n",
        "package p\n\nx: {\n\ta: b: c: 1\n\tdd: ee: 2\n}\n",
    ),
    (
        "nor does a plain field",
        "package p\n\nx: {\n\ta: b: 1\n\tlongerName: 2\n}\n",
        "package p\n\nx: {\n\ta: b: 1\n\tlongerName: 2\n}\n",
    ),
    (
        "and the block resumes after one",
        "package p\n\nx: {\n\ta: b: 1\n\tcc: dd: 2\n\te: 3\n}\n",
        "package p\n\nx: {\n\ta: b:   1\n\tcc: dd: 2\n\te: 3\n}\n",
    ),
    (
        "an attribute takes a path's braces back",
        "package p\n\nx: {\n\ta: b: 1 @go(A)\n\tcc: dd: 2 @go(B)\n}\n",
        "package p\n\nx: {\n\ta: {\n\t\tb: 1 @go(A)\n\t}\n\tcc: {\n\t\tdd: 2 @go(B)\n\t}\n}\n",
    ),
];

fn formatted(src: &str) -> String {
    let file = parse_file(src).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e:?}"));
    format_file(&file)
}

#[test]
fn every_case_is_written_the_way_upstream_writes_it() {
    let wrong: Vec<_> = CASES
        .iter()
        .filter(|(_, src, expected)| formatted(src) != *expected)
        .map(|(what, src, expected)| {
            format!(
                "{what}\n  source:   {src:?}\n  cue fmt:  {expected:?}\n  cue-rs:   {:?}",
                formatted(src)
            )
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

#[test]
fn formatting_what_was_formatted_changes_nothing() {
    // The property that makes a divergence survivable: whatever shape this formatter
    // settles on has to be a shape it settles on again. A rule that normalises and then
    // re-normalises would rewrite a file on every run of `just fmt`.
    for (what, src, _) in CASES {
        let once = formatted(src);
        assert_eq!(formatted(&once), once, "{what}: not idempotent:\n{once}");
    }
}

/// Shapes upstream keeps and this formatter normalises, and the bound on doing so.
///
/// A composite with elements on both sides of a newline is one shape too many for the
/// single bit recorded at the opening delimiter. It collapses to whichever side the
/// first element fell on — and, crucially, collapses to something `cue fmt` then leaves
/// alone, so a file formatted by either tool is a fixed point of both. That is what the
/// second column pins: not what was written, but what upstream accepts back.
const NORMALISED: &[(&str, &str, &str)] = &[
    (
        "elements after the first on new lines join the first",
        "package p\n\na: {b: 1,\n\tc: 2}\n",
        "package p\n\na: {b: 1, c: 2}\n",
    ),
    (
        "a list opened on its own line expands the rest of the way",
        "package p\n\na: [\n\t1, 2]\n",
        "package p\n\na: [\n\t1,\n\t2,\n]\n",
    ),
    (
        "and one opened on the element's line closes on it",
        "package p\n\na: [1,\n\t2]\n",
        "package p\n\na: [1, 2]\n",
    ),
];

#[test]
fn a_mixed_shape_settles_somewhere_upstream_accepts() {
    for (what, src, expected) in NORMALISED {
        assert_eq!(&formatted(src), expected, "{what}");
        assert_eq!(formatted(expected), *expected, "{what}: not idempotent");
    }
}

#[test]
fn parentheses_are_written_back_wherever_the_tree_needs_them() {
    // Not a layout rule. The parser keeps no parentheses, so `(a | b) & c` and
    // `a | b & c` arrive as different trees and printing the operands bare wrote the
    // second for both — moving `c` inside the disjunction. `(1 | 2) & 2` is `2`;
    // `1 | 2 & 2` is a disjunction with no default, which `cue export` refuses.
    for (source, expected) in [
        (
            "package p\n\np: (a | b) & c\n",
            "package p\n\np: (a | b) & c\n",
        ),
        (
            "package p\n\np: (a & b) | c\n",
            "package p\n\np: a & b | c\n",
        ),
        (
            "package p\n\np: a - (b - c)\n",
            "package p\n\np: a - (b - c)\n",
        ),
        (
            "package p\n\np: (a + b) * c\n",
            "package p\n\np: (a + b) * c\n",
        ),
        ("package p\n\np: -(a + b)\n", "package p\n\np: -(a + b)\n"),
    ] {
        assert_eq!(formatted(source), expected, "source: {source:?}");
    }
}

#[test]
fn an_open_list_is_still_open_after_a_format() {
    // `[...]` names no element type, which the parser stored as no ellipsis at all — the
    // same thing it stores for `[]`. A format then wrote `tools?: [...]` back as
    // `tools?: []`, turning a list open to anything into one closed at nothing.
    for source in [
        "package p\n\nx: [...]\n",
        "package p\n\nx: [...int]\n",
        "package p\n\nx: [1, ...]\n",
        "package p\n\nx: []\n",
    ] {
        assert_eq!(formatted(source), source);
    }
}

#[test]
fn a_wrapped_disjunction_keeps_its_lines() {
    // Seven named constants is the CUE idiom for an enum, and one line of it is 150
    // characters, so the union is written wrapped. The break is the author's.
    let source = "package p\n\n#Mode: #E.Nano | #E.Vim |\n\t#E.Helix | *#E.Nano\n";
    assert_eq!(formatted(source), source);
    assert_eq!(
        formatted("package p\n\n#Mode: #E.Nano | #E.Vim | *#E.Nano\n"),
        "package p\n\n#Mode: #E.Nano | #E.Vim | *#E.Nano\n"
    );
}
