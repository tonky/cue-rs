//! End-to-end protocol controls: no evaluator or oracle is trusted to declare
//! its own success; the runner compares their observations and owns verdicts.
#![cfg(unix)]
use cue_test_harness::TxtarArchive;
use cue_test_harness::conformance::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn executable(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn setup(
    dir: &Path,
    checks: serde_json::Value,
    observations: serde_json::Value,
) -> (Runner, PathBuf) {
    let input = "-- in.cue --\na: 1\n";
    let fixture = dir.join("case.txtar");
    fs::write(&fixture, input).unwrap();
    let sha: String = Sha256::digest(input)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let plan = json!({"revision":ORACLE_REVISION,"sha256":sha,"checks":checks});
    let response = json!({"observations":observations});
    let oracle = executable(dir, "oracle", &format!("cat <<'PLAN'\n{plan}\nPLAN"));
    let worker = executable(
        dir,
        "worker",
        &format!("cat <<'RESPONSE'\n{response}\nRESPONSE"),
    );
    (
        Runner {
            oracle,
            worker,
            timeout_seconds: 1,
            scratch: dir.join("scratch"),
            manifest: None,
        },
        fixture,
    )
}

fn check(
    id: &str,
    operation: &str,
    path: &[&str],
    expected: serde_json::Value,
) -> serde_json::Value {
    json!({"id":id,"file":"in.cue","path":path,"operation":operation,"expected":expected})
}

#[test]
fn wrong_value_missing_error_and_wrong_path_are_failures() {
    let dir = tempfile::tempdir().unwrap();
    let (runner, fixture) = setup(
        dir.path(),
        json!([
            check("value", "export", &["a"], json!(1)),
            check("error", "error_code", &["bad"], json!("eval")),
            check("path", "error_paths", &["bad"], json!(["bad.x"]))
        ]),
        json!({
            "value":{"status":"value","value":2},
            "error":{"status":"value","value":null},
            "path":{"status":"value","value":["sibling.x"]}
        }),
    );
    let case = runner.run(&fixture);
    assert!(!case.passed());
    assert_eq!(case.checks.len(), 3);
    assert!(case.checks.iter().all(|c| c.status == Status::Mismatch));
}

#[test]
fn unsupported_and_empty_checks_cannot_pass() {
    let dir = tempfile::tempdir().unwrap();
    let mut unsupported = check("unsupported", "partial", &[], json!(null));
    unsupported["status"] = json!("unsupported");
    unsupported["reason"] = json!("abstract adapter needed");
    let (runner, fixture) = setup(dir.path(), json!([unsupported]), json!({}));
    assert_eq!(runner.run(&fixture).checks[0].status, Status::Unsupported);
    let (runner, fixture) = setup(dir.path(), json!([]), json!({}));
    assert!(!runner.run(&fixture).passed());
}

#[test]
fn child_timeout_and_crash_are_distinct_from_semantic_failures() {
    let dir = tempfile::tempdir().unwrap();
    let (mut runner, fixture) = setup(
        dir.path(),
        json!([check("value", "export", &[], json!(1))]),
        json!({}),
    );
    runner.worker = executable(dir.path(), "slow", "sleep 30");
    assert_eq!(runner.run(&fixture).checks[0].status, Status::ResourceLimit);
    runner.worker = executable(dir.path(), "crash", "exit 3");
    assert_eq!(runner.run(&fixture).checks[0].status, Status::Crash);
}

#[test]
fn duplicate_unsafe_and_output_files_are_not_inputs() {
    assert!(TxtarArchive::parse("-- a.cue --\na: 1\n-- a.cue --\nb: 2\n").is_err());
    for path in ["../escape.cue", "/absolute.cue", "foo/../../escape.cue"] {
        assert!(TxtarArchive::parse(&format!("-- {path} --\na: 1\n")).is_err());
    }
    let archive=TxtarArchive::parse("-- in.cue --\na: 1\n-- out/expected.cue --\na: 2\n-- cue.mod/module.cue --\nmodule: \"x\"\n-- out/eval/stats --\nAllocs: 3\n").unwrap();
    assert_eq!(archive.cue_files(), vec![("in.cue", "a: 1\n")]);
    assert!(archive.expected_eval_output().is_none());
}

#[test]
fn invalid_fixture_and_stale_manifest_fail_before_execution() {
    let dir = tempfile::tempdir().unwrap();
    let (mut runner, fixture) = setup(
        dir.path(),
        json!([check("value", "export", &[], json!(1))]),
        json!({"value":{"status":"value","value":1}}),
    );
    assert!(runner.run(&fixture).passed());
    runner.manifest = Some(Manifest {
        oracle_revision: ORACLE_REVISION.into(),
        cases: vec![Provenance {
            case: "case.txtar".into(),
            sha256: "stale".into(),
            source_path: Some("cue/testdata/case.txtar".into()),
            source_revision: Some(ORACLE_REVISION.into()),
        }],
    });
    assert_eq!(
        runner.run(&fixture).checks[0].status,
        Status::OracleMismatch
    );
    runner.manifest = None;
    fs::write(&fixture, "-- ../escape.cue --\na: 1\n").unwrap();
    assert_eq!(
        runner.run(&fixture).checks[0].status,
        Status::InvalidFixture
    );
}

#[test]
fn baseline_success_does_not_imply_conformance() {
    let report = Report {
        oracle_revision: ORACLE_REVISION.into(),
        cases: vec![CaseResult {
            case: "case".into(),
            sha256: "hash".into(),
            checks: vec![CheckResult {
                id: "unimplemented".into(),
                operation: "partial".into(),
                path: vec![],
                status: Status::Unsupported,
                detail: "not yet".into(),
            }],
        }],
    };
    assert!(report.compare_baseline(&report).is_ok());
    assert!(!report.accepted());
    let mut changed: Report =
        serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
    changed.cases[0].checks[0].status = Status::Mismatch;
    assert!(changed.compare_baseline(&report).is_err());
    let empty = Report {
        oracle_revision: ORACLE_REVISION.into(),
        cases: vec![],
    };
    assert!(!empty.accepted());
}

#[test]
fn decimal_comparison_is_exact_and_ignores_numeric_spelling() {
    for spelling in ["2", "2.0", "0.02e2", "20e-1"] {
        assert_eq!(canonical_number(spelling).as_deref(), Some("2e0"));
    }
    assert_eq!(
        canonical_number("123e999999999999999999999").as_deref(),
        Some("123e999999999999999999999")
    );
    let a: serde_json::Value =
        serde_json::from_str("{\"x\": [2.0, 18446744073709551616]}").unwrap();
    let b: serde_json::Value = serde_json::from_str("{\"x\": [2, 18446744073709551616]}").unwrap();
    let rounded: serde_json::Value =
        serde_json::from_str("{\"x\": [2, 18446744073709551617]}").unwrap();
    assert!(json_values_equal(&a, &b));
    assert!(!json_values_equal(&a, &rounded));
    assert!(!json_values_equal(&json!(2), &json!("2")));
}

#[test]
fn matched_crashes_and_changed_observation_paths_cannot_satisfy_baseline() {
    let mut report = Report {
        oracle_revision: ORACLE_REVISION.into(),
        cases: vec![CaseResult {
            case: "case".into(),
            sha256: "hash".into(),
            checks: vec![CheckResult {
                id: "check".into(),
                operation: "error_code".into(),
                path: vec![Selector::Field("bad".into())],
                status: Status::Crash,
                detail: "crash".into(),
            }],
        }],
    };
    assert!(report.compare_baseline(&report).is_err());
    report.cases[0].checks[0].status = Status::Passed;
    let mut changed: Report =
        serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
    changed.cases[0].checks[0].path = vec![Selector::Field("sibling".into())];
    assert!(changed.compare_baseline(&report).is_err());
}
