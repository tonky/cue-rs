use crate::ast::*;

/// Read-only AST visitor for tree traversals, linting, analysis, and metric collection.
pub trait Visitor: Sized {
    fn visit_source_file(&mut self, file: &SourceFile) {
        walk_source_file(self, file);
    }

    fn visit_decl(&mut self, decl: &Decl) {
        walk_decl(self, decl);
    }

    fn visit_field_decl(&mut self, field: &FieldDecl) {
        walk_field_decl(self, field);
    }

    fn visit_label(&mut self, label: &Label) {
        walk_label(self, label);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_comprehension_clause(&mut self, clause: &ComprehensionClause) {
        walk_comprehension_clause(self, clause);
    }
}

pub fn walk_source_file<V: Visitor>(visitor: &mut V, file: &SourceFile) {
    for decl in &file.decls {
        visitor.visit_decl(decl);
    }
}

pub fn walk_decl<V: Visitor>(visitor: &mut V, decl: &Decl) {
    match decl {
        Decl::Field(f) => visitor.visit_field_decl(f),
        Decl::Alias { expr, .. } | Decl::Let { expr, .. } | Decl::Embedding(expr) => {
            visitor.visit_expr(expr);
        }
        Decl::Ellipsis(Some(expr)) => visitor.visit_expr(expr),
        Decl::Ellipsis(None) | Decl::Attribute(_) => {}
        Decl::Comprehension(comp) => {
            for clause in &comp.clauses {
                visitor.visit_comprehension_clause(clause);
            }
            for inner in &comp.struct_lit.decls {
                visitor.visit_decl(inner);
            }
        }
    }
}

pub fn walk_field_decl<V: Visitor>(visitor: &mut V, field: &FieldDecl) {
    visitor.visit_label(&field.label);
    visitor.visit_expr(&field.value);
}

pub fn walk_label<V: Visitor>(visitor: &mut V, label: &Label) {
    match label {
        Label::Pattern(expr) | Label::Dynamic(expr) => visitor.visit_expr(expr),
        _ => {}
    }
}

pub fn walk_comprehension_clause<V: Visitor>(visitor: &mut V, clause: &ComprehensionClause) {
    match clause {
        ComprehensionClause::For { source, .. } => visitor.visit_expr(source),
        ComprehensionClause::If { condition } => visitor.visit_expr(condition),
        ComprehensionClause::Let { expr, .. } => visitor.visit_expr(expr),
    }
}

pub fn walk_expr<V: Visitor>(visitor: &mut V, expr: &Expr) {
    match expr {
        Expr::Bottom
        | Expr::Top
        | Expr::Null
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::String(_)
        | Expr::Bytes(_)
        | Expr::Ident(_)
        | Expr::DefIdent(_)
        | Expr::HiddenIdent(_)
        | Expr::HiddenDefIdent(_) => {}

        Expr::Struct(s) => {
            for decl in &s.decls {
                visitor.visit_decl(decl);
            }
        }
        Expr::List(l) => {
            for elem in &l.elements {
                visitor.visit_expr(elem);
            }
            if let Some(el) = &l.ellipsis {
                visitor.visit_expr(el);
            }
        }
        Expr::Unary { expr, .. } => visitor.visit_expr(expr),
        Expr::Binary { left, right, .. } => {
            visitor.visit_expr(left);
            visitor.visit_expr(right);
        }
        Expr::Disjunction { branches } => {
            for b in branches {
                visitor.visit_expr(&b.expr);
            }
        }
        Expr::Selector { expr, .. } => visitor.visit_expr(expr),
        Expr::Index { expr, index } => {
            visitor.visit_expr(expr);
            visitor.visit_expr(index);
        }
        Expr::Slice { expr, low, high } => {
            visitor.visit_expr(expr);
            if let Some(l) = low {
                visitor.visit_expr(l);
            }
            if let Some(h) = high {
                visitor.visit_expr(h);
            }
        }
        Expr::Call { func, args } => {
            visitor.visit_expr(func);
            for a in args {
                visitor.visit_expr(a);
            }
        }
        Expr::Interpolation { parts } => {
            for p in parts {
                if let InterpolationPart::Expr(e) = p {
                    visitor.visit_expr(e);
                }
            }
        }
        Expr::ListComp(lc) => {
            for clause in &lc.clauses {
                visitor.visit_comprehension_clause(clause);
            }
            visitor.visit_expr(&lc.expr);
        }
    }
}

/// AST rewriting and transformation trait (folds AST nodes into new transformed nodes).
pub trait Folder: Sized {
    fn fold_source_file(&mut self, file: SourceFile) -> SourceFile {
        fold_source_file(self, file)
    }

    fn fold_decl(&mut self, decl: Decl) -> Decl {
        fold_decl(self, decl)
    }

    fn fold_field_decl(&mut self, field: FieldDecl) -> FieldDecl {
        fold_field_decl(self, field)
    }

    fn fold_label(&mut self, label: Label) -> Label {
        fold_label(self, label)
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        fold_expr(self, expr)
    }

    fn fold_comprehension_clause(&mut self, clause: ComprehensionClause) -> ComprehensionClause {
        fold_comprehension_clause(self, clause)
    }
}

pub fn fold_source_file<F: Folder>(folder: &mut F, file: SourceFile) -> SourceFile {
    SourceFile {
        package: file.package,
        imports: file.imports,
        decls: file
            .decls
            .into_iter()
            .map(|d| folder.fold_decl(d))
            .collect(),
    }
}

pub fn fold_decl<F: Folder>(folder: &mut F, decl: Decl) -> Decl {
    match decl {
        Decl::Field(f) => Decl::Field(folder.fold_field_decl(f)),
        Decl::Alias { ident, expr } => Decl::Alias {
            ident,
            expr: folder.fold_expr(expr),
        },
        Decl::Let { ident, expr } => Decl::Let {
            ident,
            expr: folder.fold_expr(expr),
        },
        Decl::Embedding(expr) => Decl::Embedding(folder.fold_expr(expr)),
        Decl::Ellipsis(Some(expr)) => Decl::Ellipsis(Some(folder.fold_expr(expr))),
        Decl::Ellipsis(None) => Decl::Ellipsis(None),
        Decl::Attribute(attr) => Decl::Attribute(attr),
        Decl::Comprehension(comp) => Decl::Comprehension(ComprehensionDecl {
            clauses: comp
                .clauses
                .into_iter()
                .map(|c| folder.fold_comprehension_clause(c))
                .collect(),
            struct_lit: StructLit {
                decls: comp
                    .struct_lit
                    .decls
                    .into_iter()
                    .map(|d| folder.fold_decl(d))
                    .collect(),
            },
        }),
    }
}

pub fn fold_field_decl<F: Folder>(folder: &mut F, field: FieldDecl) -> FieldDecl {
    FieldDecl {
        label: folder.fold_label(field.label),
        optional: field.optional,
        value: folder.fold_expr(field.value),
        attrs: field.attrs,
    }
}

pub fn fold_label<F: Folder>(folder: &mut F, label: Label) -> Label {
    match label {
        Label::Pattern(expr) => Label::Pattern(folder.fold_expr(expr)),
        Label::Dynamic(expr) => Label::Dynamic(folder.fold_expr(expr)),
        other => other,
    }
}

pub fn fold_comprehension_clause<F: Folder>(
    folder: &mut F,
    clause: ComprehensionClause,
) -> ComprehensionClause {
    match clause {
        ComprehensionClause::For { key, value, source } => ComprehensionClause::For {
            key,
            value,
            source: folder.fold_expr(source),
        },
        ComprehensionClause::If { condition } => ComprehensionClause::If {
            condition: folder.fold_expr(condition),
        },
        ComprehensionClause::Let { ident, expr } => ComprehensionClause::Let {
            ident,
            expr: folder.fold_expr(expr),
        },
    }
}

pub fn fold_expr<F: Folder>(folder: &mut F, expr: Expr) -> Expr {
    match expr {
        Expr::Bottom
        | Expr::Top
        | Expr::Null
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::String(_)
        | Expr::Bytes(_)
        | Expr::Ident(_)
        | Expr::DefIdent(_)
        | Expr::HiddenIdent(_)
        | Expr::HiddenDefIdent(_) => expr,

        Expr::Struct(s) => Expr::Struct(StructLit {
            decls: s.decls.into_iter().map(|d| folder.fold_decl(d)).collect(),
        }),
        Expr::List(l) => Expr::List(ListLit {
            elements: l
                .elements
                .into_iter()
                .map(|e| folder.fold_expr(e))
                .collect(),
            ellipsis: l.ellipsis.map(|e| Box::new(folder.fold_expr(*e))),
        }),
        Expr::Unary { op, expr } => Expr::Unary {
            op,
            expr: Box::new(folder.fold_expr(*expr)),
        },
        Expr::Binary { op, left, right } => Expr::Binary {
            op,
            left: Box::new(folder.fold_expr(*left)),
            right: Box::new(folder.fold_expr(*right)),
        },
        Expr::Disjunction { branches } => Expr::Disjunction {
            branches: branches
                .into_iter()
                .map(|b| DisjunctionBranch {
                    default: b.default,
                    expr: folder.fold_expr(b.expr),
                })
                .collect(),
        },
        Expr::Selector { expr, field } => Expr::Selector {
            expr: Box::new(folder.fold_expr(*expr)),
            field,
        },
        Expr::Index { expr, index } => Expr::Index {
            expr: Box::new(folder.fold_expr(*expr)),
            index: Box::new(folder.fold_expr(*index)),
        },
        Expr::Slice { expr, low, high } => Expr::Slice {
            expr: Box::new(folder.fold_expr(*expr)),
            low: low.map(|l| Box::new(folder.fold_expr(*l))),
            high: high.map(|h| Box::new(folder.fold_expr(*h))),
        },
        Expr::Call { func, args } => Expr::Call {
            func: Box::new(folder.fold_expr(*func)),
            args: args.into_iter().map(|a| folder.fold_expr(a)).collect(),
        },
        Expr::Interpolation { parts } => Expr::Interpolation {
            parts: parts
                .into_iter()
                .map(|p| match p {
                    InterpolationPart::Lit(s) => InterpolationPart::Lit(s),
                    InterpolationPart::Expr(e) => {
                        InterpolationPart::Expr(Box::new(folder.fold_expr(*e)))
                    }
                })
                .collect(),
        },
        Expr::ListComp(lc) => Expr::ListComp(ListComprehension {
            clauses: lc
                .clauses
                .into_iter()
                .map(|c| folder.fold_comprehension_clause(c))
                .collect(),
            expr: Box::new(folder.fold_expr(*lc.expr)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_file;

    struct IdentifierCollector {
        idents: Vec<String>,
    }

    impl Visitor for IdentifierCollector {
        fn visit_expr(&mut self, expr: &Expr) {
            if let Expr::Ident(id) = expr {
                self.idents.push(id.clone());
            }
            walk_expr(self, expr);
        }
    }

    struct StringUpperFolder;

    impl Folder for StringUpperFolder {
        fn fold_expr(&mut self, expr: Expr) -> Expr {
            match expr {
                Expr::String(s) => Expr::String(s.to_uppercase()),
                other => fold_expr(self, other),
            }
        }
    }

    #[test]
    fn test_visitor_collects_identifiers() {
        let src = r#"
            a: b + c
            d: {
                nested: x * y
            }
        "#;
        let file = parse_file(src).unwrap();
        let mut collector = IdentifierCollector { idents: Vec::new() };
        collector.visit_source_file(&file);
        assert_eq!(collector.idents, vec!["b", "c", "x", "y"]);
    }

    #[test]
    fn test_folder_transforms_strings() {
        let src = r#"
            msg: "hello world"
            nested: { sub: "test" }
        "#;
        let file = parse_file(src).unwrap();
        let mut folder = StringUpperFolder;
        let transformed = folder.fold_source_file(file);

        if let Decl::Field(f) = &transformed.decls[0] {
            assert_eq!(f.value, Expr::String("HELLO WORLD".to_string()));
        } else {
            panic!("Expected field decl");
        }
    }
}
