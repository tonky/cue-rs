//! A comprehension over a struct yields its fields in upstream's arc order:
//! the order the evaluator creates them. A literal's static fields come where
//! they are declared, before the fields of references, definitions and
//! selectors beside it (`#D & {y: 1, z: 2}` yields `y, z`); dynamic labels,
//! what comprehension bodies compute and disjunctions follow, in that order.
//! Decoded JSON and YAML objects keep the document's key order.
//!
//! Every fixture lists the yields as `k*: [for k, v in x {k}]`, so comparing
//! with the golden compares the order. The goldens under
//! `fixtures/struct_field_order` come from the official `cue` binary;
//! regenerate them with `regen.py` there.

use std::path::{Path, PathBuf};

use cue_eval::eval_to_json;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/struct_field_order")
}

/// Fixtures cue-rs still orders differently, each with the reason. The test
/// asserts they *still* differ, so a fix fails here and forces its entry out.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[(
    "comprehension_labels_unified_with_reference.cue",
    "a literal's comprehension-computed fields come before the fields of a reference it is \
     unified with; upstream adds them after (`{for k in [\"c\"] {(k): 1}} & S` yields S's \
     fields first)",
)];

#[test]
fn struct_field_order_matches_cue_export() {
    let mut cue_files: Vec<_> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cue"))
        .collect();
    cue_files.sort();
    assert!(cue_files.len() >= 8, "fixtures missing");

    let mut failures = Vec::new();
    for path in &cue_files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(path).unwrap();
        let actual = eval_to_json(&source).map_err(|error| error.to_string());
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(path.with_extension("json")).unwrap())
                .unwrap();
        let agrees = actual.as_ref().is_ok_and(|actual| *actual == expected);
        if let Some((_, reason)) = KNOWN_DIVERGENCES.iter().find(|(known, _)| *known == name) {
            if agrees {
                failures.push(format!(
                    "{name} now agrees with cue: remove it from KNOWN_DIVERGENCES ({reason})"
                ));
            }
            continue;
        }
        if !agrees {
            let actual = match actual {
                Ok(actual) => actual.to_string(),
                Err(error) => format!("error {error}"),
            };
            failures.push(format!("{name}: expected {expected}, got {actual}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Across the files of a package, fields come file by file in filename order,
/// as `cue export .` yields them.
#[test]
fn package_files_contribute_fields_in_file_order() {
    let (evaluator, root) = cue_eval::PackageLoader::load_dir(fixtures().join("package")).unwrap();
    let json = evaluator.to_json(root).unwrap();
    assert_eq!(json["order"], serde_json::json!(["mid", "zeta", "alpha"]));
    assert_eq!(
        json["fields"],
        serde_json::json!(["name", "steps", "image"])
    );
    assert_eq!(json["zf"], serde_json::json!(["m", "n"]));
}
