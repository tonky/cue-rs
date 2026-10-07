//! Golden fixtures produced by the official `cue export` (each directory has a
//! `regen.py`), compared with cue-rs.

use std::path::{Path, PathBuf};

use cue_eval::{EvalError, eval_to_json};
use serde_json::Value;

fn fixtures(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
}

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
            // An incomplete value names its path as `at 'x.name'`, and an
            // undecided struct as `incomplete value at 'x': <reason>`.
            !golden.exists()
                && (path.starts_with('#')
                    || message.starts_with(&format!("{path}: "))
                    || message.contains(&format!(" at '{path}' "))
                    || message.contains(&format!(" at '{path}': ")))
        }
        Err(_) => false,
    }
}

/// Every `<name>.cue` exports to `<name>.json`; when cue fails (`<name>.err`
/// holds its diagnostic) cue-rs fails too, naming the same path. Each entry of
/// `known_divergences` (file name, reason) must still differ, so a fix fails
/// here and forces its entry out.
pub fn match_cue_export(dir: &str, min_fixtures: usize, known_divergences: &[(&str, &str)]) {
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
        if let Some((_, reason)) = known_divergences.iter().find(|(known, _)| *known == name) {
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
