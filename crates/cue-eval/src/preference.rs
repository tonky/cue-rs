//! Where a preference mark may stand.
//!
//! `*` is a unary operator that marks a default only as a disjunct of its own:
//! `*a | b`, `**a | b`, `*(a | b) | c`. Anywhere else - `*a & b | c`, a lone
//! `x: *1`, a list element, an argument - upstream's compiler rejects the
//! whole file ("preference mark not allowed at this position"), however
//! unused the field holding it. The parser keeps such a mark in the tree as
//! `Expr::Unary { op: Default }`, and this check finds it.

use cue_syntax::ast::{ComprehensionClause, Decl, Expr, FieldDecl, Label, UnaryOp};
use cue_syntax::visitor::{Visitor, walk_comprehension_clause, walk_decl, walk_expr, walk_label};

/// The first misplaced mark under `decls`, named by the path upstream gives
/// it: field labels, `[]` under a pattern, `let[]` in a `let`, `for[]` in a
/// `for` comprehension and `<unknown>` under a dynamic label.
pub(crate) fn check(decls: &[Decl]) -> Result<(), String> {
    let mut marks = Marks::default();
    for decl in decls {
        marks.visit_decl(decl);
    }
    marks.error.map_or(Ok(()), Err)
}

#[derive(Default)]
struct Marks {
    path: Vec<String>,
    error: Option<String>,
}

impl Marks {
    fn under(&mut self, segment: &str, visit: impl FnOnce(&mut Self)) {
        self.path.push(segment.to_string());
        visit(self);
        self.path.pop();
    }
}

impl Visitor for Marks {
    fn visit_decl(&mut self, decl: &Decl) {
        if self.error.is_some() {
            return;
        }
        match decl {
            Decl::Let { expr, .. } => self.under("let[]", |marks| marks.visit_expr(expr)),
            Decl::Comprehension(comp) => {
                let fors = comp
                    .clauses
                    .iter()
                    .filter(|c| matches!(c, ComprehensionClause::For { .. }))
                    .count();
                for clause in &comp.clauses {
                    walk_comprehension_clause(self, clause);
                }
                self.path.extend((0..fors).map(|_| "for[]".to_string()));
                for inner in &comp.struct_lit.decls {
                    self.visit_decl(inner);
                }
                self.path.truncate(self.path.len() - fors);
            }
            decl => walk_decl(self, decl),
        }
    }

    fn visit_field_decl(&mut self, field: &FieldDecl) {
        walk_label(self, &field.label);
        let segment = match &field.label {
            Label::Pattern(_) => "[]",
            Label::Dynamic(_) => "<unknown>",
            label => label.name().unwrap_or_default(),
        };
        self.under(segment, |marks| marks.visit_expr(&field.value));
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if self.error.is_some() {
            return;
        }
        match expr {
            Expr::Unary {
                op: UnaryOp::Default,
                ..
            } => {
                let path = match self.path.is_empty() {
                    true => String::new(),
                    false => format!("{}: ", self.path.join(".")),
                };
                self.error = Some(format!(
                    "{path}preference mark not allowed at this position"
                ));
            }
            // A default branch is the operand of one mark; further marks on
            // it are the disjunct's too (`**a | b`).
            Expr::Disjunction { branches } => {
                for branch in branches {
                    let mut disjunct = &branch.expr;
                    while branch.default
                        && let Expr::Unary {
                            op: UnaryOp::Default,
                            expr,
                        } = disjunct
                    {
                        disjunct = expr;
                    }
                    self.visit_expr(disjunct);
                }
            }
            Expr::ListComp(comp) => {
                let fors = comp
                    .clauses
                    .iter()
                    .filter(|c| matches!(c, ComprehensionClause::For { .. }))
                    .count();
                for clause in &comp.clauses {
                    walk_comprehension_clause(self, clause);
                }
                self.path.extend((0..fors).map(|_| "for[]".to_string()));
                self.visit_expr(&comp.expr);
                self.path.truncate(self.path.len() - fors);
            }
            expr => walk_expr(self, expr),
        }
    }
}
