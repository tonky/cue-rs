use crate::eval::{EvalError, Evaluator};
use crate::value::{Value, ValueId};
use cue_syntax::ast::Decl;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub module: String,
    pub language_version: Option<String>,
}

pub struct PackageLoader;

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

/// How [`PackageLoader`] loads a file or package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadOptions {
    pub origin: Option<OriginAnnotation>,
}

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
        Self::load_file_with(file, &LoadOptions::default())
    }

    /// [`Self::load_file`], with options.
    pub fn load_file_with<P: AsRef<Path>>(
        file: P,
        options: &LoadOptions,
    ) -> Result<(Evaluator, ValueId), EvalError> {
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
        evaluator.origin = options.origin.clone();
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

        let root_id = evaluator.eval_root_decls(&parsed_file.decls)?;
        let parent_dir = file_path.parent().unwrap_or_else(|| Path::new("."));
        Self::annotate_origin(&mut evaluator, root_id, parent_dir);

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
        let mut evaluator = Evaluator::new();
        evaluator.origin = options.origin.clone();
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
        let root_id = evaluator.eval_root_decls(&all_decls)?;
        let canonical_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        Self::annotate_origin(evaluator, root_id, &canonical_dir);

        Ok(root_id)
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

                // A package found but failing to load is reported as itself,
                // not later as a reference to an alias nothing bound.
                let mut load_error = None;
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
                        sub.origin = evaluator.origin.clone();
                        let loaded = Self::load_dir_into(&p_dir, pkg_qualifier, &mut sub);
                        evaluator.arena = std::mem::take(&mut sub.arena);
                        match loaded {
                            Ok(package_id) => {
                                std::rc::Rc::make_mut(&mut evaluator.imports)
                                    .packages
                                    .insert(imp.path.clone(), package_id);
                                load_error = None;
                                break;
                            }
                            Err(error) => load_error = Some(error),
                        }
                    }
                }
                if let Some(error) = load_error {
                    return Err(EvalError::Evaluation(format!(
                        "import \"{}\": {error}",
                        imp.path
                    )));
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

    fn annotate_origin(evaluator: &mut Evaluator, root_id: ValueId, origin_dir: &Path) {
        let Some(annotation) = evaluator.origin.clone() else {
            return;
        };
        let Some(Value::Struct(mut root)) = evaluator.arena.get(root_id).cloned() else {
            return;
        };
        let origin = evaluator
            .arena
            .string(origin_dir.to_string_lossy().to_string());
        let mut done = std::collections::HashMap::new();
        for entry in root.fields.values_mut() {
            entry.val = annotate(evaluator, entry.val, &annotation, origin, &mut done);
        }
        if let Some(value) = evaluator.arena.get_mut(root_id) {
            *value = Value::Struct(root);
        }
    }
}

/// `id` with every struct below it that carries the marker annotated, copied
/// on write: a node another value shares - a definition's closed copy, an
/// imported package's field - is never changed in place.
fn annotate(
    evaluator: &mut Evaluator,
    id: ValueId,
    annotation: &OriginAnnotation,
    origin: ValueId,
    done: &mut std::collections::HashMap<ValueId, ValueId>,
) -> ValueId {
    if let Some(&copy) = done.get(&id) {
        return copy;
    }
    // A cyclic graph reaches a node again before its copy exists.
    done.insert(id, id);
    let annotated = match evaluator.arena.get(id).cloned() {
        Some(Value::Struct(mut s)) => {
            let mut changed = false;
            for entry in s.fields.values_mut() {
                let val = annotate(evaluator, entry.val, annotation, origin, done);
                changed |= val != entry.val;
                entry.val = val;
            }
            // An optional declaration (`originDir?: string`) is a constraint, not
            // a value: only a regular field the author set wins.
            let set = s.fields.get(&annotation.field).is_some_and(|f| !f.optional);
            if s.hidden.contains_key(&annotation.marker) && !set {
                s.insert_field(annotation.field.clone(), origin, false);
                changed = true;
            }
            if changed {
                evaluator.arena.alloc_like(id, Value::Struct(s))
            } else {
                id
            }
        }
        Some(Value::Disjunction { branches }) => {
            let annotated: Vec<crate::value::DisjunctionBranch> = branches
                .iter()
                .map(|b| crate::value::DisjunctionBranch {
                    default: b.default,
                    val: annotate(evaluator, b.val, annotation, origin, done),
                })
                .collect();
            if annotated == branches {
                id
            } else {
                evaluator.arena.alloc_like(
                    id,
                    Value::Disjunction {
                        branches: annotated,
                    },
                )
            }
        }
        Some(Value::List { elements, ellipsis }) => {
            let annotated: Vec<ValueId> = elements
                .iter()
                .map(|&e| annotate(evaluator, e, annotation, origin, done))
                .collect();
            if annotated == elements {
                id
            } else {
                evaluator.arena.alloc_like(
                    id,
                    Value::List {
                        elements: annotated,
                        ellipsis,
                    },
                )
            }
        }
        _ => id,
    };
    done.insert(id, annotated);
    annotated
}
