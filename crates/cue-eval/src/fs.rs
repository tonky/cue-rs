//! Filesystem port for package loading.
//!
//! [`PackageLoader`](crate::package::PackageLoader) reasons about CUE files;
//! this trait performs the I/O. Production passes [`StdFs`]; unit tests pass
//! [`MemFs`], so no test needs a real directory.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// The filesystem operations package loading needs. Symlink semantics match
/// `std::fs` / `Path`: existence checks follow links, `canonicalize`
/// resolves them.
pub trait FileProvider {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>>;
}

/// [`FileProvider`] backed by the real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFs;

impl FileProvider for StdFs {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            entries.push(entry?.path());
        }
        Ok(entries)
    }
}

/// [`FileProvider`] backed by an in-memory file map, for tests.
///
/// Keys are normalized lexically (`.`/`..` resolved, separators collapsed),
/// so `canonicalize` never touches disk. Only absolute paths canonicalize;
/// relative paths are an error. Directories are implied by the files under
/// them: empty directories do not exist here.
#[derive(Debug, Clone, Default)]
pub struct MemFs {
    files: HashMap<PathBuf, String>,
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a file, creating implied parent directories.
    pub fn write(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.insert(normalize(&path.into()), content.into());
    }

    fn get(&self, path: &Path) -> Option<&String> {
        self.files.get(&normalize(path))
    }

    fn children(&self, dir: &Path) -> HashSet<PathBuf> {
        let dir = normalize(dir);
        let mut out = HashSet::new();
        for file in self.files.keys() {
            if let Ok(rel) = file.strip_prefix(&dir)
                && let Some(first) = rel.components().next()
            {
                out.insert(dir.join(first));
            }
        }
        out
    }
}

/// Lexically clean an absolute path: skip `.`, resolve `..`, collapse repeats.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl FileProvider for MemFs {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        if path.is_absolute() {
            Ok(normalize(path))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "MemFs only canonicalizes absolute paths: {}",
                    path.display()
                ),
            ))
        }
    }

    fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.get(path).is_some()
    }

    fn is_dir(&self, path: &Path) -> bool {
        !self.children(path).is_empty()
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        self.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no such file: {}", path.display()),
            )
        })
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        let children = self.children(dir);
        if children.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no such directory: {}", dir.display()),
            ));
        }
        let mut sorted: Vec<PathBuf> = children.into_iter().collect();
        sorted.sort();
        Ok(sorted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implied_dirs_and_lexical_canonicalization() {
        let mut fs = MemFs::new();
        fs.write("/mem/mod/cue.mod/module.cue", "module: \"example.com/m\"");
        fs.write("/mem/mod/app/app.cue", "package app");
        assert!(fs.is_dir(Path::new("/mem/mod")));
        assert!(fs.is_dir(Path::new("/mem/mod/app")));
        assert!(!fs.is_dir(Path::new("/mem/mod/cue.mod/module.cue")));
        assert!(fs.is_file(Path::new("/mem/mod/./app/app.cue")));
        assert_eq!(
            fs.canonicalize(Path::new("/mem/mod/app/../app/app.cue"))
                .unwrap(),
            PathBuf::from("/mem/mod/app/app.cue")
        );
        assert!(fs.canonicalize(Path::new("relative/path")).is_err());
    }

    #[test]
    fn read_dir_lists_immediate_children() {
        let mut fs = MemFs::new();
        fs.write("/mem/app/a.cue", "package app");
        fs.write("/mem/app/b.cue", "package app");
        fs.write("/mem/app/nested/c.cue", "package app");
        assert_eq!(
            fs.read_dir(Path::new("/mem/app")).unwrap(),
            vec![
                PathBuf::from("/mem/app/a.cue"),
                PathBuf::from("/mem/app/b.cue"),
                PathBuf::from("/mem/app/nested"),
            ]
        );
        assert!(fs.read_dir(Path::new("/mem/missing")).is_err());
    }
}
