use anyhow::Result;
use std::path::Path;

pub fn run(file: &Path, format: &str, pretty: bool) -> Result<()> {
    let (evaluator, root_id) = if file.is_dir() {
        cue_eval::PackageLoader::load_dir(file)
            .map_err(|e| anyhow::anyhow!("Package load error: {e}"))?
    } else {
        cue_eval::PackageLoader::load_file(file)
            .map_err(|e| anyhow::anyhow!("CUE evaluation error: {e}"))?
    };
    let json = evaluator
        .to_json(root_id)
        .map_err(|e| anyhow::anyhow!("CUE export failed: {e}"))?;

    print_output(&json, format, pretty)
}

fn print_output(json: &serde_json::Value, format: &str, pretty: bool) -> Result<()> {
    if matches!(format.to_lowercase().as_str(), "yaml" | "yml") {
        let yml = cue_eval::export::json_to_yaml(json).map_err(anyhow::Error::msg)?;
        print!("{yml}");
        return Ok(());
    }

    if pretty {
        println!("{}", serde_json::to_string_pretty(json)?);
    } else {
        println!("{}", serde_json::to_string(json)?);
    }
    Ok(())
}
