//! Validated import identity: parse, don't validate.
//!
//! An import path that has been constructed is guaranteed non-empty and free
//! of control characters, so consumers never re-check that. Anything stricter
//! — absolute paths, `..` traversal — stays in `cue-eval::package`, which is
//! the only place that knows what is resolvable.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A decoded import path, e.g. `"strings"` or `"example.com/mod/pkg:qual"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackagePath(String);

/// An explicit import alias, e.g. the `s` in `import s "strings"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImportAlias(String);

/// Why a decoded import string is not a usable path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportPathError {
    Empty,
    ControlChar(char),
}

impl fmt::Display for ImportPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "import path must not be empty"),
            Self::ControlChar(c) => write!(f, "import path contains control character {c:?}"),
        }
    }
}

impl std::error::Error for ImportPathError {}

fn check(s: &str) -> Result<(), ImportPathError> {
    if s.is_empty() {
        return Err(ImportPathError::Empty);
    }
    if let Some(c) = s.chars().find(|c| c.is_control()) {
        return Err(ImportPathError::ControlChar(c));
    }
    Ok(())
}

impl PackagePath {
    /// Validate a decoded import path. Called by the parser only.
    pub fn new(path: String) -> Result<Self, ImportPathError> {
        check(&path)?;
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    /// Split the `path:qualifier` form: `("dir/path", Some("qual"))` or
    /// `(whole, None)`. The single owner of the `:` rule.
    pub fn split_qualifier(&self) -> (&str, Option<&str>) {
        match self.0.split_once(':') {
            Some((p, q)) => (p, Some(q)),
            None => (self.0.as_str(), None),
        }
    }

    /// The name an unaliased import binds: the qualifier, else the trailing
    /// path segment. The single owner of the default-alias rule.
    pub fn default_alias(&self) -> ImportAlias {
        let (dir_path, qualifier) = self.split_qualifier();
        let name = qualifier.unwrap_or_else(|| dir_path.split('/').next_back().unwrap_or(dir_path));
        ImportAlias(name.to_string())
    }
}

impl ImportAlias {
    /// Validate a decoded import alias. Called by the parser only.
    pub fn new(alias: String) -> Result<Self, ImportPathError> {
        check(&alias)?;
        Ok(Self(alias))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for ImportAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for PackagePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ImportAlias {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(PackagePath::new(String::new()), Err(ImportPathError::Empty));
        assert_eq!(ImportAlias::new(String::new()), Err(ImportPathError::Empty));
    }

    #[test]
    fn rejects_control_chars() {
        assert!(PackagePath::new("a\nb".to_string()).is_err());
    }

    #[test]
    fn qualifier_split() {
        let p = PackagePath::new("example.com/mod/pkg:qual".to_string()).unwrap();
        assert_eq!(p.split_qualifier(), ("example.com/mod/pkg", Some("qual")));
        let plain = PackagePath::new("strings".to_string()).unwrap();
        assert_eq!(plain.split_qualifier(), ("strings", None));
    }

    #[test]
    fn default_alias_rules() {
        let std = PackagePath::new("strings".to_string()).unwrap();
        assert_eq!(std.default_alias().as_str(), "strings");
        let nested = PackagePath::new("encoding/json".to_string()).unwrap();
        assert_eq!(nested.default_alias().as_str(), "json");
        let qualified =
            PackagePath::new("github.com/tonky/enve/schema/v1:devshell".to_string()).unwrap();
        assert_eq!(qualified.default_alias().as_str(), "devshell");
    }

    #[test]
    fn display_shows_the_path_verbatim() {
        let p = PackagePath::new("encoding/json".to_string()).unwrap();
        assert_eq!(p.to_string(), "encoding/json");
        assert_eq!(p.as_ref(), "encoding/json");
    }
}
