use crate::args::ImportCommands;
use anyhow::{Context, Result};
use std::path::Path;

pub fn run(command: ImportCommands) -> Result<()> {
    match command {
        ImportCommands::JsonSchema { file, root, write } => {
            run_json_schema(&file, root.as_deref(), write.as_deref())
        }
        ImportCommands::Openapi { file, write } => run_openapi(&file, write.as_deref()),
    }
}

fn run_json_schema(file: &Path, root: Option<&str>, write: Option<&Path>) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read JSON Schema file {}", file.display()))?;
    let json_val: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "Failed to parse file as valid JSON")?;

    let cue_output = cue_eval::json_schema_to_cue(&json_val, root)
        .map_err(|e| anyhow::anyhow!("JSON Schema conversion error: {e}"))?;

    write_or_print(&cue_output, write)
}

fn run_openapi(file: &Path, write: Option<&Path>) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read OpenAPI file {}", file.display()))?;

    let is_yaml = matches!(
        file.extension().and_then(|s| s.to_str()),
        Some("yaml" | "yml")
    );
    let json_val: serde_json::Value = if is_yaml {
        serde_yaml_ng::from_str(&content).with_context(|| "Failed to parse file as valid YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse file as valid JSON")?
    };

    let cue_output = cue_eval::openapi_to_cue(&json_val)
        .map_err(|e| anyhow::anyhow!("OpenAPI conversion error: {e}"))?;

    write_or_print(&cue_output, write)
}

fn write_or_print(cue_output: &str, write: Option<&Path>) -> Result<()> {
    let Some(out_path) = write else {
        print!("{cue_output}");
        return Ok(());
    };

    std::fs::write(out_path, cue_output)
        .with_context(|| format!("Failed to write CUE file to {}", out_path.display()))?;
    println!("Generated CUE schema at {}", out_path.display());
    Ok(())
}
