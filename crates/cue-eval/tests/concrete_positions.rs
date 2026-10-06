//! Positions that need a concrete value must behave exactly like `cue export`.
//!
//! Comprehension guards and sources, dynamic labels, interpolations, operator
//! operands, builtin arguments and index/slice/select targets resolve a
//! disjunction to its default, and fail (never silently skip) when the value
//! is not concrete. The goldens under `fixtures/concrete_positions` come from
//! the official `cue` binary; regenerate them with `regen.py` there.

use std::path::{Path, PathBuf};

use cue_eval::eval_to_json;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/concrete_positions")
}

/// Fixtures cue-rs still evaluates differently, each with the reason. The test
/// asserts they *still* differ, so a fix fails here and forces its entry out.
/// Each one fails loudly rather than giving a wrong value.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    (
        "alias_field.cue",
        "an alias on a quoted label (`X=\"a-b\": 1`) is refused: a quoted label declares no identifier, and the alias would need one",
    ),
    (
        "alias_quoted_nested.cue",
        "an alias on a quoted label is refused",
    ),
    (
        "alias_quoted_top.cue",
        "an alias on a quoted label is refused",
    ),
    (
        "alias_shadow.cue",
        "upstream rejects an alias named like a field of an enclosing scope; cue-rs lets the alias shadow it",
    ),
    (
        "alias_value.cue",
        "value aliases (`a: V={...}`) do not parse",
    ),
];

/// Every `<name>.cue` exports to `<name>.json`, or fails when cue failed
/// (`<name>.err` holds cue's diagnostic).
#[test]
fn concrete_positions_match_cue_export() {
    let mut cue_files: Vec<_> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cue"))
        .collect();
    cue_files.sort();
    assert!(cue_files.len() > 40, "fixtures missing");

    let mut failures = Vec::new();
    for path in &cue_files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(path).unwrap();
        let actual = eval_to_json(&source).map_err(|error| error.to_string());
        let golden = path.with_extension("json");
        let cue_error = path.with_extension("err");
        if let Some((_, reason)) = KNOWN_DIVERGENCES.iter().find(|(known, _)| *known == name) {
            let agrees = match &actual {
                Ok(actual) => {
                    golden.exists()
                        && *actual
                            == serde_json::from_str::<Value>(
                                &std::fs::read_to_string(&golden).unwrap(),
                            )
                            .unwrap()
                }
                Err(_) => !golden.exists(),
            };
            if agrees {
                failures.push(format!(
                    "{name} now agrees with cue: remove it from KNOWN_DIVERGENCES ({reason})"
                ));
            }
            continue;
        }
        match (golden.exists(), actual) {
            (true, Ok(actual)) => {
                let expected: Value =
                    serde_json::from_str(&std::fs::read_to_string(&golden).unwrap()).unwrap();
                if actual != expected {
                    failures.push(format!("{name}: expected {expected}, got {actual}"));
                }
            }
            (true, Err(error)) => {
                failures.push(format!("{name}: cue exports, cue-rs fails: {error}"))
            }
            (false, Ok(actual)) => {
                let cue = std::fs::read_to_string(&cue_error).unwrap_or_default();
                failures.push(format!(
                    "{name}: cue fails ({}), cue-rs exports {actual}",
                    cue.lines().next().unwrap_or("no golden")
                ));
            }
            (false, Err(_)) => assert!(cue_error.exists(), "{name}: no golden"),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every comparison operator over every value kind, each evaluated on its own
/// as `v: (lhs) op (rhs)`: the result equals cue's, and whatever cue rejects
/// (unsupported operands, incomplete or bottom values) cue-rs rejects too.
#[test]
fn comparisons_match_cue_export() {
    let golden = std::fs::read_to_string(fixtures().join("comparisons.json")).unwrap();
    let entries: Vec<Value> = serde_json::from_str(&golden).unwrap();
    let mut failures = Vec::new();
    for entry in &entries {
        let expression = format!(
            "({}) {} ({})",
            entry["lhs"].as_str().unwrap(),
            entry["op"].as_str().unwrap(),
            entry["rhs"].as_str().unwrap()
        );
        let actual = eval_to_json(&format!("v: {expression}\n")).map(|json| json["v"].clone());
        match (entry.get("value"), actual) {
            (Some(expected), Ok(actual)) if *expected == actual => {}
            (Some(expected), Ok(actual)) => {
                failures.push(format!("{expression}: expected {expected}, got {actual}"))
            }
            (Some(expected), Err(error)) => failures.push(format!(
                "{expression}: expected {expected}, got error {error}"
            )),
            (None, Ok(actual)) => failures.push(format!(
                "{expression}: cue fails ({}), cue-rs gives {actual}",
                entry["error"].as_str().unwrap()
            )),
            (None, Err(_)) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} comparisons differ from cue:\n{}",
        failures.len(),
        entries.len(),
        failures.join("\n")
    );
}
