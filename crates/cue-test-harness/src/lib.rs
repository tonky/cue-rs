use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TxtarError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid txtar format: {0}")]
    Format(String),
}

/// A parsed txtar archive, containing a comment/header and named files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxtarArchive {
    pub comment: String,
    pub files: HashMap<String, String>,
    pub file_order: Vec<String>,
}

impl TxtarArchive {
    /// Parse a txtar archive string.
    pub fn parse(input: &str) -> Result<Self, TxtarError> {
        let mut comment_lines = Vec::new();
        let mut files = HashMap::new();
        let mut file_order = Vec::new();

        let mut current_file: Option<String> = None;
        let mut current_content: Vec<&str> = Vec::new();

        for line in input.lines() {
            if let Some(filename) = Self::parse_file_marker(line) {
                if let Some(prev_file) = current_file.take() {
                    let content = current_content.join("\n");
                    files.insert(prev_file, content);
                    current_content.clear();
                } else {
                    // Everything before the first file marker is the comment
                    // Ensure comment ends cleanly
                }
                let fname_str = filename.to_string();
                file_order.push(fname_str.clone());
                current_file = Some(fname_str);
            } else if current_file.is_none() {
                comment_lines.push(line);
            } else {
                current_content.push(line);
            }
        }

        if let Some(last_file) = current_file {
            let mut content = current_content.join("\n");
            if input.ends_with('\n') && !content.is_empty() {
                content.push('\n');
            }
            files.insert(last_file, content);
        }

        let mut comment = comment_lines.join("\n");
        if !comment.is_empty() && (input.starts_with('\n') || comment_lines.len() > 1) {
            comment.push('\n');
        }

        Ok(TxtarArchive {
            comment,
            files,
            file_order,
        })
    }

    /// Read and parse a txtar archive from a file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, TxtarError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    fn parse_file_marker(line: &str) -> Option<&str> {
        let trimmed = line.trim();
        if trimmed.starts_with("-- ") && trimmed.ends_with(" --") && trimmed.len() > 6 {
            let inner = &trimmed[3..trimmed.len() - 3].trim();
            if !inner.is_empty() {
                return Some(inner);
            }
        }
        None
    }

    /// Get all CUE files from the archive in order.
    pub fn cue_files(&self) -> Vec<(&str, &str)> {
        self.file_order
            .iter()
            .filter(|name| name.ends_with(".cue"))
            .filter_map(|name| self.files.get(name).map(|c| (name.as_str(), c.as_str())))
            .collect()
    }

    /// Get the expected evaluation output if present (`out/eval` or `out/eval/stats`).
    pub fn expected_eval_output(&self) -> Option<&str> {
        self.files
            .get("out/eval")
            .or_else(|| self.files.get("out/eval/stats"))
            .map(|s| s.as_str())
    }

    /// Find all .txtar files in a directory.
    pub fn find_fixtures_in_dir<P: AsRef<Path>>(dir: P) -> Vec<PathBuf> {
        let mut fixtures = Vec::new();
        if !dir.as_ref().exists() {
            return fixtures;
        }
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("txtar") {
                fixtures.push(entry.path().to_path_buf());
            }
        }
        fixtures.sort();
        fixtures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_txtar() {
        let txt = r#"This is a test comment.
It explains the test case.
-- in.cue --
a: 1
b: 2
c: a + b
-- out/eval --
(int): {
  a: 1
  b: 2
  c: 3
}
"#;
        let archive = TxtarArchive::parse(txt).unwrap();
        assert!(archive.comment.contains("This is a test comment"));
        assert_eq!(archive.files.get("in.cue").unwrap().trim(), "a: 1\nb: 2\nc: a + b");
        assert_eq!(
            archive.expected_eval_output().unwrap().trim(),
            "(int): {\n  a: 1\n  b: 2\n  c: 3\n}"
        );
        assert_eq!(archive.cue_files().len(), 1);
        assert_eq!(archive.cue_files()[0].0, "in.cue");
    }

    #[test]
    fn test_parse_multiple_cue_files() {
        let txt = r#"-- file1.cue --
package test
x: 10
-- file2.cue --
package test
y: x * 2
-- out/eval --
x: 10
y: 20
"#;
        let archive = TxtarArchive::parse(txt).unwrap();
        assert_eq!(archive.file_order, vec!["file1.cue", "file2.cue", "out/eval"]);
        let cue_files = archive.cue_files();
        assert_eq!(cue_files.len(), 2);
        assert_eq!(cue_files[0].0, "file1.cue");
        assert_eq!(cue_files[1].0, "file2.cue");
    }
}
