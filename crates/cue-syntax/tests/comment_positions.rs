//! Where a comment survives a format, and where it does not.
//!
//! `enve cue fmt` rewrites a file in place, so every comment it loses is lost from
//! someone's source. The parser attaches comments to *declaration positions* — the gaps
//! between one declaration and the next, plus the gap above `package`. A comment anywhere
//! else has no declaration to hang on and is dropped.
//!
//! That limit is a contract, not an accident, so this file names every position and says
//! which side of the line it falls on. A position that moves from `Dropped` to `Kept` is a
//! feature; one that moves the other way is a regression, and either way the table has to
//! change first.

use cue_syntax::{format_file, parse_file};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fate {
    Kept,
    Dropped,
}

use Fate::{Dropped, Kept};

/// Every position a `// why` comment can occupy, and what a format does to it.
const POSITIONS: &[(&str, Fate, &str)] = &[
    (
        "above the package clause",
        Kept,
        "// why\n\npackage p\n\na: 1\n",
    ),
    (
        "above a top-level declaration",
        Kept,
        "package p\n\n// why\na: 1\n",
    ),
    (
        "above a declaration nested in a struct",
        Kept,
        "package p\n\na: {\n\t// why\n\tb: 1\n}\n",
    ),
    (
        "after the last declaration of a struct",
        Kept,
        "package p\n\na: {\n\tb: 1\n\t// why\n}\n",
    ),
    (
        "after the last declaration of the file",
        Kept,
        "package p\n\na: 1\n\n// why\n",
    ),
    (
        "above an import clause",
        Dropped,
        "package p\n\n// why\nimport \"strings\"\n\na: 1\n",
    ),
    (
        "inside a parenthesised import block",
        Dropped,
        "package p\n\nimport (\n\t// why\n\t\"strings\"\n)\n\na: 1\n",
    ),
    (
        "trailing, on the same line as a declaration",
        Dropped,
        "package p\n\na: 1 // why\n",
    ),
    (
        "between a field's name and its value",
        Dropped,
        "package p\n\na:\n\t// why\n\t1\n",
    ),
    (
        "inside a list literal",
        Dropped,
        "package p\n\na: [\n\t// why\n\t1,\n\t2,\n]\n",
    ),
    (
        "inside an expression",
        Dropped,
        "package p\n\na: 1 +\n\t// why\n\t2\n",
    ),
];

fn formatted(src: &str) -> String {
    let file = parse_file(src).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e:?}"));
    format_file(&file)
}

#[test]
fn every_comment_position_has_a_stated_fate() {
    let wrong: Vec<_> = POSITIONS
        .iter()
        .filter_map(|(position, fate, src)| {
            let out = formatted(src);
            let actual = if out.contains("// why") {
                Kept
            } else {
                Dropped
            };
            (actual != *fate).then(|| format!("{position}: expected {fate:?}, got {actual:?}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "comment fate changed:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn a_dropped_comment_costs_nothing_but_the_comment() {
    // Losing a comment must not disturb the declarations around it: the output of a
    // dropped-comment case has to equal the output of the same source with the comment
    // deleted by hand.
    for (position, fate, src) in POSITIONS.iter().filter(|(_, f, _)| *f == Dropped) {
        let without: String = src
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .map(|line| line.split(" // ").next().unwrap_or(line).trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            formatted(src),
            formatted(&format!("{without}\n")),
            "{position}: dropping the comment changed the surrounding declarations ({fate:?})"
        );
    }
}

#[test]
fn a_kept_comment_survives_a_second_format() {
    // Idempotence: whatever the first pass writes, the second pass has to leave alone.
    for (position, _, src) in POSITIONS.iter().filter(|(_, f, _)| *f == Kept) {
        let once = formatted(src);
        assert_eq!(
            formatted(&once),
            once,
            "{position}: format is not idempotent"
        );
    }
}

#[test]
fn a_run_of_blank_lines_collapses_to_one() {
    let out = formatted("package p\n\n\n\n\na: 1\n\n\n\nb: 2\n");
    assert_eq!(out, "package p\n\na: 1\n\nb: 2\n");
}

#[test]
fn blank_lines_do_not_accumulate_at_the_edges() {
    // Leading and trailing blank lines inside a struct are trimmed, so repeated formats
    // cannot grow the file.
    let out = formatted("package p\n\na: {\n\n\tb: 1\n\n}\n");
    assert_eq!(out, "package p\n\na: {\n\tb: 1\n}\n");
}

#[test]
fn a_comment_inside_a_string_is_not_a_comment() {
    // `//` belongs to whichever token contains it. A URL in a value must survive.
    let src = "package p\n\nhome: \"https://example.com/docs\"\n";
    assert_eq!(formatted(src), src);
}

#[test]
fn the_shape_of_a_comment_is_preserved_verbatim() {
    // Everything after the two slashes is carried through untouched: spacing, a second
    // pair of slashes, a doc-comment marker, an empty comment.
    let src = concat!(
        "package p\n",
        "\n",
        "//\n",
        "//   indented   \n",
        "//// four slashes\n",
        "//!bang\n",
        "a: 1\n",
    );
    assert_eq!(formatted(src), src);
}
