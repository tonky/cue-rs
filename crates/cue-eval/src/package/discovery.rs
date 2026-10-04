use super::model::ModuleInfo;
use crate::fs::FileProvider;
use cue_syntax::ast::{Decl, SourceFile, StructLit};
use std::path::{Component, Path, PathBuf};

pub(crate) fn find_all_module_roots_with_fs(
    start_dir: &Path,
    fs: &dyn FileProvider,
) -> Vec<(PathBuf, ModuleInfo)> {
    let mut roots = Vec::new();
    let start = if start_dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        start_dir
    };
    let Ok(canonical) = fs.canonicalize(start) else {
        return roots;
    };
    let mut curr = canonical;
    if fs.is_file(&curr) {
        curr.pop();
    }

    loop {
        let mod_cue = curr.join("cue.mod").join("module.cue");
        if fs.is_file(&mod_cue)
            && let Ok(content) = fs.read_to_string(&mod_cue)
            && let Ok(source) = cue_syntax::parse_file(&content)
            && let Some(info) = parse_module_info(&source)
        {
            roots.push((curr.clone(), info));
        }

        if !curr.pop() {
            break;
        }
    }
    roots
}

pub(crate) fn parse_module_info(source: &SourceFile) -> Option<ModuleInfo> {
    let mut mod_name = None;
    let mut lang_ver = None;

    for decl in &source.decls {
        let Decl::Field(f) = decl else { continue };
        match f.label.name() {
            Some("module") => {
                if let cue_syntax::ast::Expr::String(s) = &f.value {
                    mod_name = Some(s.value.clone());
                }
            }
            Some("language") => {
                if let cue_syntax::ast::Expr::Struct(st) = &f.value {
                    lang_ver = extract_version_from_struct(st);
                }
            }
            _ => {}
        }
    }

    mod_name.map(|module| ModuleInfo {
        module,
        language_version: lang_ver,
    })
}

fn extract_version_from_struct(st: &StructLit) -> Option<String> {
    st.decls.iter().find_map(|d| match d {
        Decl::Field(f) if f.label.name() == Some("version") => match &f.value {
            cue_syntax::ast::Expr::String(v) => Some(v.value.clone()),
            _ => None,
        },
        _ => None,
    })
}

pub(crate) fn is_invalid_import_path(dir_path: &str) -> bool {
    let p_check = Path::new(dir_path);
    p_check.is_absolute()
        || p_check.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

pub(crate) fn find_pkg_dir_in_root(
    dir_path: &str,
    mod_root: &Path,
    mod_info: &ModuleInfo,
    fs: &dyn FileProvider,
) -> Option<PathBuf> {
    let mod_base = mod_info
        .module
        .split('@')
        .next()
        .unwrap_or(&mod_info.module);

    if let Some(stripped) = dir_path.strip_prefix(mod_base) {
        let sub = stripped.trim_start_matches('/');
        let p = mod_root.join(sub);
        if fs.is_dir(&p) {
            return Some(p);
        }
    }

    for subdir in &["pkg", "gen", "usr"] {
        let cand = mod_root.join("cue.mod").join(subdir).join(dir_path);
        if fs.is_dir(&cand) {
            return Some(cand);
        }
    }
    None
}
