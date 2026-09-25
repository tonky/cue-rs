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
