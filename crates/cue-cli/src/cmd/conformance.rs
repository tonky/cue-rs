use anyhow::Result;
use cue_test_harness::TxtarArchive;
use cue_test_harness::conformance::{Manifest, ORACLE_REVISION, Report, Runner, Status};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ConformanceOptions<'a> {
    pub path: &'a Path,
    pub oracle: &'a Path,
    pub report: &'a Path,
    pub baseline: Option<&'a Path>,
    pub manifest: Option<&'a Path>,
    pub filter: Option<&'a str>,
    pub timeout_seconds: u64,
}

pub fn run(opts: ConformanceOptions<'_>) -> Result<()> {
    let fixtures = collect_fixtures(opts.path, opts.filter)?;
    let manifest = load_manifest(opts.manifest)?;
    let baseline = load_baseline(opts.baseline, opts.report)?;

    let runner = Runner {
        oracle: std::fs::canonicalize(opts.oracle)?,
        worker: std::env::current_exe()?,
        timeout_seconds: opts.timeout_seconds,
        scratch: PathBuf::from("tmp/conformance"),
        manifest,
    };

    let results = run_fixtures(&runner, &fixtures);
    write_report_and_verify(&results, opts.report, baseline.as_ref())
}

fn collect_fixtures(path: &Path, filter: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut fixtures = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        TxtarArchive::discover(path)?
    } else {
        anyhow::bail!("fixture path does not exist: {}", path.display());
    };

    if let Some(filter) = filter {
        fixtures.retain(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(filter))
        });
    }

    anyhow::ensure!(!fixtures.is_empty(), "no txtar fixtures discovered");
    let mut names = HashSet::new();
    anyhow::ensure!(
        fixtures.iter().all(|f| names.insert(f.file_name())),
        "duplicate case names in fixture directory"
    );
    Ok(fixtures)
}

fn load_manifest(manifest: Option<&Path>) -> Result<Option<Manifest>> {
    let Some(path) = manifest else {
        return Ok(None);
    };
    let content = std::fs::read(path)?;
    let parsed: Manifest = serde_json::from_slice(&content)?;
    Ok(Some(parsed))
}

fn load_baseline(baseline: Option<&Path>, report: &Path) -> Result<Option<Report>> {
    let Some(path) = baseline else {
        return Ok(None);
    };
    let report_clash = path == report
        || (std::fs::canonicalize(path).ok())
            .zip(std::fs::canonicalize(report).ok())
            .is_some_and(|(a, b)| a == b);
    anyhow::ensure!(
        !report_clash,
        "report output must not overwrite the baseline"
    );

    let content = std::fs::read(path)?;
    let parsed: Report = serde_json::from_slice(&content)?;
    Ok(Some(parsed))
}

fn run_fixtures(runner: &Runner, fixtures: &[PathBuf]) -> Report {
    let mut results = Report {
        oracle_revision: ORACLE_REVISION.into(),
        cases: Vec::with_capacity(fixtures.len()),
    };

    for fixture in fixtures {
        let result = runner.run(fixture);
        let checked = result
            .checks
            .iter()
            .filter(|c| c.status == Status::Passed)
            .count();
        let status_str = if result.passed() {
            "PASSED"
        } else {
            "INCOMPLETE/FAILED"
        };
        println!(
            "{}: {} ({checked}/{} checks passed)",
            result.case,
            status_str,
            result.checks.len()
        );
        results.cases.push(result);
    }
    results
}

fn write_report_and_verify(
    results: &Report,
    report: &Path,
    baseline: Option<&Report>,
) -> Result<()> {
    if let Some(parent) = report.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(report, serde_json::to_vec_pretty(results)?)?;

    let passed = results.cases.iter().filter(|c| c.passed()).count();
    println!(
        "Conformance: {passed}/{} cases fully verified; report {}",
        results.cases.len(),
        report.display()
    );

    if let Some(baseline) = baseline {
        results
            .compare_baseline(baseline)
            .map_err(anyhow::Error::msg)?;
        println!("Migration baseline matched; this is not a conformance pass.");
        return Ok(());
    }

    anyhow::ensure!(
        results.accepted(),
        "conformance has failures or incomplete verification"
    );
    Ok(())
}
