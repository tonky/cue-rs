use anyhow::Result;
use clap::Parser;

mod args;
mod cmd;
mod conformance;

fn main() -> Result<()> {
    let cli = args::Cli::parse();
    cmd::run(cli.command)
}
