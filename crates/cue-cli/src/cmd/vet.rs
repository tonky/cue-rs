use anyhow::{Context, Result};
use std::path::Path;

pub fn run(schema: &Path, data: &Path) -> Result<()> {
    let schema_content = std::fs::read_to_string(schema)
        .with_context(|| format!("Failed to read schema file {}", schema.display()))?;
    let data_content = std::fs::read_to_string(data)
        .with_context(|| format!("Failed to read data file {}", data.display()))?;

    let json_data: serde_json::Value =
        serde_json::from_str(&data_content).with_context(|| "Failed to parse data file as JSON")?;

    match cue_eval::validate_json(&schema_content, &json_data) {
        Ok(()) => {
            println!("Validation successful");
            Ok(())
        }
        Err(e) => anyhow::bail!("Validation failed: {e}"),
    }
}
