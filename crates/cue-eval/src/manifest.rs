use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DependencyInfo {
    pub version: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleManifest {
    pub module: String,
    pub language_version: String,
    pub dependencies: BTreeMap<String, DependencyInfo>,
}

impl Default for ModuleManifest {
    fn default() -> Self {
        Self {
            module: String::new(),
            language_version: "v0.12.0".to_string(),
            dependencies: BTreeMap::new(),
        }
    }
}

impl ModuleManifest {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module: module_name.into(),
            language_version: "v0.12.0".to_string(),
            dependencies: BTreeMap::new(),
        }
    }

    /// Initialize a new CUE module in the specified directory.
    pub fn init<P: AsRef<Path>>(root_dir: P, module_name: &str) -> Result<PathBuf, String> {
        let root = root_dir.as_ref();
        let cue_mod_dir = root.join("cue.mod");
        let pkg_dir = cue_mod_dir.join("pkg");
        let usr_dir = cue_mod_dir.join("usr");

        fs::create_dir_all(&pkg_dir).map_err(|e| format!("Failed to create pkg directory: {e}"))?;
        fs::create_dir_all(&usr_dir).map_err(|e| format!("Failed to create usr directory: {e}"))?;

        let manifest = ModuleManifest::new(module_name);
        let manifest_file = cue_mod_dir.join("module.cue");
        fs::write(&manifest_file, manifest.to_cue_string())
            .map_err(|e| format!("Failed to write module.cue: {e}"))?;

        Ok(manifest_file)
    }

    /// Serialize the manifest into canonical CUE source code.
    pub fn to_cue_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("module: \"{}\"\n", self.module));
        out.push_str("language: {\n");
        out.push_str(&format!("\tversion: \"{}\"\n", self.language_version));
        out.push_str("}\n");

        if !self.dependencies.is_empty() {
            out.push_str("deps: {\n");
            for (dep_name, info) in &self.dependencies {
                out.push_str(&format!("\t\"{}\": {{\n", dep_name));
                out.push_str(&format!("\t\tv: \"{}\"\n", info.version));
                if let Some(src) = &info.source {
                    out.push_str(&format!("\t\tsource: \"{}\"\n", src));
                }
                out.push_str("\t}\n");
            }
            out.push_str("}\n");
        }

        out
    }

    /// Parse a `cue.mod/module.cue` source file into a `ModuleManifest`.
    pub fn from_cue_string(source: &str) -> Result<Self, String> {
        let file = cue_syntax::parse_file(source)
            .map_err(|e| format!("Parse error in module.cue: {e}"))?;

        let mut manifest = ModuleManifest::default();

        for decl in &file.decls {
            if let cue_syntax::ast::Decl::Field(f) = decl {
                match f.label.name() {
                    Some("module") => {
                        if let cue_syntax::ast::Expr::String(s) = &f.value {
                            manifest.module = s.clone();
                        }
                    }
                    Some("language") => {
                        if let cue_syntax::ast::Expr::Struct(st) = &f.value {
                            for d in &st.decls {
                                if let cue_syntax::ast::Decl::Field(inner) = d
                                    && inner.label.name() == Some("version")
                                    && let cue_syntax::ast::Expr::String(v) = &inner.value
                                {
                                    manifest.language_version = v.clone();
                                }
                            }
                        }
                    }
                    Some("deps") => {
                        if let cue_syntax::ast::Expr::Struct(st) = &f.value {
                            for d in &st.decls {
                                if let cue_syntax::ast::Decl::Field(dep_f) = d
                                    && let Some(dep_name) = dep_f.label.name()
                                    && let cue_syntax::ast::Expr::Struct(dep_st) = &dep_f.value
                                {
                                    let mut dep_info = DependencyInfo::default();
                                    for dd in &dep_st.decls {
                                        if let cue_syntax::ast::Decl::Field(df) = dd {
                                            match df.label.name() {
                                                Some("v") => {
                                                    if let cue_syntax::ast::Expr::String(v) = &df.value {
                                                        dep_info.version = v.clone();
                                                    }
                                                }
                                                Some("source") => {
                                                    if let cue_syntax::ast::Expr::String(s) = &df.value {
                                                        dep_info.source = Some(s.clone());
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    manifest.dependencies.insert(dep_name.to_string(), dep_info);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if manifest.module.is_empty() {
            return Err("Missing 'module' field in module.cue".to_string());
        }

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_roundtrip() {
        let mut manifest = ModuleManifest::new("example.com/mymod@v0");
        manifest.language_version = "v0.12.0".to_string();
        manifest.dependencies.insert(
            "github.com/org/schema@v0".to_string(),
            DependencyInfo {
                version: "v0.1.0".to_string(),
                source: Some("oci://registry.example.com/schema".to_string()),
            },
        );

        let cue_str = manifest.to_cue_string();
        let parsed = ModuleManifest::from_cue_string(&cue_str).unwrap();
        assert_eq!(parsed.module, "example.com/mymod@v0");
        assert_eq!(parsed.language_version, "v0.12.0");
        assert_eq!(parsed.dependencies.len(), 1);
        let dep = parsed.dependencies.get("github.com/org/schema@v0").unwrap();
        assert_eq!(dep.version, "v0.1.0");
        assert_eq!(dep.source.as_deref(), Some("oci://registry.example.com/schema"));
    }
}
