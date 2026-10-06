//! Compile-time reference checks upstream applies before evaluating a file.
//!
//! A `let` clause or an alias nobody reads and an import nobody uses are errors
//! in `cue` ("unreferenced alias or let clause", "imported and not used"). They
//! usually mean a reference went to the wrong name, so accepting them hides
//! the mistake rather than tolerating a harmless leftover.

use cue_syntax::ast::{Decl, Expr, SourceFile};
use cue_syntax::visitor::{Visitor, walk_decl, walk_expr};
use std::collections::HashMap;

/// The first unreferenced `let`, alias or import in `file`, as upstream words
/// it.
pub fn check_file(file: &SourceFile) -> Result<(), String> {
    let used = identifiers(&file.decls);
    for import in &file.imports {
        let name = import
            .alias
            .clone()
            .unwrap_or_else(|| import.path.default_alias())
            .into_inner();
        if !used.contains_key(&name) {
            return Err(format!(
                "imported and not used: \"{}\"",
                import.path.as_str()
            ));
        }
    }
    let mut checker = Checker { error: None };
    checker.literal(&file.decls);
    for decl in &file.decls {
        checker.visit_decl(decl);
    }
    checker.error.map_or(Ok(()), Err)
}

/// Every identifier read anywhere under `decls`, with how often.
fn identifiers(decls: &[Decl]) -> HashMap<String, usize> {
    let mut collector = Identifiers::default();
    for decl in decls {
        collector.visit_decl(decl);
    }
    collector.names
}

fn expr_identifiers(expr: &Expr) -> HashMap<String, usize> {
    let mut collector = Identifiers::default();
    collector.visit_expr(expr);
    collector.names
}

#[derive(Default)]
struct Identifiers {
    names: HashMap<String, usize>,
}

impl Visitor for Identifiers {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Ident(name)
        | Expr::DefIdent(name)
        | Expr::HiddenIdent(name)
        | Expr::HiddenDefIdent(name) = expr
        {
            *self.names.entry(name.clone()).or_default() += 1;
        }
        walk_expr(self, expr);
    }
}

/// Checks each struct literal's own `let` and alias declarations against the
/// identifiers read anywhere inside that literal. An inner literal that
/// shadows the name and reads only its own counts as reading the outer one,
/// which accepts a little more than upstream and never rejects valid input.
struct Checker {
    error: Option<String>,
}

impl Checker {
    fn literal(&mut self, decls: &[Decl]) {
        if self.error.is_some() {
            return;
        }
        // A `let` read only by its own expression is unreferenced; a field
        // alias read inside its own field (`X=a: {b: 1, c: X.b}`) is not.
        let bindings: Vec<(&String, Option<&Expr>)> = decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Let { ident, expr } | Decl::Alias { ident, expr } => {
                    Some((ident, Some(expr)))
                }
                Decl::Field(field) => field.alias.as_ref().map(|alias| (alias, None)),
                _ => None,
            })
            .collect();
        if bindings.is_empty() {
            return;
        }
        let used = identifiers(decls);
        for (name, own) in bindings {
            let reads = used.get(name).copied().unwrap_or(0);
            let own_reads = own.map_or(0, |expr| {
                expr_identifiers(expr).get(name).copied().unwrap_or(0)
            });
            if reads <= own_reads {
                self.error = Some(format!("unreferenced alias or let clause {name}"));
                return;
            }
        }
    }
}

impl Visitor for Checker {
    fn visit_decl(&mut self, decl: &Decl) {
        if let Decl::Comprehension(comp) = decl {
            self.literal(&comp.struct_lit.decls);
        }
        walk_decl(self, decl);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Struct(s) = expr {
            self.literal(&s.decls);
        }
        walk_expr(self, expr);
    }
}
