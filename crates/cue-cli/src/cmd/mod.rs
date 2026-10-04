pub mod conformance;
pub mod eval;
pub mod fmt;
pub mod import;
pub mod module;
pub mod txtar;
pub mod vet;

use crate::args::Commands;
use anyhow::Result;

pub fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Eval {
            file,
            format,
            pretty,
        } => eval::run(&file, &format, pretty),
        Commands::Vet { schema, data } => vet::run(&schema, &data),
        Commands::Fmt { file, write } => fmt::run(&file, write),
        Commands::TestTxtar {
            path,
            strict_errors,
        } => txtar::run_test_txtar(&path, strict_errors),
        Commands::Conformance {
            path,
            oracle,
            report,
            baseline,
            manifest,
            filter,
            timeout_seconds,
        } => conformance::run(conformance::ConformanceOptions {
            path: &path,
            oracle: &oracle,
            report: &report,
            baseline: baseline.as_deref(),
            manifest: manifest.as_deref(),
            filter: filter.as_deref(),
            timeout_seconds,
        }),
        Commands::ConformanceWorker { request } => crate::conformance::worker(&request),
        Commands::SyncUpstream {
            src,
            dest,
            filter,
            test,
        } => txtar::run_sync_upstream(&src, &dest, filter.as_deref(), test),
        Commands::Import { command } => import::run(command),
        Commands::Mod { command } => module::run(command),
    }
}
