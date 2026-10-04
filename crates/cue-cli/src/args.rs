use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "cue-rs",
    version = "0.1.0",
    about = "High-performance CUE language validator and evaluator in Rust"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
pub enum ImportCommands {
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
pub enum ModCommands {
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
