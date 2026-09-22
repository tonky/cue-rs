use crate::ast::*;

/// One level of indentation.
///
/// A tab, because that is what `cue fmt` emits and therefore what every CUE file
/// formatted by upstream already carries. Writing spaces instead would rewrite every
/// indented line of a module the first time `enve cue fmt` ran over it, which is not a
/// diff anyone reviews.
const INDENT: &str = "\t";

pub fn format_file(file: &SourceFile) -> String {
    let mut out = String::new();

    for decl in &file.header {
        format_decl(decl, &mut out, 0);
        out.push('\n');
    }

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
                    out.push_str(&format!("{INDENT}{alias} \"{}\"\n", imp.path));
                } else {
                    out.push_str(&format!("{INDENT}\"{}\"\n", imp.path));
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
    let pad = INDENT.repeat(indent);
    match decl {
        // The text is held without its `//` so a comment round-trips as written,
        // whatever spacing it had. A blank line writes nothing: the caller's newline is
        // the line, and padding it would leave trailing whitespace.
        Decl::Comment(text) => out.push_str(&format!("{pad}//{text}")),
        Decl::BlankLine => {}
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
        Label::Ident(s) | Label::DefIdent(s) | Label::HiddenIdent(s) | Label::HiddenDefIdent(s) => {
            out.push_str(s)
        }
        Label::String(s) => format_quoted(s, out),
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
    let pad = INDENT.repeat(indent);
    match expr {
        Expr::Bottom => out.push_str("_|_"),
        Expr::Top => out.push('_'),
        Expr::Null => out.push_str("null"),
        Expr::Bool(b) => out.push_str(&b.to_string()),
        Expr::Number(n) => out.push_str(n),
        Expr::String(s) => format_string_lit(s, '"', out, indent),
        Expr::Bytes(b) => format_string_lit(b, '\'', out, indent),
        Expr::Ident(id) | Expr::DefIdent(id) | Expr::HiddenIdent(id) | Expr::HiddenDefIdent(id) => {
            out.push_str(id)
        }
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
        Expr::Interpolation { parts, form } => {
            let form = emittable_form(
                *form,
                parts.iter().filter_map(|p| match p {
                    InterpolationPart::Lit(s) => Some(s.as_str()),
                    InterpolationPart::Expr(_) => None,
                }),
                '"',
            );
            let mut body = String::new();
            for part in parts {
                match part {
                    InterpolationPart::Lit(s) => escape_into(form, s, '"', &mut body),
                    InterpolationPart::Expr(e) => {
                        // The guard goes between the backslash and the paren, so a raw
                        // literal's interpolation is spelled `\#(…)`.
                        body.push_str(&format!("\\{}(", "#".repeat(form.hashes())));
                        format_expr(e, &mut body, 0);
                        body.push(')');
                    }
                }
            }
            wrap_literal(form, '"', &body, out, indent);
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

/// Writes a literal back in the spelling it was read in.
///
/// The value in the AST is what the literal *means*; the form is how it was written. Both
/// are needed: re-emitting every string as `"…"` would turn a readable `postgresql.conf`
/// block into one line of `\n`-separated escapes, and would reinterpret a raw literal's
/// backslashes on the way back in.
fn format_string_lit(lit: &StringLit, quote: char, out: &mut String, indent: usize) {
    let form = emittable_form(lit.form, std::iter::once(lit.value.as_str()), quote);
    let mut body = String::new();
    escape_into(form, &lit.value, quote, &mut body);
    wrap_literal(form, quote, &body, out, indent);
}

/// The form to write this value in: the one it was read in, unless that form cannot hold
/// it, in which case the quoted form, which can hold anything.
///
/// A raw literal cannot hold its own escape prefix or its own closing delimiter, and a
/// block literal cannot hold a bare `"""`. Falling back changes the spelling and never
/// the value — the alternative is emitting a file that does not reparse.
fn emittable_form<'a>(
    form: StringForm,
    values: impl IntoIterator<Item = &'a str>,
    quote: char,
) -> StringForm {
    let guard = "#".repeat(form.hashes());
    let fence: String = std::iter::repeat_n(quote, 3).collect();
    let closer = match form.is_block() {
        true => format!("{fence}{guard}"),
        false => format!("{quote}{guard}"),
    };
    let escape = format!("\\{guard}");

    let holds = |value: &str| match form.is_raw() {
        true => !value.contains(&escape) && !value.contains(&closer),
        false => !form.is_block() || !value.contains(&fence),
    };
    match values.into_iter().all(holds) {
        true => form,
        false => StringForm::Quoted,
    }
}

/// Escapes a literal run for the form it is being written in.
///
/// A raw form escapes almost nothing — that is what it is for — but it still cannot hold
/// a newline on one line, and its escapes carry the same `#` guard as its delimiters, so
/// the newline is written `\#n`. A block form keeps its newlines and tabs as themselves.
/// Only the single-line quoted form needs the whole set.
///
/// A control character with no escape of its own is written `\uXXXX`, because CUE has no
/// `\0`: a digit after the backslash opens an octal escape, which a string literal does
/// not accept at all.
fn escape_into(form: StringForm, value: &str, quote: char, out: &mut String) {
    let guard = "#".repeat(form.hashes());
    let escaped = |tail: &str| format!("\\{guard}{tail}");
    for c in value.chars() {
        match c {
            // A raw form gives a bare backslash no meaning, so it needs no protection.
            '\\' if !form.is_raw() => out.push_str(&escaped("\\")),
            '\n' | '\t' if form.is_block() => out.push(c),
            '\n' => out.push_str(&escaped("n")),
            '\r' => out.push_str(&escaped("r")),
            '\t' if !form.is_raw() => out.push_str(&escaped("t")),
            c if c == quote && !form.is_block() && !form.is_raw() => {
                out.push_str(&escaped(&quote.to_string()))
            }
            c if c.is_control() => out.push_str(&escaped(&format!("u{:04x}", c as u32))),
            _ => out.push(c),
        }
    }
}

/// Puts the delimiters around an escaped body, laying a block literal out the way
/// `cue fmt` does: the body one level in from the field it belongs to, with the closing
/// delimiter on its own line at the same level.
fn wrap_literal(form: StringForm, quote: char, body: &str, out: &mut String, indent: usize) {
    let guard = "#".repeat(form.hashes());
    if !form.is_block() {
        out.push_str(&format!("{guard}{quote}{body}{quote}{guard}"));
        return;
    }

    let fence: String = std::iter::repeat_n(quote, 3).collect();
    let pad = INDENT.repeat(indent + 1);
    out.push_str(&format!("{guard}{fence}\n"));
    for line in body.split('\n') {
        // A blank line stays blank rather than carrying trailing whitespace; the dedent
        // reads it back the same either way.
        match line.is_empty() {
            true => out.push('\n'),
            false => out.push_str(&format!("{pad}{line}\n")),
        }
    }
    out.push_str(&format!("{pad}{fence}{guard}"));
}

/// The literal spelling of a name: a label or an import path, which carries no form of
/// its own and is always written quoted.
fn format_quoted(value: &str, out: &mut String) {
    out.push('"');
    escape_into(StringForm::Quoted, value, '"', out);
    out.push('"');
}
