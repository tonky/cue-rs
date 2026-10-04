use anyhow::{Context, Result};
use std::path::Path;

pub fn run(file: &Path, write: bool) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read file {}", file.display()))?;
    let source_file = cue_syntax::parse_file(&content).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            e.format_with_source(&content, Some(&file.to_string_lossy()))
        )
    })?;
    let formatted = cue_syntax::format_file(&source_file);

    if !write {
        print!("{formatted}");
        return Ok(());
    }

    std::fs::write(file, &formatted)
        .with_context(|| format!("Failed to write file {}", file.display()))?;
    println!("Formatted {}", file.display());
    Ok(())
}
