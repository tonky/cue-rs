use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn run_test_txtar(path: &Path, strict_errors: bool) -> Result<()> {
    println!("Legacy heuristic checks; passes do not establish conformance.");
    if path.is_file() {
        println!("Running test: {}", path.display());
        run_txtar_case(path, strict_errors)?;
        println!("PASSED");
        return Ok(());
    }

    if path.is_dir() {
        return run_txtar_dir(path, strict_errors);
    }

    anyhow::bail!("Path {} does not exist", path.display())
}

fn run_txtar_dir(dir: &Path, strict_errors: bool) -> Result<()> {
    let fixtures = discover_txtar_fixtures(dir)?;
    println!("Discovered {} .txtar fixtures\n", fixtures.len());

    let mut passed = 0;
    let mut failed = 0;

    for fix in fixtures {
        print!("Running {} ... ", fix.display());
        match run_txtar_case(&fix, strict_errors) {
            Ok(()) => {
                println!("PASSED");
                passed += 1;
            }
            Err(e) => {
                println!("FAILED: {e}");
                failed += 1;
            }
        }
    }

    println!("\nTest Results: {passed} passed, {failed} failed");
    anyhow::ensure!(passed + failed > 0, "no txtar fixtures discovered");
    anyhow::ensure!(failed == 0, "{failed} legacy checks failed");
    Ok(())
}

fn discover_txtar_fixtures(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut fixtures = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("txtar") {
            fixtures.push(p);
        }
    }
    fixtures.sort();
    Ok(fixtures)
}

pub fn run_sync_upstream(src: &Path, dest: &Path, filter: Option<&str>, test: bool) -> Result<()> {
    if !src.exists() {
        anyhow::bail!("Source path {} does not exist", src.display());
    }
    std::fs::create_dir_all(dest)?;

    let mut synced = Vec::new();
    sync_txtar_recursive(src, dest, filter, &mut synced)?;
    println!(
        "Successfully ingested {} upstream txtar test fixtures to {}",
        synced.len(),
        dest.display()
    );

    if test {
        verify_synced_fixtures(&synced)?;
    }
    Ok(())
}

fn verify_synced_fixtures(fixtures: &[PathBuf]) -> Result<()> {
    println!("\n--- Running Ingestion Conformance Verification ---");
    let mut passed = 0;
    let mut failed = 0;

    for p in fixtures {
        print!("Testing {} ... ", p.display());
        match run_txtar_file(p) {
            Ok(()) => {
                println!("PASSED");
                passed += 1;
            }
            Err(e) => {
                println!("FAILED: {e}");
                failed += 1;
            }
        }
    }

    println!("\nSync Verification Results: {passed} passed, {failed} failed");
    anyhow::ensure!(failed == 0, "{failed} legacy checks failed");
    Ok(())
}

fn sync_txtar_recursive(
    src: &Path,
    dest: &Path,
    filter: Option<&str>,
    synced: &mut Vec<PathBuf>,
) -> Result<()> {
    if src.is_file() {
        sync_txtar_file(src, dest, filter, synced)?;
        return Ok(());
    }

    if !src.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            sync_txtar_recursive(&path, dest, filter, synced)?;
        } else {
            sync_txtar_file(&path, dest, filter, synced)?;
        }
    }
    Ok(())
}

fn sync_txtar_file(
    path: &Path,
    dest: &Path,
    filter: Option<&str>,
    synced: &mut Vec<PathBuf>,
) -> Result<()> {
    if path.extension().and_then(|s| s.to_str()) != Some("txtar") {
        return Ok(());
    }

    let Some(file_name_os) = path.file_name() else {
        return Ok(());
    };
    let file_name = file_name_os.to_string_lossy();
    if let Some(f) = filter
        && !file_name.contains(f)
        && !path.to_string_lossy().contains(f)
    {
        return Ok(());
    }

    let clean_name = derive_fixture_name(path);
    let target_path = dest.join(clean_name);
    std::fs::copy(path, &target_path)?;
    synced.push(target_path);
    Ok(())
}

fn derive_fixture_name(path: &Path) -> String {
    let p_str = path.to_string_lossy();
    if let Some(pos) = p_str.find("/cue/testdata/") {
        let sub = &p_str[pos + 1..];
        return format!("upstream_{}", sub.replace('/', "_"));
    }
    if let Some(pos) = p_str.find("/pkg/") {
        let sub = &p_str[pos + 1..];
        return format!("upstream_{}", sub.replace('/', "_"));
    }

    let base = path
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("fixture"));
    format!("upstream_{base}")
}

struct LegacyBackend;

impl cue_test_harness::legacy::Backend for LegacyBackend {
    fn evaluate(
        &self,
        files: &[(&str, &str)],
        export_each: bool,
    ) -> std::result::Result<(), cue_test_harness::legacy::Failure> {
        use cue_test_harness::legacy::Failure;
        let mut evaluator = cue_eval::Evaluator::new();
        let mut last = None;

        for (name, content) in files {
            // Upstream's fixtures are written at the language version of the
            // revision they were imported from.
            let options =
                cue_syntax::ParseOptions::at_version(crate::conformance::ORACLE_LANGUAGE_VERSION);
            let file = cue_syntax::parse_file_with(content, &options).map_err(|e| Failure {
                message: format!("{name}: {e}"),
                during_export: false,
            })?;
            let value = evaluator.eval_file(&file).map_err(|e| Failure {
                message: format!("{name}: {e}"),
                during_export: false,
            })?;
            if export_each {
                evaluator.to_json(value).map_err(|e| Failure {
                    message: format!("{name}: {e}"),
                    during_export: true,
                })?;
            }
            last = Some(value);
        }

        if let Some(value) = last {
            evaluator.to_json(value).map_err(|message| Failure {
                message,
                during_export: true,
            })?;
        }
        Ok(())
    }
}

pub fn run_txtar_case(path: &Path, strict_errors: bool) -> Result<()> {
    cue_test_harness::legacy::run_case(path, strict_errors, &LegacyBackend)
        .map_err(anyhow::Error::msg)
}

fn run_txtar_file(path: &Path) -> Result<()> {
    run_txtar_case(path, false)
}
