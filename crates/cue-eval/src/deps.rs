//! The names a recipe could read.
//!
//! Re-deriving a merged struct only has to run the recipes whose inputs moved,
//! and which names a recipe reads is a lexical question: it is answered once,
//! from the expression, by collecting every identifier the expression mentions.
//!
//! Nothing is subtracted - not a comprehension's loop variable, not a nested
//! literal's own `let`. Over-collecting costs a derivation that produces the
//! same value; under-collecting would leave a stale one, so the walk only ever
//! errs towards more names.

use std::collections::HashSet;
use std::rc::Rc;

use cue_syntax::ast::{
    ComprehensionClause, ComprehensionDecl, Decl, Expr, InterpolationPart, Label,
    ListComprehension, ListLit, StructLit,
};

/// Names an expression could resolve in its enclosing scopes, with the names a
/// `let` of the same literal reaches folded in.
pub fn recipe_deps(expr: &Expr, lets: &[(String, Rc<Expr>)]) -> HashSet<String> {
    expand_lets(direct_deps(expr), lets)
}

pub(crate) fn direct_deps(expr: &Expr) -> HashSet<String> {
    let mut names = HashSet::new();
    walk_expr(expr, &mut names);
    names
}

pub(crate) fn expand_lets(
    mut names: HashSet<String>,
    lets: &[(String, Rc<Expr>)],
) -> HashSet<String> {
    // A recipe that reads a binding also reads whatever the binding reads, and a
    // binding may read another, so the closure runs until it stops growing.
    // Each pass adds at least one name or ends the loop.
    loop {
        let mut grew = false;
        for (name, bound) in lets {
            if !names.contains(name) {
                continue;
            }
            let mut reached = HashSet::new();
            walk_expr(bound, &mut reached);
            for reached_name in reached {
                grew |= names.insert(reached_name);
            }
        }
        if !grew {
            break;
        }
    }
    names
}

fn walk_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name)
        | Expr::DefIdent(name)
        | Expr::HiddenIdent(name)
        | Expr::HiddenDefIdent(name) => {
            names.insert(name.clone());
        }
        Expr::Bottom
        | Expr::Top
        | Expr::Null
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::String(_)
        | Expr::Bytes(_) => {}
        Expr::Struct(lit) => walk_struct(lit, names),
        Expr::List(lit) => walk_list(lit, names),
        Expr::Unary { expr, .. } => walk_expr(expr, names),
        Expr::Binary { left, right, .. } => {
            walk_expr(left, names);
            walk_expr(right, names);
        }
        Expr::Disjunction { branches } => {
            for branch in branches {
                walk_expr(&branch.expr, names);
            }
        }
        // The selected name is read off a value rather than out of a scope, so
        // only the value it is selected from is a dependency.
        Expr::Selector { expr, .. } => walk_expr(expr, names),
        Expr::Index { expr, index } => {
            walk_expr(expr, names);
            walk_expr(index, names);
        }
        Expr::Slice { expr, low, high } => {
            walk_expr(expr, names);
            for bound in [low, high].into_iter().flatten() {
                walk_expr(bound, names);
            }
        }
        Expr::Call { func, args } => {
            walk_expr(func, names);
            for arg in args {
                walk_expr(arg, names);
            }
        }
        Expr::Interpolation { parts, .. } => {
            for part in parts {
                match part {
                    InterpolationPart::Lit(_) => {}
                    InterpolationPart::Expr(expr) => walk_expr(expr, names),
                }
            }
        }
        Expr::ListComp(ListComprehension { clauses, expr }) => {
            walk_clauses(clauses, names);
            walk_expr(expr, names);
        }
    }
}

fn walk_struct(lit: &StructLit, names: &mut HashSet<String>) {
    for decl in &lit.decls {
        match decl {
            Decl::Field(field) => {
                match &field.label {
                    Label::Pattern(expr) | Label::Dynamic(expr) => walk_expr(expr, names),
                    Label::Ident(_)
                    | Label::DefIdent(_)
                    | Label::HiddenIdent(_)
                    | Label::HiddenDefIdent(_)
                    | Label::String(_) => {}
                }
                walk_expr(&field.value, names);
            }
            Decl::Alias { expr, .. } | Decl::Let { expr, .. } | Decl::Embedding(expr) => {
                walk_expr(expr, names)
            }
            Decl::Ellipsis(expr) => {
                if let Some(expr) = expr {
                    walk_expr(expr, names);
                }
            }
            Decl::Comprehension(ComprehensionDecl {
                clauses,
                struct_lit,
            }) => {
                walk_clauses(clauses, names);
                walk_struct(struct_lit, names);
            }
            Decl::Attribute(_) | Decl::Comment(_) | Decl::BlankLine => {}
        }
    }
}

fn walk_list(lit: &ListLit, names: &mut HashSet<String>) {
    for element in &lit.elements {
        walk_expr(element, names);
    }
    if let Some(ellipsis) = &lit.ellipsis {
        walk_expr(ellipsis, names);
    }
}

fn walk_clauses(clauses: &[ComprehensionClause], names: &mut HashSet<String>) {
    for clause in clauses {
        match clause {
            ComprehensionClause::For { source, .. } => walk_expr(source, names),
            ComprehensionClause::If { condition } => walk_expr(condition, names),
            ComprehensionClause::Let { expr, .. } => walk_expr(expr, names),
        }
    }
}
