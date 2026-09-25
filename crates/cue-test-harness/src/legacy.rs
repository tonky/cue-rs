//! The old heuristic runner, retained only for migration baseline comparisons.
//! A success here does not establish conformance.
use crate::TxtarArchive;

pub trait Backend {
    fn evaluate(&self, files: &[(&str, &str)], export_each: bool) -> Result<(), Failure>;
}

pub struct Failure {
    pub message: String,
    pub during_export: bool,
}

/// What upstream recorded a case failing with.
enum ExpectedErrors {
    /// No `out/errors.txt`, or an empty one.
    None,
    /// Every recorded error is `field not allowed`.
    Closedness,
    Other,
}

impl ExpectedErrors {
    fn of(archive: &TxtarArchive) -> Self {
        let Some(errors) = archive.files.get("out/errors.txt") else {
            return Self::None;
        };
        let messages: Vec<&str> = errors
            .lines()
            .filter(|line| line.starts_with('['))
            .collect();
        if messages.is_empty() {
            Self::None
        } else if messages.iter().all(|m| m.contains("field not allowed")) {
            Self::Closedness
        } else {
            Self::Other
        }
    }
}

pub fn run_case(
    path: &std::path::Path,
    strict_errors: bool,
    backend: &impl Backend,
) -> Result<(), String> {
    if !strict_errors {
        return run_file(path, backend);
    }
    let archive = TxtarArchive::from_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let expected = ExpectedErrors::of(&archive);
    if matches!(expected, ExpectedErrors::None) {
        return run_file(path, backend);
    }
    match (expected, evaluate_archive(&archive, backend)) {
        (_, Ok(())) => Err("evaluated without the error upstream records".into()),
        (ExpectedErrors::Closedness, Err(e)) if !e.contains("not allowed") => {
            Err(format!("expected a closedness error, got: {e}"))
        }
        (_, Err(_)) => Ok(()),
    }
}

/// Evaluate and export every CUE file of an archive, stopping at the first
/// error. Each file is exported, not only the last: a case's errors may sit in
/// any of them.
fn evaluate_archive(archive: &TxtarArchive, backend: &impl Backend) -> Result<(), String> {
    backend
        .evaluate(&archive.cue_files(), true)
        .map_err(|e| e.message)
}

fn run_file(path: &std::path::Path, backend: &impl Backend) -> Result<(), String> {
    let archive = TxtarArchive::from_file(path).map_err(|e| format!("{}: {e}", path.display()))?;

    let cue_files = archive.cue_files();
    if cue_files.is_empty() {
        return Err("No CUE files found in archive".into());
    }

    let has_expected_error = archive.files.keys().any(|k| k.contains("error"))
        || archive
            .files
            .values()
            .any(|v| v.contains("Errors:") || v.contains("_|_"));

    let schema = archive.files.keys().any(|k| {
        k.contains("error")
            || k.contains("evalalpha")
            || k.contains("stats")
            || k.contains("compile")
    });
    match backend.evaluate(&cue_files, false) {
        Ok(()) => Ok(()),
        Err(error) if has_expected_error || (schema && error.during_export) => Ok(()),
        Err(error) => Err(error.message),
    }
}
