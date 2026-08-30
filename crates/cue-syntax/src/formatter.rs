use crate::ast::*;

pub fn format_file(file: &SourceFile) -> String {
    let mut out = String::new();

    if let Some(pkg) = &file.package {
        out.push_str(&format!("package {pkg}\n\n"));
    }

    if !file.imports.is_empty() {
        if file.imports.len() == 1 {
            let imp = &file.imports[0];
            if let Some(alias) = &imp.alias {
                out.push_str(&format!("import {alias} \"{}\"\n\n", imp.path));
            } else {
                out.push_str(&format!("import \"{}\"\n\n", imp.path));
            }
        } else {
            out.push_str("import (\n");
            for imp in &file.imports {
                if let Some(alias) = &imp.alias {
                    out.push_str(&format!("    {alias} \"{}\"\n", imp.path));
                } else {
                    out.push_str(&format!("    \"{}\"\n", imp.path));
                }
            }
            out.push_str(")\n\n");
        }
    }

    for decl in &file.decls {
        format_decl(decl, &mut out, 0);
        out.push('\n');
    }

    out
}

fn format_decl(decl: &Decl, out: &mut String, indent: usize) {
    let pad = "    ".repeat(indent);
    match decl {
        Decl::Field(f) => {
            out.push_str(&pad);
            format_label(&f.label, out);
            if f.optional {
                out.push('?');
            }
            out.push_str(": ");
            format_expr(&f.value, out, indent);
            for attr in &f.attrs {
                if attr.body.is_empty() {
                    out.push_str(&format!(" @{}", attr.name));
                } else {
                    out.push_str(&format!(" @{}({})", attr.name, attr.body));
                }
            }
        }
        Decl::Alias { ident, expr } => {
            out.push_str(&format!("{pad}{ident} = "));
            format_expr(expr, out, indent);
        }
        Decl::Let { ident, expr } => {
            out.push_str(&format!("{pad}let {ident} = "));
            format_expr(expr, out, indent);
        }
        Decl::Embedding(expr) => {
            out.push_str(&pad);
            format_expr(expr, out, indent);
        }
        Decl::Ellipsis(elem) => {
            out.push_str(&format!("{pad}..."));
            if let Some(e) = elem {
                format_expr(e, out, indent);
            }
        }
        Decl::Comprehension(comp) => {
            out.push_str(&pad);
            for clause in &comp.clauses {
                match clause {
                    ComprehensionClause::For { key, value, source } => {
                        out.push_str("for ");
                        if let Some(k) = key {
                            out.push_str(&format!("{k}, "));
                        }
                        out.push_str(&format!("{value} in "));
                        format_expr(source, out, indent);
                        out.push(' ');
                    }
                    ComprehensionClause::If { condition } => {
                        out.push_str("if ");
                        format_expr(condition, out, indent);
                        out.push(' ');
                    }
                    ComprehensionClause::Let { ident, expr } => {
                        out.push_str(&format!("let {ident} = "));
                        format_expr(expr, out, indent);
                        out.push(' ');
                    }
                }
            }
            out.push_str("{\n");
            for inner_decl in &comp.struct_lit.decls {
                format_decl(inner_decl, out, indent + 1);
                out.push('\n');
            }
            out.push_str(&format!("{pad}}}"));
        }
        Decl::Attribute(attr) => {
            out.push_str(&pad);
            if attr.body.is_empty() {
                out.push_str(&format!("@{}", attr.name));
            } else {
                out.push_str(&format!("@{}({})", attr.name, attr.body));
            }
        }
    }
}

fn format_label(label: &Label, out: &mut String) {
    match label {
        Label::Ident(s)
        | Label::DefIdent(s)
        | Label::HiddenIdent(s)
        | Label::HiddenDefIdent(s) => out.push_str(s),
        Label::String(s) => out.push_str(&format!("\"{s}\"")),
        Label::Pattern(expr) => {
            out.push('[');
            format_expr(expr, out, 0);
            out.push(']');
        }
        Label::Dynamic(expr) => {
            out.push('(');
            format_expr(expr, out, 0);
            out.push(')');
        }
    }
}

fn format_expr(expr: &Expr, out: &mut String, indent: usize) {
    let pad = "    ".repeat(indent);
    match expr {
        Expr::Bottom => out.push_str("_|_"),
        Expr::Top => out.push('_'),
        Expr::Null => out.push_str("null"),
        Expr::Bool(b) => out.push_str(&b.to_string()),
        Expr::Number(n) => out.push_str(n),
        Expr::String(s) => out.push_str(&format!("\"{s}\"")),
        Expr::Bytes(b) => out.push_str(&format!("'{b}'")),
        Expr::Ident(id)
        | Expr::DefIdent(id)
        | Expr::HiddenIdent(id)
        | Expr::HiddenDefIdent(id) => out.push_str(id),
        Expr::Struct(s) => {
            if s.decls.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for decl in &s.decls {
                    format_decl(decl, out, indent + 1);
                    out.push('\n');
                }
                out.push_str(&format!("{pad}}}"));
            }
        }
        Expr::List(l) => {
            out.push('[');
            for (i, elem) in l.elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(elem, out, indent);
            }
            if let Some(el) = &l.ellipsis {
                if !l.elements.is_empty() {
                    out.push_str(", ");
                }
                out.push_str("...");
                format_expr(el, out, indent);
            }
            out.push(']');
        }
        Expr::Binary { op, left, right } => {
            format_expr(left, out, indent);
            let op_str = match op {
                BinaryOp::Unify => " & ",
                BinaryOp::Disjoin => " | ",
                BinaryOp::LogicalAnd => " && ",
                BinaryOp::LogicalOr => " || ",
                BinaryOp::Add => " + ",
                BinaryOp::Sub => " - ",
                BinaryOp::Mul => " * ",
                BinaryOp::Div => " / ",
                BinaryOp::Equal => " == ",
                BinaryOp::NotEqual => " != ",
                BinaryOp::Less => " < ",
                BinaryOp::LessEqual => " <= ",
                BinaryOp::Greater => " > ",
                BinaryOp::GreaterEqual => " >= ",
                BinaryOp::RegexMatch => " =~ ",
                BinaryOp::RegexNotMatch => " !~ ",
            };
            out.push_str(op_str);
            format_expr(right, out, indent);
        }
        Expr::Unary { op, expr } => {
            let op_str = match op {
                UnaryOp::Pos => "+",
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
                UnaryOp::Default => "*",
                UnaryOp::Less => "<",
                UnaryOp::LessEqual => "<=",
                UnaryOp::Greater => ">",
                UnaryOp::GreaterEqual => ">=",
                UnaryOp::NotEqual => "!=",
                UnaryOp::RegexMatch => "=~",
                UnaryOp::RegexNotMatch => "!~",
            };
            out.push_str(op_str);
            format_expr(expr, out, indent);
        }
        Expr::Disjunction { branches } => {
            for (i, b) in branches.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                if b.default {
                    out.push('*');
                }
                format_expr(&b.expr, out, indent);
            }
        }
        Expr::Selector { expr, field } => {
            format_expr(expr, out, indent);
            out.push('.');
            out.push_str(field);
        }
        Expr::Index { expr, index } => {
            format_expr(expr, out, indent);
            out.push('[');
            format_expr(index, out, indent);
            out.push(']');
        }
        Expr::Slice { expr, low, high } => {
            format_expr(expr, out, indent);
            out.push('[');
            if let Some(l) = low {
                format_expr(l, out, indent);
            }
            out.push(':');
            if let Some(h) = high {
                format_expr(h, out, indent);
            }
            out.push(']');
        }
        Expr::Call { func, args } => {
            format_expr(func, out, indent);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(a, out, indent);
            }
            out.push(')');
        }
        Expr::Interpolation { parts } => {
            out.push('"');
            for part in parts {
                match part {
                    InterpolationPart::Lit(s) => out.push_str(s),
                    InterpolationPart::Expr(e) => {
                        out.push_str(r"\(");
                        format_expr(e, out, 0);
                        out.push(')');
                    }
                }
            }
            out.push('"');
        }
        Expr::ListComp(comp) => {
            out.push_str("[ ");
            for clause in &comp.clauses {
                match clause {
                    ComprehensionClause::For { key, value, source } => {
                        out.push_str("for ");
                        if let Some(k) = key {
                            out.push_str(k);
                            out.push_str(", ");
                        }
                        out.push_str(value);
                        out.push_str(" in ");
                        format_expr(source, out, indent);
                        out.push(' ');
                    }
                    ComprehensionClause::If { condition } => {
                        out.push_str("if ");
                        format_expr(condition, out, indent);
                        out.push(' ');
                    }
                    ComprehensionClause::Let { ident, expr } => {
                        out.push_str("let ");
                        out.push_str(ident);
                        out.push_str(" = ");
                        format_expr(expr, out, indent);
                        out.push(' ');
                    }
                }
            }
            out.push_str("{ ");
            format_expr(&comp.expr, out, indent);
            out.push_str(" } ]");
        }
    }
}
