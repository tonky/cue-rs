//! A comprehension is an element of the list literal that holds it, even when
//! it is the only element: `[[for k in s {k}]]` is a list holding one list,
//! and the yields are never spliced into an enclosing list. Covers any nesting
//! depth, comprehensions mixed with plain elements, `if` guards, nested `for`
//! and `let` clauses, struct and list yields, and `list.Concat` over them.
//!
//! The goldens under `fixtures/list_comprehension_nesting` come from the
//! official `cue` binary; regenerate them with `regen.py` there.

use std::path::{Path, PathBuf};

use cue_eval::eval_to_json;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/list_comprehension_nesting")
}

fn cue_files() -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cue"))
        .collect();
    files.sort();
    assert!(files.len() >= 5, "fixtures missing");
    files
}

/// Compares one source against the golden of `path`: `<name>.json` when cue
/// exports, or an error when cue fails (`<name>.err`).
fn check(path: &Path, source: &str, what: &str, failures: &mut Vec<String>) {
    let name = path.file_name().unwrap().to_string_lossy();
    let actual = eval_to_json(source).map_err(|error| error.to_string());
    let golden = path.with_extension("json");
    match (golden.exists(), actual) {
        (true, Ok(actual)) => {
            let expected: Value =
                serde_json::from_str(&std::fs::read_to_string(&golden).unwrap()).unwrap();
            if actual != expected {
                failures.push(format!(
                    "{name} ({what}): expected {expected}, got {actual}"
                ));
            }
        }
        (true, Err(error)) => failures.push(format!(
            "{name} ({what}): cue exports, cue-rs fails: {error}"
        )),
        (false, Ok(actual)) => failures.push(format!(
            "{name} ({what}): cue fails, cue-rs exports {actual}"
        )),
        (false, Err(_)) => assert!(path.with_extension("err").exists(), "{name}: no golden"),
    }
}

#[test]
fn nested_list_comprehensions_match_cue_export() {
    let mut failures = Vec::new();
    for path in cue_files() {
        let source = std::fs::read_to_string(&path).unwrap();
        check(&path, &source, "source", &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The formatter writes a comprehension element without brackets of its own:
/// `[1, for k in s {k}]` must not come back as `[1, [for k in s {k}]]`.
#[test]
fn formatted_list_comprehensions_keep_their_meaning() {
    let mut failures = Vec::new();
    for path in cue_files() {
        let source = std::fs::read_to_string(&path).unwrap();
        let formatted = cue_syntax::format_file(&cue_syntax::parse_file(&source).unwrap());
        check(&path, &formatted, "formatted", &mut failures);
        let again = cue_syntax::format_file(&cue_syntax::parse_file(&formatted).unwrap());
        if again != formatted {
            failures.push(format!(
                "{}: formatting is not stable:\n{formatted}\n---\n{again}",
                path.display()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
