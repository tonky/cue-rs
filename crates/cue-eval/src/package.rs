use cue_syntax::ast::Decl;
use crate::eval::{EvalError, Evaluator};
use crate::value::{StructValue, Value, ValueId};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub module: String,
    pub language_version: Option<String>,
}

pub struct PackageLoader;

impl PackageLoader {
    /// Discovers the module root by searching parent directories for `cue.mod/module.cue`.
    pub fn find_module_root<P: AsRef<Path>>(start_dir: P) -> Option<(PathBuf, ModuleInfo)> {
        let mut curr = start_dir.as_ref().to_path_buf();
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
                            && let cue_syntax::ast::Expr::String(s) = &f.value {
                                mod_name = s.clone();
                            }
                        if f.label.name() == Some("language")
                            && let cue_syntax::ast::Expr::Struct(st) = &f.value {
                                for d in &st.decls {
                                    if let Decl::Field(inner_f) = d
                                        && inner_f.label.name() == Some("version")
                                        && let cue_syntax::ast::Expr::String(v) = &inner_f.value {
                                            lang_ver = Some(v.clone());
                                        }
                                }
                            }
                    }
                }
                if !mod_name.is_empty() {
                    return Some((curr, ModuleInfo {
                        module: mod_name,
                        language_version: lang_ver,
                    }));
                }
            }

            if !curr.pop() {
                break;
            }
        }
        None
    }

    /// Load and evaluate all .cue files in a directory as a unified package with multi-file hoisting.
    pub fn load_dir<P: AsRef<Path>>(dir: P) -> Result<(Evaluator, ValueId), EvalError> {
        let mut evaluator = Evaluator::new();
        let files = Self::find_cue_files(dir)?;

        if files.is_empty() {
            return Err(EvalError::Evaluation(
                "No .cue files found in directory".to_string(),
            ));
        }

        let mut parsed_files = Vec::new();
        for path in files {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| EvalError::Evaluation(format!("Failed to read {}: {e}", path.display())))?;
            let source_file = cue_syntax::parse_file(&content)?;
            parsed_files.push(source_file);
        }

        // Pass 1: Pre-register and evaluate definitions and aliases across all files
        for file in &parsed_files {
            for decl in &file.decls {
                if let Decl::Field(f) = decl {
                    if f.label.is_definition() {
                        let val_id = evaluator.eval_expr(&f.value)?;
                        if let Some(name) = f.label.name() {
                            evaluator.insert_binding(name, val_id);
                            if let Some(&p_id) = evaluator.placeholders.get(name)
                                && let Some(Value::RecursiveRef { target, .. }) =
                                    evaluator.arena.get_mut(p_id)
                                {
                                    *target = Some(val_id);
                                }
                        }
                    }
                } else if let Decl::Alias { ident, expr } = decl {
                    let val_id = evaluator.eval_expr(expr)?;
                    evaluator.insert_binding(ident, val_id);
                } else if let Decl::Let { ident, expr } = decl {
                    let val_id = evaluator.eval_expr(expr)?;
                    evaluator.insert_binding(ident, val_id);
                }
            }
        }

        // Pass 2: Evaluate all declarations from all files into package_struct
        let mut package_struct = StructValue::new(false);
        for file in parsed_files {
            evaluator.eval_decls_into_struct(&file.decls, &mut package_struct)?;
        }

        let root_id = evaluator.arena.alloc(Value::Struct(package_struct));
        Ok((evaluator, root_id))
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
}
