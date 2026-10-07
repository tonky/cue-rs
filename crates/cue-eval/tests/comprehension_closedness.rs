//! Closed definitions whose fields come from comprehensions, and the path an
//! error names, must behave exactly like `cue export`.
//!
//! A field an `if` or `for` in a definition's own body adds is part of that
//! definition, also when the guard reads a field the definition is unified
//! with later, and in embedded, nested, listed, pattern-matched and disjoined
//! definitions. A field the definition never adds stays refused. An error
//! names the full path of the field that became bottom (`x.o.1.t: ...`), as
//! cue does. The goldens under `fixtures/comprehension_closedness` and
//! `fixtures/error_paths` come from the official `cue` binary; regenerate them
//! with `regen.py` in each.

use std::path::{Path, PathBuf};

use cue_eval::{EvalError, eval_to_json};
use serde_json::Value;

fn fixtures(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
}

/// Fixtures cue-rs still evaluates differently, each with the reason. The test
/// asserts they *still* differ, so a fix fails here and forces its entry out.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    (
        "sibling_read_no_def.cue",
        "a comprehension-generated field reading a sibling of the enclosing literal keeps the value from before the merge when the guard decides the same way (`x: 0`, cue `x: 1`)",
    ),
    (
        "sibling_read_in_def.cue",
        "the same stale sibling read inside a definition (`create postgres`, cue `create app`)",
    ),
    (
        "conflict_after_incomplete.cue",
        "cue reports the conflict at `t.a.b` before the incomplete `s.a.b`; cue-rs reports the first field it exports",
    ),
];

/// The path cue's diagnostic names: its first line up to the first `: `.
fn cue_error_path(diagnostic: &str) -> &str {
    let line = diagnostic.lines().next().unwrap_or_default();
    line.split_once(": ").map_or(line, |(path, _)| path)
}

fn agrees(actual: &Result<Value, EvalError>, golden: &Path, cue_error: &Path) -> bool {
    match actual {
        Ok(actual) => {
            golden.exists()
                && *actual
                    == serde_json::from_str::<Value>(&std::fs::read_to_string(golden).unwrap())
                        .unwrap()
        }
        Err(EvalError::Evaluation(message)) => {
            let cue = std::fs::read_to_string(cue_error).unwrap_or_default();
            let path = cue_error_path(&cue);
            // cue checks a definition on its own and reports its error there
            // (`#P.b: ...`); cue-rs reports it where the definition is used.
            // An incomplete value names its path as `at 'x.name'`.
            !golden.exists()
                && (path.starts_with('#')
                    || message.starts_with(&format!("{path}: "))
                    || message.contains(&format!(" at '{path}' ")))
        }
        Err(_) => false,
    }
}

/// Every `<name>.cue` exports to `<name>.json`; when cue fails (`<name>.err`
/// holds its diagnostic) cue-rs fails too, naming the same path.
fn match_cue_export(dir: &str, min_fixtures: usize) {
    let mut cue_files: Vec<_> = std::fs::read_dir(fixtures(dir))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cue"))
        .collect();
    cue_files.sort();
    assert!(cue_files.len() >= min_fixtures, "{dir}: fixtures missing");

    let mut failures = Vec::new();
    for path in &cue_files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(path).unwrap();
        let actual = eval_to_json(&source);
        let golden = path.with_extension("json");
        let cue_error = path.with_extension("err");
        assert!(
            golden.exists() || cue_error.exists(),
            "{dir}/{name}: no golden"
        );
        let agrees = agrees(&actual, &golden, &cue_error);
        if let Some((_, reason)) = KNOWN_DIVERGENCES.iter().find(|(known, _)| *known == name) {
            if agrees {
                failures.push(format!(
                    "{name} now agrees with cue: remove it from KNOWN_DIVERGENCES ({reason})"
                ));
            }
            continue;
        }
        if agrees {
            continue;
        }
        let cue = match std::fs::read_to_string(&golden) {
            Ok(json) => json.split_whitespace().collect::<Vec<_>>().join(" "),
            Err(_) => std::fs::read_to_string(&cue_error)
                .unwrap()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
        };
        let actual = match actual {
            Ok(json) => json.to_string(),
            Err(error) => error.to_string(),
        };
        failures.push(format!("{name}:\n  cue:    {cue}\n  cue-rs: {actual}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn comprehension_closedness_matches_cue_export() {
    match_cue_export("comprehension_closedness", 60);
}

#[test]
fn error_paths_match_cue_export() {
    match_cue_export("error_paths", 16);
}
