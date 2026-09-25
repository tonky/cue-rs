use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cue_test_harness::TxtarArchive;
use std::path::PathBuf;

mod conformance;

#[derive(Parser, Debug)]
#[command(
    name = "cue-rs",
    version = "0.1.0",
    about = "High-performance CUE language validator and evaluator in Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Evaluate a CUE file and export to JSON or YAML
    Eval {
        /// Input CUE file
        file: PathBuf,
        /// Output export format (json or yaml)
        #[arg(long, default_value = "json")]
        format: String,
        /// Pretty-print the JSON output
        #[arg(short, long, default_value_t = true)]
        pretty: bool,
    },
    /// Vet / validate a JSON or CUE data file against a CUE schema
    Vet {
        /// Schema CUE file
        schema: PathBuf,
        /// Data file to validate
        data: PathBuf,
    },
    /// Format a CUE file
    Fmt {
        /// Input CUE file
        file: PathBuf,
        /// Overwrite file in-place
        #[arg(short, long)]
        write: bool,
    },
    /// Run legacy heuristic txtar checks (does not establish conformance)
    TestTxtar {
        /// Path to .txtar file or directory containing .txtar files
        path: PathBuf,
        /// Hold a case to the errors upstream recorded: a case with an
        /// `out/errors.txt` must fail, and one whose recorded errors are all
        /// closedness errors must fail with one.
        #[arg(long)]
        strict_errors: bool,
    },
    /// Compare operation-specific assertions with the pinned upstream oracle
    Conformance {
        #[arg(default_value = "tests/testdata")]
        path: PathBuf,
        #[arg(long, default_value = "tmp/conformance-oracle")]
        oracle: PathBuf,
        #[arg(long, default_value = "tmp/conformance-report.json")]
        report: PathBuf,
        /// Migration comparison only; success is not conformance acceptance
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Verify fixture bytes and source revision against this manifest
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Select case filenames containing this substring
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..))]
        timeout_seconds: u64,
    },
    #[command(hide = true)]
    ConformanceWorker { request: PathBuf },
    /// Ingest / sync upstream test suites from a local CUE repository checkout
    SyncUpstream {
        /// Source directory or file in upstream CUE repo (e.g. /path/to/cue/cue/testdata/eval)
        #[arg(short, long)]
        src: PathBuf,
        /// Destination directory in cue-rs
        #[arg(short, long, default_value = "tests/testdata")]
        dest: PathBuf,
        /// Optional substring filter for fixture filename
        #[arg(short, long)]
        filter: Option<String>,
        /// Automatically run test runner on synced fixtures
        #[arg(short, long)]
        test: bool,
    },
    /// Import schema definitions from other formats (JSON Schema, OpenAPI)
    Import {
        #[command(subcommand)]
        command: ImportCommands,
    },
    /// CUE module and dependency management
    Mod {
        #[command(subcommand)]
        command: ModCommands,
    },
}

#[derive(Subcommand, Debug)]
enum ImportCommands {
    /// Convert a JSON Schema file to CUE definitions
    JsonSchema {
        /// Input JSON Schema file
        file: PathBuf,
        /// Root definition name (default: Schema)
        #[arg(short, long)]
        root: Option<String>,
        /// Output file to write CUE schema to (defaults to stdout)
        #[arg(short, long)]
        write: Option<PathBuf>,
    },
    /// Convert an OpenAPI v3 specification to CUE definitions
    Openapi {
        /// Input OpenAPI specification file (JSON or YAML)
        file: PathBuf,
        /// Output file to write CUE schema to (defaults to stdout)
        #[arg(short, long)]
        write: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum ModCommands {
    /// Initialize a new CUE module in the current directory or specified path
    Init {
        /// Module path / name (e.g. example.com/mymod@v0)
        module: String,
        /// Target directory
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
    },
    /// Verify and tidy module dependencies in cue.mod/module.cue
    Tidy {
        /// Target directory
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Fmt { file, write } => {
            let content = std::fs::read_to_string(&file)
                .with_context(|| format!("Failed to read file {}", file.display()))?;
            let source_file = cue_syntax::parse_file(&content).map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    e.format_with_source(&content, Some(&file.to_string_lossy()))
                )
            })?;
            let formatted = cue_syntax::format_file(&source_file);

            if write {
                std::fs::write(&file, &formatted)
                    .with_context(|| format!("Failed to write file {}", file.display()))?;
                println!("Formatted {}", file.display());
            } else {
                print!("{formatted}");
            }
        }
        Commands::Eval {
            file,
            format,
            pretty,
        } => {
            let (evaluator, root_id) = if file.is_dir() {
                cue_eval::PackageLoader::load_dir(&file)
                    .map_err(|e| anyhow::anyhow!("Package load error: {e}"))?
            } else {
                cue_eval::PackageLoader::load_file(&file)
                    .map_err(|e| anyhow::anyhow!("CUE evaluation error: {e}"))?
            };
            let json = evaluator
                .to_json(root_id)
                .map_err(|e| anyhow::anyhow!("CUE export failed: {e}"))?;

            match format.to_lowercase().as_str() {
                "yaml" | "yml" => {
                    let yml = cue_eval::export::json_to_yaml(&json).map_err(anyhow::Error::msg)?;
                    print!("{yml}");
                }
                _ => {
                    if pretty {
                        println!("{}", serde_json::to_string_pretty(&json)?);
                    } else {
                        println!("{}", serde_json::to_string(&json)?);
                    }
                }
            }
        }
        Commands::Vet { schema, data } => {
            let schema_content = std::fs::read_to_string(&schema)
                .with_context(|| format!("Failed to read schema file {}", schema.display()))?;
            let data_content = std::fs::read_to_string(&data)
                .with_context(|| format!("Failed to read data file {}", data.display()))?;

            let json_data: serde_json::Value = serde_json::from_str(&data_content)
                .with_context(|| "Failed to parse data file as JSON")?;

            match cue_eval::validate_json(&schema_content, &json_data) {
                Ok(()) => println!("Validation successful"),
                Err(e) => anyhow::bail!("Validation failed: {e}"),
            }
        }
        Commands::ConformanceWorker { request } => conformance::worker(&request)?,
        Commands::Conformance {
            path,
            oracle,
            report,
            baseline,
            manifest,
            filter,
            timeout_seconds,
        } => {
            use cue_test_harness::conformance::{ORACLE_REVISION, Report, Runner};
            let mut fixtures = if path.is_file() {
                vec![path]
            } else if path.is_dir() {
                TxtarArchive::discover(&path)?
            } else {
                anyhow::bail!("fixture path does not exist: {}", path.display());
            };
            if let Some(filter) = filter {
                fixtures.retain(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().contains(&filter))
                });
            }
            anyhow::ensure!(!fixtures.is_empty(), "no txtar fixtures discovered");
            let mut names = std::collections::HashSet::new();
            anyhow::ensure!(
                fixtures.iter().all(|f| names.insert(f.file_name())),
                "duplicate case names in fixture directory"
            );
            let manifest = manifest
                .map(|path| -> Result<_> { Ok(serde_json::from_slice(&std::fs::read(path)?)?) })
                .transpose()?;
            let baseline: Option<Report> = baseline
                .map(|path| -> Result<_> {
                    anyhow::ensure!(
                        path != report
                            && (std::fs::canonicalize(&path)
                                .ok()
                                .zip(std::fs::canonicalize(&report).ok())
                                .is_none_or(|(a, b)| a != b)),
                        "report output must not overwrite the baseline"
                    );
                    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
                })
                .transpose()?;
            let runner = Runner {
                oracle: std::fs::canonicalize(oracle)?,
                worker: std::env::current_exe()?,
                timeout_seconds,
                scratch: PathBuf::from("tmp/conformance"),
                manifest,
            };
            let mut results = Report {
                oracle_revision: ORACLE_REVISION.into(),
                cases: Vec::new(),
            };
            for fixture in fixtures {
                let result = runner.run(&fixture);
                let checked = result
                    .checks
                    .iter()
                    .filter(|c| c.status == cue_test_harness::conformance::Status::Passed)
                    .count();
                println!(
                    "{}: {} ({checked}/{} checks passed)",
                    result.case,
                    if result.passed() {
                        "PASSED"
                    } else {
                        "INCOMPLETE/FAILED"
                    },
                    result.checks.len()
                );
                results.cases.push(result);
            }
            if let Some(parent) = report.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&report, serde_json::to_vec_pretty(&results)?)?;
            let passed = results.cases.iter().filter(|c| c.passed()).count();
            println!(
                "Conformance: {passed}/{} cases fully verified; report {}",
                results.cases.len(),
                report.display()
            );
            if let Some(baseline) = baseline {
                results
                    .compare_baseline(&baseline)
                    .map_err(anyhow::Error::msg)?;
                println!("Migration baseline matched; this is not a conformance pass.");
            } else {
                anyhow::ensure!(
                    results.accepted(),
                    "conformance has failures or incomplete verification"
                );
            }
        }
        Commands::TestTxtar {
            path,
            strict_errors,
        } => {
            println!("Legacy heuristic checks; passes do not establish conformance.");
            if path.is_file() {
                println!("Running test: {}", path.display());
                run_txtar_case(&path, strict_errors)?;
                println!("PASSED");
            } else if path.is_dir() {
                let mut fixtures = Vec::new();
                for entry in std::fs::read_dir(&path)? {
                    let entry = entry?;
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("txtar") {
                        fixtures.push(p);
                    }
                }
                fixtures.sort();
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
            } else {
                anyhow::bail!("Path {} does not exist", path.display());
            }
        }
        Commands::SyncUpstream {
            src,
            dest,
            filter,
            test,
        } => {
            if !src.exists() {
                anyhow::bail!("Source path {} does not exist", src.display());
            }
            std::fs::create_dir_all(&dest)?;
            let mut synced = Vec::new();
            sync_txtar_recursive(&src, &dest, filter.as_deref(), &mut synced)?;
            println!(
                "Successfully ingested {} upstream txtar test fixtures to {}",
                synced.len(),
                dest.display()
            );

            if test {
                println!("\n--- Running Ingestion Conformance Verification ---");
                let mut passed = 0;
                let mut failed = 0;
                for p in &synced {
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
            }
        }
        Commands::Import { command } => match command {
            ImportCommands::JsonSchema { file, root, write } => {
                let content = std::fs::read_to_string(&file).with_context(|| {
                    format!("Failed to read JSON Schema file {}", file.display())
                })?;
                let json_val: serde_json::Value = serde_json::from_str(&content)
                    .with_context(|| "Failed to parse file as valid JSON")?;

                let cue_output = cue_eval::json_schema_to_cue(&json_val, root.as_deref())
                    .map_err(|e| anyhow::anyhow!("JSON Schema conversion error: {e}"))?;

                if let Some(out_path) = write {
                    std::fs::write(&out_path, &cue_output).with_context(|| {
                        format!("Failed to write CUE file to {}", out_path.display())
                    })?;
                    println!("Generated CUE schema at {}", out_path.display());
                } else {
                    print!("{cue_output}");
                }
            }
            ImportCommands::Openapi { file, write } => {
                let content = std::fs::read_to_string(&file)
                    .with_context(|| format!("Failed to read OpenAPI file {}", file.display()))?;

                let json_val: serde_json::Value = if file.extension().and_then(|s| s.to_str())
                    == Some("yaml")
                    || file.extension().and_then(|s| s.to_str()) == Some("yml")
                {
                    serde_yaml_ng::from_str(&content)
                        .with_context(|| "Failed to parse file as valid YAML")?
                } else {
                    serde_json::from_str(&content)
                        .with_context(|| "Failed to parse file as valid JSON")?
                };

                let cue_output = cue_eval::openapi_to_cue(&json_val)
                    .map_err(|e| anyhow::anyhow!("OpenAPI conversion error: {e}"))?;

                if let Some(out_path) = write {
                    std::fs::write(&out_path, &cue_output).with_context(|| {
                        format!("Failed to write CUE file to {}", out_path.display())
                    })?;
                    println!("Generated CUE schema at {}", out_path.display());
                } else {
                    print!("{cue_output}");
                }
            }
        },
        Commands::Mod { command } => match command {
            ModCommands::Init { module, dir } => {
                let manifest_path = cue_eval::ModuleManifest::init(&dir, &module)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize CUE module: {e}"))?;
                println!(
                    "Initialized CUE module '{}' at {}",
                    module,
                    manifest_path.display()
                );
            }
            ModCommands::Tidy { dir } => {
                let cue_mod = dir.join("cue.mod").join("module.cue");
                if !cue_mod.is_file() {
                    anyhow::bail!("No cue.mod/module.cue found in {}", dir.display());
                }
                let content = std::fs::read_to_string(&cue_mod)
                    .with_context(|| format!("Failed to read {}", cue_mod.display()))?;
                let manifest = cue_eval::ModuleManifest::from_cue_string(&content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse module manifest: {e}"))?;
                std::fs::write(&cue_mod, manifest.to_cue_string())
                    .with_context(|| format!("Failed to update {}", cue_mod.display()))?;
                println!("Tidied {}", cue_mod.display());
            }
        },
    }

    Ok(())
}

fn sync_txtar_recursive(
    src: &std::path::Path,
    dest: &std::path::Path,
    filter: Option<&str>,
    synced: &mut Vec<PathBuf>,
) -> Result<()> {
    if src.is_file() && src.extension().and_then(|s| s.to_str()) == Some("txtar") {
        let Some(file_name_os) = src.file_name() else {
            return Ok(());
        };
        let file_name = file_name_os.to_string_lossy();
        if let Some(f) = filter
            && !file_name.contains(f)
            && !src.to_string_lossy().contains(f)
        {
            return Ok(());
        }
        let clean_name = derive_fixture_name(src);
        let target_path = dest.join(clean_name);
        std::fs::copy(src, &target_path)?;
        synced.push(target_path);
    } else if src.is_dir() {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                sync_txtar_recursive(&path, dest, filter, synced)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("txtar") {
                let Some(file_name_os) = path.file_name() else {
                    continue;
                };
                let file_name = file_name_os.to_string_lossy();
                if let Some(f) = filter
                    && !file_name.contains(f)
                    && !path.to_string_lossy().contains(f)
                {
                    continue;
                }
                let clean_name = derive_fixture_name(&path);
                let target_path = dest.join(clean_name);
                std::fs::copy(&path, &target_path)?;
                synced.push(target_path);
            }
        }
    }
    Ok(())
}

fn derive_fixture_name(path: &std::path::Path) -> String {
    let p_str = path.to_string_lossy();
    if let Some(pos) = p_str.find("/cue/testdata/") {
        let sub = &p_str[pos + 1..];
        format!("upstream_{}", sub.replace('/', "_"))
    } else if let Some(pos) = p_str.find("/pkg/") {
        let sub = &p_str[pos + 1..];
        format!("upstream_{}", sub.replace('/', "_"))
    } else {
        let base = path
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("fixture"));
        format!("upstream_{base}")
    }
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
            let file = cue_syntax::parse_file(content).map_err(|e| Failure {
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
fn run_txtar_case(path: &std::path::Path, strict_errors: bool) -> Result<()> {
    cue_test_harness::legacy::run_case(path, strict_errors, &LegacyBackend)
        .map_err(anyhow::Error::msg)
}
fn run_txtar_file(path: &std::path::Path) -> Result<()> {
    run_txtar_case(path, false)
}
