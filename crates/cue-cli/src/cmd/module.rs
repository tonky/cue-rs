use crate::args::ModCommands;
use anyhow::{Context, Result};
use std::path::Path;

pub fn run(command: ModCommands) -> Result<()> {
    match command {
        ModCommands::Init { module, dir } => run_init(&dir, &module),
        ModCommands::Tidy { dir } => run_tidy(&dir),
    }
}

fn run_init(dir: &Path, module: &str) -> Result<()> {
    let manifest_path = cue_eval::ModuleManifest::init(dir, module)
        .map_err(|e| anyhow::anyhow!("Failed to initialize CUE module: {e}"))?;
    println!(
        "Initialized CUE module '{}' at {}",
        module,
        manifest_path.display()
    );
    Ok(())
}

fn run_tidy(dir: &Path) -> Result<()> {
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
    Ok(())
}
