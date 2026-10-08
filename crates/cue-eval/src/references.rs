//! Compile-time reference checks upstream applies before evaluating a file.
//!
//! A `let` clause or an alias nobody reads and an import nobody uses are errors
//! in `cue` ("unreferenced alias or let clause", "imported and not used"). They
//! usually mean a reference went to the wrong name, so accepting them hides
//! the mistake rather than tolerating a harmless leftover. So is an alias or
//! `let` named like a field it can see, or that can see it.

use cue_syntax::ast::{Decl, Expr, FieldDecl, Label, SourceFile};
use cue_syntax::visitor::{Visitor, walk_decl, walk_expr, walk_field_decl};
use std::collections::{HashMap, HashSet};

/// The first unreferenced `let`, alias or import in `file`, as upstream words
/// it.
pub fn check_file(file: &SourceFile) -> Result<(), String> {
    crate::preference::check(&file.decls)?;
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
    let mut scopes = Scopes::default();
    scopes.enter(&file.decls);
    for decl in &file.decls {
        scopes.visit_decl(decl);
    }
    if let Some(error) = scopes.error {
        return Err(error);
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
        // alias read inside its own field (`X=a: {b: 1, c: X.b}`) is not. A
        // value alias (`a: X=v`) is read only inside the value it names. A
        // label alias (`[K=string]`, `a~(K,_)`) may go unread, as upstream.
        let mut bindings: Vec<Binding<'_>> = Vec::new();
        for decl in decls {
            match decl {
                Decl::Let { ident, expr } | Decl::Alias { ident, expr } => {
                    bindings.push(Binding::Literal(ident, Some(expr)));
                }
                Decl::Field(field) => {
                    if let Some(alias) = &field.alias {
                        bindings.push(Binding::Literal(alias, None));
                    }
                    if let Some(alias) = &field.value_alias {
                        bindings.push(Binding::Within(alias, &field.value));
                    }
                }
                _ => {}
            }
        }
        if bindings.is_empty() {
            return;
        }
        let used = identifiers(decls);
        for binding in bindings {
            let (name, unreferenced) = match binding {
                Binding::Literal(name, own) => {
                    let reads = used.get(name).copied().unwrap_or(0);
                    let own_reads = own.map_or(0, |expr| {
                        expr_identifiers(expr).get(name).copied().unwrap_or(0)
                    });
                    (name, reads <= own_reads)
                }
                Binding::Within(name, value) => (name, !expr_identifiers(value).contains_key(name)),
            };
            if unreferenced {
                self.error = Some(format!("unreferenced alias or let clause {name}"));
                return;
            }
        }
    }
}

/// Where a binding of one literal is visible.
enum Binding<'a> {
    /// Anywhere in the literal; a `let` does not count reading itself.
    Literal(&'a String, Option<&'a Expr>),
    /// Only inside the value it names.
    Within(&'a String, &'a Expr),
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

/// Upstream refuses an alias or `let` and a field of one name where either is
/// in the other's scope: `X: 5` and `o: {let X = 1}`, `o: {let X = 1, p: {X:
/// 2}}`, `o: {X: 1, a~X: 2}` ("cannot have both alias and field with name").
/// A field counts by its identifier label, not a quoted one; a comprehension's
/// own variables and `let` clauses are no aliases.
#[derive(Default)]
struct Scopes {
    /// Per enclosing struct literal, innermost last: its field names and its
    /// alias and `let` names.
    stack: Vec<(HashSet<String>, HashSet<String>)>,
    error: Option<String>,
}

impl Scopes {
    fn enter(&mut self, decls: &[Decl]) {
        let mut fields = HashSet::new();
        let mut aliases = HashSet::new();
        for decl in decls {
            match decl {
                Decl::Let { ident, .. } | Decl::Alias { ident, .. } => {
                    aliases.insert(ident.clone());
                }
                Decl::Field(field) => {
                    if let Some(name) = field.label.ident_name() {
                        fields.insert(name.to_string());
                    }
                    if !matches!(field.label, Label::Pattern(_)) {
                        aliases.extend(field.alias.iter().chain(&field.label_alias).cloned());
                    }
                }
                _ => {}
            }
        }
        self.push(fields, aliases);
    }

    /// The scope of what a field's aliases name only inside its value: a
    /// pattern's (`[string]~(K,V): v`) and a value alias (`a: X=v`).
    fn enter_value(&mut self, field: &FieldDecl) -> bool {
        let mut aliases: HashSet<String> = field.value_alias.iter().cloned().collect();
        if matches!(field.label, Label::Pattern(_)) {
            aliases.extend(field.alias.iter().chain(&field.label_alias).cloned());
        }
        if aliases.is_empty() {
            return false;
        }
        self.push(HashSet::new(), aliases);
        true
    }

    fn push(&mut self, fields: HashSet<String>, aliases: HashSet<String>) {
        if self.error.is_none() {
            let clash = aliases
                .iter()
                .find(|alias| {
                    fields.contains(*alias)
                        || self.stack.iter().any(|(outer, _)| outer.contains(*alias))
                })
                .or_else(|| {
                    fields
                        .iter()
                        .find(|field| self.stack.iter().any(|(_, outer)| outer.contains(*field)))
                });
            if let Some(name) = clash {
                self.error = Some(format!(
                    "cannot have both alias and field with name \"{name}\" in same scope"
                ));
            }
        }
        self.stack.push((fields, aliases));
    }
}

impl Visitor for Scopes {
    fn visit_field_decl(&mut self, field: &FieldDecl) {
        let entered = self.enter_value(field);
        walk_field_decl(self, field);
        if entered {
            self.stack.pop();
        }
    }

    fn visit_decl(&mut self, decl: &Decl) {
        if self.error.is_some() {
            return;
        }
        if let Decl::Comprehension(comp) = decl {
            for clause in &comp.clauses {
                self.visit_comprehension_clause(clause);
            }
            self.enter(&comp.struct_lit.decls);
            for inner in &comp.struct_lit.decls {
                self.visit_decl(inner);
            }
            self.stack.pop();
            return;
        }
        walk_decl(self, decl);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if self.error.is_some() {
            return;
        }
        if let Expr::Struct(s) = expr {
            self.enter(&s.decls);
            walk_expr(self, expr);
            self.stack.pop();
            return;
        }
        walk_expr(self, expr);
    }
}
