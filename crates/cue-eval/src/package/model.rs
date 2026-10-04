#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub module: String,
    pub language_version: Option<String>,
}

/// Record the directory each value was declared in, for the values that ask.
///
/// A struct carrying the hidden field `marker` gets the regular field `field`
/// set to the canonical directory of the package - or, for a single file, the
/// file's directory - it was declared in. A schema opts its values in by
/// declaring the marker, so nothing else in the tree is touched. The first
/// package to reach a value wins: an imported value keeps its own directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginAnnotation {
    pub marker: String,
    pub field: String,
}

/// How `PackageLoader` loads a file or package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadOptions {
    pub origin: Option<OriginAnnotation>,
}
