use crate::eval::{EvalError, Evaluator};
use crate::value::{StructValue, Value, ValueId};
use cue_syntax::ast::Decl;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub module: String,
    pub language_version: Option<String>,
}

pub struct PackageLoader;

impl PackageLoader {
    /// Discovers all ancestor module roots by searching parent directories for `cue.mod/module.cue`.
    pub fn find_all_module_roots<P: AsRef<Path>>(start_dir: P) -> Vec<(PathBuf, ModuleInfo)> {
        let mut roots = Vec::new();
        let start = if start_dir.as_ref().as_os_str().is_empty() {
            Path::new(".")
        } else {
            start_dir.as_ref()
        };
        let Ok(canonical) = std::fs::canonicalize(start) else {
            return roots;
        };
        let mut curr = canonical;
        if curr.is_file() {
            curr.pop();
        }

        loop {
            let mod_cue = curr.join("cue.mod").join("module.cue");
            if mod_cue.is_file()
                && let Ok(content) = std::fs::read_to_string(&mod_cue)
                && let Ok(source) = cue_syntax::parse_file(&content)
            {
                let mut mod_name = String::new();
                let mut lang_ver = None;
                for decl in &source.decls {
                    if let Decl::Field(f) = decl {
                        if f.label.name() == Some("module")
                            && let cue_syntax::ast::Expr::String(s) = &f.value
                        {
                            mod_name = s.value.clone();
                        }
                        if f.label.name() == Some("language")
                            && let cue_syntax::ast::Expr::Struct(st) = &f.value
                        {
                            for d in &st.decls {
                                if let Decl::Field(inner_f) = d
                                    && inner_f.label.name() == Some("version")
                                    && let cue_syntax::ast::Expr::String(v) = &inner_f.value
                                {
                                    lang_ver = Some(v.value.clone());
                                }
                            }
                        }
                    }
                }
                if !mod_name.is_empty() {
                    roots.push((
                        curr.clone(),
                        ModuleInfo {
                            module: mod_name,
                            language_version: lang_ver,
                        },
                    ));
                }
            }

            if !curr.pop() {
                break;
            }
        }
        roots
    }

    /// Discovers the nearest module root by searching parent directories for `cue.mod/module.cue`.
    pub fn find_module_root<P: AsRef<Path>>(start_dir: P) -> Option<(PathBuf, ModuleInfo)> {
        Self::find_all_module_roots(start_dir).into_iter().next()
    }

    /// Load and evaluate a single .cue file with module and vendored package resolution.
    pub fn load_file<P: AsRef<Path>>(file: P) -> Result<(Evaluator, ValueId), EvalError> {
        let canonical_file =
            std::fs::canonicalize(file.as_ref()).unwrap_or_else(|_| file.as_ref().to_path_buf());
        let file_path = canonical_file.as_path();
        if !file_path.is_file() {
            return Err(EvalError::Evaluation(format!(
                "File {} does not exist",
                file.as_ref().display()
            )));
        }

        let content = std::fs::read_to_string(file_path).map_err(|e| {
            EvalError::Evaluation(format!("Failed to read {}: {e}", file_path.display()))
        })?;
        let parsed_file = cue_syntax::parse_file(&content)?;

        let mut evaluator = Evaluator::new();
        let mod_roots = file_path
            .parent()
            .map(Self::find_all_module_roots)
            .unwrap_or_default();

        // Resolve imports in file
        Self::resolve_imports_for_files(
            std::slice::from_ref(&parsed_file),
            &mod_roots,
            &mut evaluator,
        )?;

        let mut root_struct = StructValue::new(false);
        evaluator.eval_decls_into_struct(&parsed_file.decls, &mut root_struct)?;
        let parent_dir = file_path.parent().unwrap_or_else(|| Path::new("."));
        Self::attach_origin_dir_to_structs(&mut evaluator, &mut root_struct, parent_dir);

        let root_id = evaluator.arena.alloc(Value::Struct(root_struct));
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
        let mut evaluator = Evaluator::new();
        let root_id = Self::load_dir_into(dir.as_ref(), target_pkg, &mut evaluator)?;
        Ok((evaluator, root_id))
    }

    /// Load a package into an evaluator the caller owns, so every value it
    /// allocates - and every recipe its fields carry - lives in that arena.
    ///
    /// The package gets its own `Evaluator`, because a scope stack, a
    /// placeholder table and an import set are per package. It shares only the
    /// arena, which is what keeps an imported field's conjuncts usable at the
    /// importer's merge.
    fn load_dir_into(
        dir: &Path,
        target_pkg: Option<&str>,
        evaluator: &mut Evaluator,
    ) -> Result<ValueId, EvalError> {
        let files = Self::find_cue_files(dir)?;

        if files.is_empty() {
            return Err(EvalError::Evaluation(
                "No .cue files found in directory".to_string(),
            ));
        }

        let mut parsed_files = Vec::new();
        let mut all_files = Vec::new();
        for path in files {
            let content = std::fs::read_to_string(&path).map_err(|e| {
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

        let parsed_files = if parsed_files.is_empty() {
            all_files
        } else {
            parsed_files
        };

        if parsed_files.is_empty() {
            return Err(EvalError::Evaluation(format!(
                "No .cue files found in directory {}",
                dir.display()
            )));
        }

        let mod_roots = Self::find_all_module_roots(dir);

        // Process imports across parsed files and resolve external/module packages
        Self::resolve_imports_for_files(&parsed_files, &mod_roots, evaluator)?;

        let all_decls: Vec<Decl> = parsed_files.into_iter().flat_map(|f| f.decls).collect();
        let mut package_struct = StructValue::new(false);
        evaluator.eval_decls_into_struct(&all_decls, &mut package_struct)?;
        let canonical_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        Self::attach_origin_dir_to_structs(evaluator, &mut package_struct, &canonical_dir);

        Ok(evaluator.arena.alloc(Value::Struct(package_struct)))
    }

    fn resolve_imports_for_files(
        files: &[cue_syntax::ast::SourceFile],
        mod_roots: &[(PathBuf, ModuleInfo)],
        evaluator: &mut Evaluator,
    ) -> Result<(), EvalError> {
        for file in files {
            for imp in &file.imports {
                let (dir_path, pkg_qualifier) = match imp.path.split_once(':') {
                    Some((p, q)) => (p, Some(q)),
                    None => (imp.path.as_str(), None),
                };

                let default_alias = pkg_qualifier
                    .unwrap_or_else(|| dir_path.split('/').next_back().unwrap_or(dir_path));
                let alias = imp
                    .alias
                    .clone()
                    .unwrap_or_else(|| default_alias.to_string());
                std::rc::Rc::make_mut(&mut evaluator.imports)
                    .aliases
                    .insert(alias, imp.path.clone());

                if evaluator.imports.packages.contains_key(&imp.path) {
                    continue;
                }

                // Disallow paths attempting directory traversal or absolute paths escaping the module
                let p_check = Path::new(dir_path);
                if p_check.is_absolute()
                    || p_check.components().any(|c| {
                        matches!(
                            c,
                            std::path::Component::ParentDir
                                | std::path::Component::RootDir
                                | std::path::Component::Prefix(_)
                        )
                    })
                {
                    continue;
                }

                for (mod_root, mod_info) in mod_roots {
                    let mod_base = mod_info
                        .module
                        .split('@')
                        .next()
                        .unwrap_or(&mod_info.module);
                    let pkg_dir = if let Some(stripped) = dir_path.strip_prefix(mod_base) {
                        let sub = stripped.trim_start_matches('/');
                        let p = mod_root.join(sub);
                        if p.is_dir() { Some(p) } else { None }
                    } else {
                        None
                    };

                    let pkg_dir = pkg_dir.or_else(|| {
                        let vendored = mod_root.join("cue.mod").join("pkg").join(dir_path);
                        let gen_dir = mod_root.join("cue.mod").join("gen").join(dir_path);
                        let usr = mod_root.join("cue.mod").join("usr").join(dir_path);

                        if vendored.is_dir() {
                            Some(vendored)
                        } else if gen_dir.is_dir() {
                            Some(gen_dir)
                        } else if usr.is_dir() {
                            Some(usr)
                        } else {
                            None
                        }
                    });

                    if let Some(p_dir) = pkg_dir
                        && p_dir.is_dir()
                    {
                        // The package is evaluated into this evaluator's arena, so
                        // its values keep their ids here and the recipes its fields
                        // carry survive the import. The arena comes back either way:
                        // a package that fails to load must not take it with it.
                        let mut sub = Evaluator::with_arena(std::mem::take(&mut evaluator.arena));
                        let loaded = Self::load_dir_into(&p_dir, pkg_qualifier, &mut sub);
                        evaluator.arena = std::mem::take(&mut sub.arena);
                        if let Ok(package_id) = loaded {
                            std::rc::Rc::make_mut(&mut evaluator.imports)
                                .packages
                                .insert(imp.path.clone(), package_id);
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn find_cue_files<P: AsRef<Path>>(dir: P) -> Result<Vec<PathBuf>, EvalError> {
        let mut files = Vec::new();
        if !dir.as_ref().exists() {
            return Err(EvalError::Evaluation(format!(
                "Directory {} does not exist",
                dir.as_ref().display()
            )));
        }

        let read_dir = std::fs::read_dir(dir.as_ref())
            .map_err(|e| EvalError::Evaluation(format!("Failed to read dir: {e}")))?;

        for entry in read_dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("cue") {
                files.push(path);
            }
        }

        files.sort();
        Ok(files)
    }

    fn attach_origin_dir_to_structs(
        evaluator: &mut Evaluator,
        root: &mut StructValue,
        origin_dir: &Path,
    ) {
        let dir_str = origin_dir.to_string_lossy();
        let origin_id = evaluator.arena.string(dir_str.to_string());

        let mut queue = Vec::new();
        for entry in root.fields.values() {
            queue.push(entry.val);
        }

        let mut visited = std::collections::HashSet::new();
        while let Some(vid) = queue.pop() {
            if !visited.insert(vid) {
                continue;
            }
            if let Some(Value::Struct(s)) = evaluator.arena.get_mut(vid) {
                let is_service = s.fields.contains_key("command")
                    || s.fields.contains_key("image")
                    || s.fields.contains_key("readinessProbe")
                    || s.fields.contains_key("healthCheck")
                    || (s.fields.contains_key("name") && s.fields.contains_key("port"))
                    || s.fields.contains_key("lifecycle");

                if is_service && !s.fields.contains_key("originDir") {
                    s.insert_field("originDir".to_string(), origin_id, false);
                }

                for entry in s.fields.values() {
                    queue.push(entry.val);
                }
            }
        }
    }
}
