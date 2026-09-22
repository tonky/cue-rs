//! What CUE's four string spellings mean, and that formatting preserves it.
//!
//! Every value below was confirmed against upstream `cue export` v0.16.1 on 2026-09-22.
//! They are here because cue-rs used to store the raw source slice for a `"""` literal
//! and unescape a `#"…"#` one, so a `files:` entry in an enve project evaluated with the
//! surrounding indentation baked in and a regex lost its backslashes. A literal now
//! carries the form it was written in, which is what both the evaluation and the
//! formatter need.

use cue_syntax::{Decl, Expr, FieldDecl, Label, SourceFile, StringForm, StringLit};

/// Each spelling, the value it denotes, and why that case is here.
const MEANING: &[(&str, &str)] = &[
    // an escape in a quoted literal is resolved
    ("\"a\\tb\"", "a\tb"),
    // a bare escape in a raw literal is text
    ("#\"a\\nb\"#", "a\\nb"),
    // a guarded escape in a raw literal is resolved
    ("#\"a\\#nb\"#", "a\nb"),
    // which is what makes a Windows path readable
    ("#\"C:\\new\\table\"#", "C:\\new\\table"),
    // two guards let the text hold one
    ("##\"say \"hi\"#\"##", "say \"hi\"#"),
    // a quote is escaped in the quoted form
    ("\"he said \\\"hi\\\"\"", "he said \"hi\""),
    // an interpolation is evaluated
    ("\"x\\(1+1)y\"", "x2y"),
    // a raw literal interpolates under its guard
    ("#\"a\\#(1+1)b\"#", "a2b"),
    // an escaped backslash does not open one
    ("\"a\\\\(b\"", "a\\(b"),
    // a backslash directly before one still does not
    ("\"sep: \\\\\\(1)\"", "sep: \\1"),
    // a block literal keeps its relative indent and loses the rest
    ("\"\"\"\n\tone\n\t  two\n\t\"\"\"", "one\n  two"),
    // a block literal interpolates
    ("\"\"\"\n\tx\\(1+1)y\n\t\"\"\"", "x2y"),
    // a tab inside a block literal is a tab
    ("\"\"\"\n\ttab\there\n\t\"\"\"", "tab\there"),
    // a trailing blank line is part of the value
    ("\"\"\"\n\tone\n\n\t\"\"\"", "one\n"),
    // a block literal can be empty
    ("\"\"\"\n\n\t\"\"\"", ""),
    // a guarded block literal is still a block literal
    ("##\"\"\"\n\tguarded\n\t\"\"\"##", "guarded"),
    // the bell, which cue-rs did not implement at all
    ("\"A\\aB\"", "AB"),
    // nor the vertical tab
    ("\"A\\vB\"", "AB"),
    // an escaped slash is a slash
    ("\"A\\/B\"", "A/B"),
    // a four-digit code point
    ("\"A\\u0041B\"", "AAB"),
    // and an eight-digit one
    ("\"A\\U0001F600B\"", "A😀B"),
    // which a raw literal spells under its guard
    ("#\"A\\#u0041B\"#", "AAB"),
    // the delimiter's own quote
    ("\"A\\\"B\"", "A\"B"),
];

fn value_of(literal: &str) -> serde_json::Value {
    let source = format!("a: {literal}");
    cue_eval::eval_to_json(&source)
        .unwrap_or_else(|e| panic!("{literal} did not evaluate: {e}"))["a"]
        .clone()
}

fn formatted(source: &str) -> String {
    let file =
        cue_syntax::parse_file(source).unwrap_or_else(|e| panic!("{source} did not parse: {e}"));
    cue_syntax::format_file(&file)
}

#[test]
fn every_spelling_denotes_what_cue_says_it_denotes() {
    for (literal, expected) in MEANING {
        assert_eq!(
            value_of(literal),
            serde_json::Value::String((*expected).to_string()),
            "{literal}"
        );
    }
}

#[test]
fn formatting_keeps_the_value_and_settles_in_one_pass() {
    for (literal, expected) in MEANING {
        let source = format!("a: {literal}\n");
        let once = formatted(&source);
        assert_eq!(
            cue_eval::eval_to_json(&once).unwrap()["a"],
            serde_json::Value::String((*expected).to_string()),
            "formatting changed the value of {literal}:\n{once}"
        );
        assert_eq!(
            formatted(&once),
            once,
            "formatting {literal} is not idempotent:\n{once}"
        );
    }
}

#[test]
fn a_block_literal_is_written_back_as_a_block_literal() {
    // The regression enve saw: a service's config file, read out of a `files:` entry.
    // Flattening it to `"listen_addresses = '*'\nport = 5432"` is the same value and an
    // unreadable diff, so the form has to survive the round trip — indented under the
    // field it belongs to, however deeply that field is nested.
    // The path is written as a path, as the author wrote it, so the literal indents one
    // level from the line the field sits on — not from the braces the formatter used to
    // invent for it. Both spellings below are byte-identical to `cue fmt` v0.16.1.
    for (source, indent) in [
        (
            concat!(
                "services: postgres: files: \"postgresql.conf\": \"\"\"\n",
                "\tlisten_addresses = '*'\n",
                "\tport = 5432\n",
                "\t\"\"\"\n"
            ),
            "\t",
        ),
        (
            concat!(
                "services: {\n",
                "\tpostgres: files: \"postgresql.conf\": \"\"\"\n",
                "\t\tlisten_addresses = '*'\n",
                "\t\tport = 5432\n",
                "\t\t\"\"\"\n",
                "}\n"
            ),
            "\t\t",
        ),
    ] {
        let once = formatted(source);
        assert!(
            once.contains(&format!(
                "\"postgresql.conf\": \"\"\"\n\
                 {indent}listen_addresses = '*'\n\
                 {indent}port = 5432\n\
                 {indent}\"\"\"\n"
            )),
            "a block literal must stay one, indented under its field:\n{once}"
        );
        assert_eq!(
            formatted(&once),
            once,
            "formatting the block literal is not idempotent:\n{once}"
        );
    }
    let once = formatted(concat!(
        "services: postgres: files: \"postgresql.conf\": \"\"\"\n",
        "\tlisten_addresses = '*'\n",
        "\tport = 5432\n",
        "\t\"\"\"\n"
    ));
    assert_eq!(
        cue_eval::eval_to_json(&once).unwrap()["services"]["postgres"]["files"]["postgresql.conf"],
        serde_json::Value::String("listen_addresses = '*'\nport = 5432".to_string()),
        "{once}"
    );
    assert_eq!(formatted(&once), once);
}

#[test]
fn a_form_that_cannot_hold_its_value_falls_back_to_quoting() {
    // Nothing in a source file produces these — a raw literal holding its own closing
    // delimiter cannot be written raw in the first place. A folder rewriting values can,
    // and the formatter's job is to emit a file that reparses to the same value, not to
    // insist on the spelling it was handed.
    for form in [StringForm::Raw { hashes: 1 }, StringForm::Block] {
        for value in ["a\"#b", "a\"\"\"b", "a\\#(b"] {
            let file = SourceFile {
                header: Vec::new(),
                package: None,
                imports: Vec::new(),
                decls: vec![Decl::Field(FieldDecl {
                    label: Label::Ident("a".to_string()),
                    optional: false,
                    value: Expr::String(StringLit {
                        value: value.to_string(),
                        form,
                    }),
                    attrs: Vec::new(),
                })],
            };
            let printed = cue_syntax::format_file(&file);
            assert_eq!(
                cue_eval::eval_to_json(&printed)
                    .unwrap_or_else(|e| panic!("{form:?} {value:?} printed {printed}: {e}"))["a"],
                serde_json::Value::String(value.to_string()),
                "{form:?} {value:?} printed:\n{printed}"
            );
        }
    }
}

/// Input CUE refuses, the message that must name why, and why the case is here. Every one
/// of these was confirmed rejected by upstream `cue` v0.16.1 on 2026-09-22 — the messages
/// differ in wording, the verdicts do not.
const REJECTED: &[(&str, &str)] = &[
    // an escape that is not one
    ("v: \"a\\qb\"", "'\\q'"),
    // CUE has no `\0`: a digit opens an octal escape
    ("v: \"a\\0b\"", "'\\0'"),
    // and `\xHH` is a bytes escape, not a string one
    ("v: \"a\\x41b\"", "'\\x'"),
    // as is octal
    ("v: \"a\\101b\"", "'\\1'"),
    // a guarded escape is still checked
    ("v: #\"a\\#qb\"#", "'\\#q'"),
    // and so is one inside a block literal
    ("v: \"\"\"\\n\\ta\\qb\\n\\t\"\"\"", "'\\q'"),
    // a code point needs its digits
    ("v: \"a\\u00ZZb\"", "'\\u00ZZ'"),
    // and has to name a real one
    ("v: \"a\\UFFFFFFFFb\"", "'\\UFFFFFFFF'"),
    // a single-line literal stops at its line
    ("v: \"a\nb\"", "not terminated"),
];

#[test]
fn an_escape_cue_does_not_have_is_refused_by_name() {
    for (source, expected) in REJECTED {
        let error = cue_eval::eval_to_json(source)
            .err()
            .unwrap_or_else(|| panic!("{source:?} was accepted"))
            .to_string();
        assert!(
            error.contains(expected),
            "{source:?} was refused, but not for {expected}: {error}"
        );
    }
}

#[test]
fn a_refusal_points_at_the_escape_it_refuses() {
    // Not merely "somewhere in this file". A block literal is the exception: its
    // indentation has been stripped by the time the escape is read, so the whole literal
    // is pointed at instead of a column inside it.
    let error = cue_eval::eval_to_json(r#"v: "a\qb""#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("5..7"), "{error}");
}

#[test]
fn a_bytes_literal_says_it_cannot_interpolate_rather_than_guessing() {
    // Upstream accepts `'a\(1)b'` and gives "a1b". cue-rs does not carry a delimiter on
    // `Expr::Interpolation`, so it cannot represent one — and before this it silently
    // produced the seven characters `a\(1)b`. A stated limit beats a wrong value.
    let error = cue_eval::eval_to_json(r"v: 'a\(1)b'")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Interpolation is not supported"), "{error}");
}
