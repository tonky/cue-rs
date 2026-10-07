//! Files whose every value is checked against the official `cue` binary, before and after
//! a format.
//!
//! The goldens beside each `.cue` are `cue export` output (v0.17.1). Evaluating the
//! formatted file as well pins what a format may not do: change a value. The parser keeps
//! no parentheses, so the formatter has to derive every pair the tree needs — and
//! `(#A & {raw: "a"}).out` written back as `#A & {raw: "a"}.out` evaluates to something
//! else, as does a parser that ranks `||` below `|`.
//!
//! Regenerate a golden with `cue export <file>.cue > <file>.json`.

use std::path::Path;

use cue_eval::eval_to_json;
use serde_json::Value;

fn check(fixture: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cue"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());

    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(path).unwrap();
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(path.with_extension("json")).unwrap())
                .unwrap();
        let formatted = cue_syntax::format_file(&cue_syntax::parse_file(&source).unwrap());
        for (what, text) in [("source", &source), ("formatted", &formatted)] {
            match eval_to_json(text) {
                Ok(actual) => {
                    let Value::Object(fields) = &expected else {
                        unreachable!("a golden is an object")
                    };
                    for (field, want) in fields {
                        if actual.get(field) != Some(want) {
                            failures.push(format!(
                                "{name} ({what}) {field}: cue says {want}, cue-rs {}",
                                actual.get(field).unwrap_or(&Value::Null)
                            ));
                        }
                    }
                    if actual.as_object().map(|o| o.len()) != Some(fields.len()) {
                        failures.push(format!("{name} ({what}): fields differ: {actual}"));
                    }
                }
                Err(error) => failures.push(format!("{name} ({what}): error {error}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// An empty separator splits between characters, as Go's `strings.Split` does — no empty
/// part before the first one or after the last — and every other builtin given an empty
/// argument answers as upstream's.
#[test]
fn strings_with_empty_arguments_match_cue_export() {
    check("strings_empty_separator");
}

/// `strings.TrimPrefixAny` and `TrimSuffixAny` are cue-rs's own; an empty element strips
/// nothing, and used to be taken for a change and looped forever.
#[test]
fn trimming_an_empty_affix_strips_nothing() {
    let json = eval_to_json(
        r#"import "strings"
p: strings.TrimPrefixAny("aab", ["", "a"])
s: strings.TrimSuffixAny("abb", ["", "b"])
"#,
    )
    .unwrap();
    assert_eq!(json["p"], "b");
    assert_eq!(json["s"], "a");
}
