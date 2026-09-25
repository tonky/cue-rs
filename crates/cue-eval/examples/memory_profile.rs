//! Allocation attribution only; measure peak RSS with the uninstrumented CLI.
use cue_eval::PackageLoader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: memory_profile PACKAGE_DIRECTORY")?;
    let (evaluator, root) = PackageLoader::load_dir(path)?;
    let profile = evaluator.arena.memory_profile();
    let export = match evaluator.to_json(root) {
        Ok(_) => serde_json::json!({"concrete": true}),
        Err(error) => serde_json::json!({"concrete": false, "error": error}),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "arena": profile,
            "derivations": evaluator.derivations,
            "unsettled": evaluator.unsettled,
            "export": export,
        }))?
    );
    Ok(())
}
