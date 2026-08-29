use cue_syntax::ast::Decl;
use crate::eval::{EvalError, Evaluator};
use crate::value::{DisjunctionBranch, FieldEntry, PatternConstraint, StructValue, Value, ValueArena, ValueId};
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
        let files = Self::find_cue_files(dir.as_ref())?;

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

        let mod_root_opt = Self::find_module_root(dir.as_ref());

        // Process imports across parsed files and resolve external/module packages
        for file in &parsed_files {
            for imp in &file.imports {
                let alias = imp
                    .alias
                    .clone()
                    .unwrap_or_else(|| imp.path.split('/').next_back().unwrap_or(&imp.path).to_string());
                evaluator.import_aliases.insert(alias, imp.path.clone());

                if let Some((ref mod_root, ref mod_info)) = mod_root_opt
                    && !evaluator.imported_packages.contains_key(&imp.path) {
                        let pkg_dir = if imp.path.starts_with(&mod_info.module) {
                            let sub = imp.path[mod_info.module.len()..].trim_start_matches('/');
                            Some(mod_root.join(sub))
                        } else {
                            let vendored = mod_root.join("cue.mod").join("pkg").join(&imp.path);
                            if vendored.is_dir() {
                                Some(vendored)
                            } else {
                                None
                            }
                        };

                        if let Some(p_dir) = pkg_dir
                            && p_dir.is_dir()
                            && let Ok((sub_eval, sub_val)) = Self::load_dir(&p_dir) {
                                let imported_id = clone_value_into(&sub_eval.arena, &mut evaluator.arena, sub_val);
                                evaluator.imported_packages.insert(imp.path.clone(), imported_id);
                            }
                    }
            }
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

fn clone_value_into(
    from_arena: &ValueArena,
    to_arena: &mut ValueArena,
    id: ValueId,
) -> ValueId {
    let Some(val) = from_arena.get(id) else {
        return to_arena.alloc(Value::Top);
    };
    match val {
        Value::Top => to_arena.alloc(Value::Top),
        Value::Bottom(msg) => to_arena.alloc(Value::Bottom(msg.clone())),
        Value::Null => to_arena.alloc(Value::Null),
        Value::Bool(b) => to_arena.alloc(Value::Bool(*b)),
        Value::Int(i) => to_arena.alloc(Value::Int(i.clone())),
        Value::Float(f) => to_arena.alloc(Value::Float(*f)),
        Value::String(s) => to_arena.alloc(Value::String(s.clone())),
        Value::Bytes(b) => to_arena.alloc(Value::Bytes(b.clone())),
        Value::Type(t) => to_arena.alloc(Value::Type(*t)),
        Value::Bounds {
            base_type,
            constraints,
        } => {
            let cloned_constraints = constraints
                .iter()
                .map(|(op, v)| (*op, clone_value_into(from_arena, to_arena, *v)))
                .collect();
            to_arena.alloc(Value::Bounds {
                base_type: *base_type,
                constraints: cloned_constraints,
            })
        }
        Value::List { elements, ellipsis } => {
            let cloned_elems = elements
                .iter()
                .map(|&e| clone_value_into(from_arena, to_arena, e))
                .collect();
            let cloned_el = ellipsis.map(|e| clone_value_into(from_arena, to_arena, e));
            to_arena.alloc(Value::List {
                elements: cloned_elems,
                ellipsis: cloned_el,
            })
        }
        Value::Struct(s) => {
            let mut new_s = StructValue::new(s.is_closed);
            for (k, entry) in &s.fields {
                new_s.fields.insert(
                    k.clone(),
                    FieldEntry {
                        val: clone_value_into(from_arena, to_arena, entry.val),
                        optional: entry.optional,
                    },
                );
            }
            for (k, entry) in &s.definitions {
                new_s.definitions.insert(
                    k.clone(),
                    FieldEntry {
                        val: clone_value_into(from_arena, to_arena, entry.val),
                        optional: entry.optional,
                    },
                );
            }
            for (k, entry) in &s.hidden {
                new_s.hidden.insert(
                    k.clone(),
                    FieldEntry {
                        val: clone_value_into(from_arena, to_arena, entry.val),
                        optional: entry.optional,
                    },
                );
            }
            for pc in &s.pattern_constraints {
                new_s.pattern_constraints.push(PatternConstraint {
                    pattern_val: clone_value_into(from_arena, to_arena, pc.pattern_val),
                    target_val: clone_value_into(from_arena, to_arena, pc.target_val),
                });
            }
            to_arena.alloc(Value::Struct(new_s))
        }
        Value::Disjunction { branches } => {
            let cloned_branches = branches
                .iter()
                .map(|b| DisjunctionBranch {
                    val: clone_value_into(from_arena, to_arena, b.val),
                    default: b.default,
                })
                .collect();
            to_arena.alloc(Value::Disjunction {
                branches: cloned_branches,
            })
        }
        Value::BuiltinValidator { name, target } => {
            let cloned_target = clone_value_into(from_arena, to_arena, *target);
            to_arena.alloc(Value::BuiltinValidator {
                name: name.clone(),
                target: cloned_target,
            })
        }
        Value::Validators(vec) => {
            let cloned_vec = vec
                .iter()
                .map(|&v| clone_value_into(from_arena, to_arena, v))
                .collect();
            to_arena.alloc(Value::Validators(cloned_vec))
        }
        Value::RecursiveRef { name, target } => {
            let cloned_target = target.map(|t| clone_value_into(from_arena, to_arena, t));
            to_arena.alloc(Value::RecursiveRef {
                name: name.clone(),
                target: cloned_target,
            })
        }
    }
}

