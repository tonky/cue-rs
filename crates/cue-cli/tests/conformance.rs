use serde_json::Value;
use std::fs;
use std::process::Command;

fn oracle_case(source: &str) -> (bool, Value) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("case.txtar"), source).unwrap();
    let report = dir.path().join("report.json");
    let oracle = std::env::var_os("CUE_CONFORMANCE_ORACLE")
        .expect("set CUE_CONFORMANCE_ORACLE to the pinned adapter executable");
    let output = Command::new(env!("CARGO_BIN_EXE_cue-rs"))
        .args(["conformance"])
        .arg(dir.path())
        .arg("--oracle")
        .arg(oracle)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    let result = fs::read(&report)
        .unwrap_or_else(|_| panic!("no report: {}", String::from_utf8_lossy(&output.stderr)));
    (
        output.status.success(),
        serde_json::from_slice(&result).unwrap(),
    )
}

#[test]
fn legacy_directory_failure_and_empty_directory_exit_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_cue-rs"))
            .arg("test-txtar")
            .arg(dir.path())
            .arg("--strict-errors")
            .output()
            .unwrap()
    };
    assert!(!run().status.success());
    fs::write(dir.path().join("bad.txtar"), "-- in.cue --\na: 1 + true\n").unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 failed"));
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn real_oracle_checks_multifile_values_and_selected_errors() {
    let (ok, report) = oracle_case(
        "-- a.cue --\npackage test\na: b+1 @test(eq, 3)\n-- b.cue --\npackage test\nb: 2\n",
    );
    assert!(ok, "{report}");
    let (ok, report) = oracle_case("-- in.cue --\nbad: 1 & 2 @test(err, code=eval)\ngood: 1\n");
    assert!(ok, "{report}");
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn real_oracle_cannot_pass_wrong_value_wrong_path_or_unsupported_assertion() {
    let (ok, report) = oracle_case("-- in.cue --\na: 18446744073709551617\n");
    assert!(ok, "{report}");
    let (ok, report) = oracle_case("-- in.cue --\na: 1 @test(eq, 2)\n");
    assert!(!ok);
    assert!(report.to_string().contains("oracle_mismatch"));
    let (ok, report) = oracle_case("-- in.cue --\nbad: 1 & 2\ngood: 1 @test(err, code=eval)\n");
    assert!(!ok);
    assert!(report.to_string().contains("oracle_mismatch"));
    let (ok, report) = oracle_case("-- in.cue --\na: int @test(eq, int)\n");
    assert!(!ok);
    assert!(report.to_string().contains("unsupported"));
}

#[test]
fn a_report_cannot_overwrite_its_own_baseline() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("case.txtar"), "-- in.cue --\na: 1\n").unwrap();
    let baseline = dir.path().join("baseline.json");
    fs::write(&baseline, "preserve this baseline").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cue-rs"))
        .arg("conformance")
        .arg(dir.path())
        .arg("--baseline")
        .arg(&baseline)
        .arg("--report")
        .arg(&baseline)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not overwrite"));
    assert_eq!(
        fs::read_to_string(baseline).unwrap(),
        "preserve this baseline"
    );
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn concrete_export_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/export/concrete.txtar"),
        include_str!("../../../tests/conformance/regressions/export/ambiguous.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

#[test]
fn cli_yaml_exports_exact_numeric_scalars_and_base64_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("in.cue");
    fs::write(&file, "a: 18446744073709551617\nb: '\\xff'\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cue-rs"))
        .arg("eval")
        .arg(file)
        .args(["--format", "yaml"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "a: 18446744073709551617\nb: /w==\n"
    );
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn scalar_operator_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/scalars/defaults.txtar"),
        include_str!("../../../tests/conformance/regressions/scalars/numbers.txtar"),
        include_str!("../../../tests/conformance/regressions/scalars/errors.txtar"),
        include_str!("../../../tests/conformance/regressions/scalars/signs.txtar"),
        include_str!("../../../tests/conformance/regressions/scalars/incomplete.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn plain_embedding_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/embeddings/values.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/bounds.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/errors.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/constraints.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/package.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/forward.txtar"),
        include_str!("../../../tests/conformance/regressions/embeddings/comprehensions.txtar"),
        "-- in.cue --\n\"hello\"\n",
        "-- in.cue --\n'hello\\nworld'\n",
        "-- in.cue --\n[1,2]\n",
        "-- in.cue --\n*1 | int\n",
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn interpolation_merge_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/interpolation/merge.txtar"),
        include_str!("../../../tests/conformance/regressions/interpolation/incomplete.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn builtin_argument_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/builtins/merge.txtar"),
        include_str!("../../../tests/conformance/regressions/builtins/incomplete.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn scalar_metadata_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/metadata/values.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/merge.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/recipes.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/choices.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/scopes.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/errors.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/closed-choice.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/package.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/mixed-choice.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/mixed-neighbors.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/mixed-views.txtar"),
        include_str!("../../../tests/conformance/regressions/metadata/mixed-recipes.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/basic.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/neighbors.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/embedding.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/provenance.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/required.txtar"),
        include_str!("../../../tests/conformance/regressions/dynamic-choice/branch-fields.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        assert!(ok, "{report}");
    }
}

/// Builtins over arguments declared further down, forward lets, chains of a
/// hundred operands and repeated declarations beside a pattern, and the
/// issue3851 closedness errors. An export mismatch leaves the exit status
/// alone, so every check must have passed.
#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn relaxation_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/relaxation/pending.txtar"),
        include_str!("../../../tests/conformance/regressions/relaxation/lets.txtar"),
        include_str!("../../../tests/conformance/regressions/relaxation/chains.txtar"),
        include_str!("../../../tests/conformance/regressions/relaxation/closedness.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        let checks = report["cases"][0]["checks"].as_array().unwrap();
        assert!(
            ok && !checks.is_empty() && checks.iter().all(|check| check["status"] == "passed"),
            "{report}"
        );
    }
}

#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn alias_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/aliases/postfix.txtar"),
        include_str!("../../../tests/conformance/regressions/aliases/prefix.txtar"),
        include_str!("../../../tests/conformance/regressions/aliases/closed.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        let checks = report["cases"][0]["checks"].as_array().unwrap();
        assert!(
            ok && !checks.is_empty() && checks.iter().all(|check| check["status"] == "passed"),
            "{report}"
        );
    }
}

/// The runner owns its deadlines: nothing on PATH is needed to run a case, so a
/// stock macOS, which has no GNU `timeout`, runs the corpus too. The oracle
/// stand-in is a shell script using only builtins; the worker is this binary.
#[cfg(unix)]
#[test]
fn a_case_runs_with_nothing_on_path() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let cases = dir.path().join("cases");
    fs::create_dir(&cases).unwrap();
    let input = "-- in.cue --\na: 1\n";
    fs::write(cases.join("case.txtar"), input).unwrap();
    let sha: String = Sha256::digest(input)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let plan = serde_json::json!({
        "revision": cue_test_harness::conformance::ORACLE_REVISION,
        "sha256": sha,
        "checks": [{"id": "x", "file": "in.cue", "path": [], "operation": "export", "expected": {"a": 1}}],
    });
    let oracle = dir.path().join("oracle");
    fs::write(&oracle, format!("#!/bin/sh\nprintf '%s\\n' '{plan}'\n")).unwrap();
    fs::set_permissions(&oracle, fs::Permissions::from_mode(0o700)).unwrap();
    let empty = dir.path().join("empty-path");
    fs::create_dir(&empty).unwrap();
    let report = dir.path().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_cue-rs"))
        .current_dir(dir.path())
        .env("PATH", &empty)
        .arg("conformance")
        .arg(&cases)
        .arg("--oracle")
        .arg(&oracle)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(
        &fs::read(&report)
            .unwrap_or_else(|_| panic!("no report: {}", String::from_utf8_lossy(&output.stderr))),
    )
    .unwrap();
    assert!(output.status.success(), "{report}");
    assert_eq!(
        report["cases"][0]["checks"][0]["status"], "passed",
        "{report}"
    );
}

/// Quoted selectors and labels that spell a definition, a hidden field or a
/// keyword, and preference marks as disjuncts. A misplaced mark fails the
/// whole package, which the worker cannot attribute to a path yet
/// (`open/misplaced-marks.txtar`).
#[test]
#[ignore = "requires pinned Go oracle; run just conformance-test under just safe"]
fn label_and_mark_regressions_match_the_pinned_reference() {
    for source in [
        include_str!("../../../tests/conformance/regressions/labels/quoted.txtar"),
        include_str!("../../../tests/conformance/regressions/labels/quoted-errors.txtar"),
        include_str!("../../../tests/conformance/regressions/marks/disjuncts.txtar"),
    ] {
        let (ok, report) = oracle_case(source);
        let checks = report["cases"][0]["checks"].as_array().unwrap();
        assert!(
            ok && !checks.is_empty() && checks.iter().all(|check| check["status"] == "passed"),
            "{report}"
        );
    }
}
