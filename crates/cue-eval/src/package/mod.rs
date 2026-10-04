mod annotate;
mod discovery;
mod model;

pub use model::{LoadOptions, ModuleInfo, OriginAnnotation};

use crate::eval::{EvalError, Evaluator};
use crate::fs::{FileProvider, StdFs};
use crate::value::ValueId;
use cue_syntax::ast::{Decl, ImportDecl, SourceFile};
use std::path::{Path, PathBuf};

pub struct PackageLoader;

impl PackageLoader {
    /// Discovers all ancestor module roots by searching parent directories for `cue.mod/module.cue`.
    pub fn find_all_module_roots<P: AsRef<Path>>(start_dir: P) -> Vec<(PathBuf, ModuleInfo)> {
        Self::find_all_module_roots_with_fs(start_dir, &StdFs)
    }

    /// [`Self::find_all_module_roots`], reading through `fs`.
    pub fn find_all_module_roots_with_fs<P: AsRef<Path>>(
        start_dir: P,
        fs: &dyn FileProvider,
    ) -> Vec<(PathBuf, ModuleInfo)> {
        discovery::find_all_module_roots_with_fs(start_dir.as_ref(), fs)
    }

    /// Discovers the nearest module root by searching parent directories for `cue.mod/module.cue`.
    pub fn find_module_root<P: AsRef<Path>>(start_dir: P) -> Option<(PathBuf, ModuleInfo)> {
        Self::find_module_root_with_fs(start_dir, &StdFs)
    }

    /// [`Self::find_module_root`], reading through `fs`.
    pub fn find_module_root_with_fs<P: AsRef<Path>>(
        start_dir: P,
        fs: &dyn FileProvider,
    ) -> Option<(PathBuf, ModuleInfo)> {
        Self::find_all_module_roots_with_fs(start_dir, fs)
            .into_iter()
            .next()
    }

    /// Load and evaluate a single .cue file with module and vendored package resolution.
    pub fn load_file<P: AsRef<Path>>(file: P) -> Result<(Evaluator, ValueId), EvalError> {
        Self::load_file_with(file, &LoadOptions::default())
    }

    /// [`Self::load_file`], with options.
    pub fn load_file_with<P: AsRef<Path>>(
        file: P,
        options: &LoadOptions,
    ) -> Result<(Evaluator, ValueId), EvalError> {
        Self::load_file_with_fs(file, options, &StdFs)
    }

    /// [`Self::load_file_with`], reading through `fs`.
    pub fn load_file_with_fs<P: AsRef<Path>>(
        file: P,
        options: &LoadOptions,
        fs: &dyn FileProvider,
    ) -> Result<(Evaluator, ValueId), EvalError> {
        let canonical_file = fs
            .canonicalize(file.as_ref())
            .unwrap_or_else(|_| file.as_ref().to_path_buf());
        let file_path = canonical_file.as_path();
        if !fs.is_file(file_path) {
            return Err(EvalError::Evaluation(format!(
                "File {} does not exist",
                file.as_ref().display()
            )));
        }

        let content = fs.read_to_string(file_path).map_err(|e| {
            EvalError::Evaluation(format!("Failed to read {}: {e}", file_path.display()))
        })?;
        let parsed_file = cue_syntax::parse_file(&content)?;

        let mut evaluator = Evaluator::new();
        evaluator.origin = options.origin.clone();
        let mod_roots = file_path
            .parent()
            .map(|parent| Self::find_all_module_roots_with_fs(parent, fs))
            .unwrap_or_default();

        Self::resolve_imports_for_files(
            std::slice::from_ref(&parsed_file),
            &mod_roots,
            &mut evaluator,
            fs,
        )?;

        let root_id = evaluator.eval_root_decls(&parsed_file.decls)?;
        let parent_dir = file_path.parent().unwrap_or_else(|| Path::new("."));
        annotate::annotate_origin(&mut evaluator, root_id, parent_dir);

        Ok((evaluator, root_id))
    }

    /// Load and evaluate all .cue files in a directory as a unified package with multi-file hoisting.
    pub fn load_dir<P: AsRef<Path>>(dir: P) -> Result<(Evaluator, ValueId), EvalError> {
        Self::load_dir_with_package(dir, None)
    }

    /// Load and evaluate all .cue files in a directory matching an optional package name.
    pub fn load_dir_with_package<P: AsRef<Path>>(
        dir: P,
        target_pkg: Option<&str>,
    ) -> Result<(Evaluator, ValueId), EvalError> {
        Self::load_dir_with(dir, target_pkg, &LoadOptions::default())
    }

    /// [`Self::load_dir_with_package`], with options.
    pub fn load_dir_with<P: AsRef<Path>>(
        dir: P,
        target_pkg: Option<&str>,
        options: &LoadOptions,
    ) -> Result<(Evaluator, ValueId), EvalError> {
        Self::load_dir_with_fs(dir, target_pkg, options, &StdFs)
    }

    /// [`Self::load_dir_with`], reading through `fs`.
    pub fn load_dir_with_fs<P: AsRef<Path>>(
        dir: P,
        target_pkg: Option<&str>,
        options: &LoadOptions,
        fs: &dyn FileProvider,
    ) -> Result<(Evaluator, ValueId), EvalError> {
        let mut evaluator = Evaluator::new();
        evaluator.origin = options.origin.clone();
        let root_id = Self::load_dir_into(dir.as_ref(), target_pkg, &mut evaluator, fs)?;
        Ok((evaluator, root_id))
    }

    /// Load a package into an evaluator the caller owns, so every value it
    /// allocates - and every recipe its fields carry - lives in that arena.
    fn load_dir_into(
        dir: &Path,
        target_pkg: Option<&str>,
        evaluator: &mut Evaluator,
        fs: &dyn FileProvider,
    ) -> Result<ValueId, EvalError> {
        let files = Self::find_cue_files(dir, fs)?;
        if files.is_empty() {
            return Err(EvalError::Evaluation(
                "No .cue files found in directory".to_string(),
            ));
        }

        let parsed_files = read_and_filter_files(&files, target_pkg, fs)?;
        if parsed_files.is_empty() {
            return Err(EvalError::Evaluation(format!(
                "No .cue files found in directory {}",
                dir.display()
            )));
        }

        let mod_roots = Self::find_all_module_roots_with_fs(dir, fs);
        Self::resolve_imports_for_files(&parsed_files, &mod_roots, evaluator, fs)?;

        let all_decls: Vec<Decl> = parsed_files.into_iter().flat_map(|f| f.decls).collect();
        let root_id = evaluator.eval_root_decls(&all_decls)?;
        let canonical_dir = fs.canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        annotate::annotate_origin(evaluator, root_id, &canonical_dir);

        Ok(root_id)
    }

    fn resolve_imports_for_files(
        files: &[SourceFile],
        mod_roots: &[(PathBuf, ModuleInfo)],
        evaluator: &mut Evaluator,
        fs: &dyn FileProvider,
    ) -> Result<(), EvalError> {
        for file in files {
            for imp in &file.imports {
                Self::resolve_single_import(imp, mod_roots, evaluator, fs)?;
            }
        }
        Ok(())
    }

    fn resolve_single_import(
        imp: &ImportDecl,
        mod_roots: &[(PathBuf, ModuleInfo)],
        evaluator: &mut Evaluator,
        fs: &dyn FileProvider,
    ) -> Result<(), EvalError> {
        let (dir_path, pkg_qualifier) = imp.path.split_qualifier();
        let alias = imp
            .alias
            .clone()
            .unwrap_or_else(|| imp.path.default_alias())
            .into_inner();

        std::rc::Rc::make_mut(&mut evaluator.imports)
            .aliases
            .insert(alias, imp.path.as_str().to_string());

        if evaluator.imports.packages.contains_key(imp.path.as_str()) {
            return Ok(());
        }

        if discovery::is_invalid_import_path(dir_path) {
            return Ok(());
        }

        let mut load_error = None;
        for (mod_root, mod_info) in mod_roots {
            let Some(pkg_dir) = discovery::find_pkg_dir_in_root(dir_path, mod_root, mod_info, fs)
            else {
                continue;
            };

            let mut sub = Evaluator::with_arena(std::mem::take(&mut evaluator.arena));
            sub.origin = evaluator.origin.clone();
            let loaded = Self::load_dir_into(&pkg_dir, pkg_qualifier, &mut sub, fs);
            evaluator.arena = std::mem::take(&mut sub.arena);

            match loaded {
                Ok(package_id) => {
                    std::rc::Rc::make_mut(&mut evaluator.imports)
                        .packages
                        .insert(imp.path.as_str().to_string(), package_id);
                    return Ok(());
                }
                Err(error) => load_error = Some(error),
            }
        }

        if let Some(error) = load_error {
            return Err(EvalError::Evaluation(format!(
                "import \"{}\": {error}",
                imp.path
            )));
        }
        Ok(())
    }

    fn find_cue_files<P: AsRef<Path>>(
        dir: P,
        fs: &dyn FileProvider,
    ) -> Result<Vec<PathBuf>, EvalError> {
        if !fs.exists(dir.as_ref()) {
            return Err(EvalError::Evaluation(format!(
                "Directory {} does not exist",
                dir.as_ref().display()
            )));
        }

        let read_dir = fs
            .read_dir(dir.as_ref())
            .map_err(|e| EvalError::Evaluation(format!("Failed to read dir: {e}")))?;

        let mut files: Vec<PathBuf> = read_dir
            .into_iter()
            .filter(|path| {
                fs.is_file(path) && path.extension().and_then(|s| s.to_str()) == Some("cue")
            })
            .collect();

        files.sort();
        Ok(files)
    }
}

fn read_and_filter_files(
    files: &[PathBuf],
    target_pkg: Option<&str>,
    fs: &dyn FileProvider,
) -> Result<Vec<SourceFile>, EvalError> {
    let mut parsed_files = Vec::with_capacity(files.len());
    let mut all_files = Vec::with_capacity(files.len());

    for path in files {
        let content = fs.read_to_string(path).map_err(|e| {
            EvalError::Evaluation(format!("Failed to read {}: {e}", path.display()))
        })?;
        let source_file = cue_syntax::parse_file(&content)?;

        let matches_pkg = match (target_pkg, &source_file.package) {
            (Some(target), Some(pkg)) => target == pkg,
            _ => true,
        };
        if matches_pkg {
            parsed_files.push(source_file.clone());
        }
        all_files.push(source_file);
    }

    if parsed_files.is_empty() {
        Ok(all_files)
    } else {
        Ok(parsed_files)
    }
}
