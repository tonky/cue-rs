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

    format_block(&file.header, &mut out, 0);

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

    format_block(&file.decls, &mut out, 0);

    out
}

/// Writes one declaration, `pad` first.
///
/// The pad is passed rather than derived from `indent` because a declaration inside an
/// inline struct starts where the brace left off and has none, while anything it nests
/// still indents from the line the struct sits on.
fn write_decl(decl: &Decl, out: &mut String, indent: usize, pad: &str) {
    match decl {
        // The text is held without its `//` so a comment round-trips as written,
        // whatever spacing it had. A blank line writes nothing: the caller's newline is
        // the line, and padding it would leave trailing whitespace.
        Decl::Comment(text) => out.push_str(&format!("{pad}//{text}")),
        Decl::BlankLine => {}
        Decl::Field(f) => {
            out.push_str(pad);
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
            out.push_str(pad);
            format_expr(expr, out, indent);
        }
        Decl::Ellipsis(elem) => {
            out.push_str(&format!("{pad}..."));
            if let Some(e) = elem {
                format_expr(e, out, indent);
            }
        }
        Decl::Comprehension(comp) => {
            out.push_str(pad);
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
            format_struct(&comp.struct_lit, out, indent, pad);
        }
        Decl::Attribute(attr) => {
            out.push_str(pad);
            if attr.body.is_empty() {
                out.push_str(&format!("@{}", attr.name));
            } else {
                out.push_str(&format!("@{}({})", attr.name, attr.body));
            }
        }
    }
}

/// Writes a struct in the shape it was written in.
///
/// A path struct reaching here is one [`render_field`] could not write as a path — it has
/// attributes, or it is not the value of a field at all — so it takes its braces back.
fn format_struct(s: &StructLit, out: &mut String, indent: usize, pad: &str) {
    if s.decls.is_empty() {
        out.push_str("{}");
        return;
    }
    if s.form == StructForm::Inline && s.decls.iter().all(fits_on_a_line) {
        out.push('{');
        for (i, decl) in s.decls.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write_decl(decl, out, indent, "");
        }
        out.push('}');
        return;
    }
    out.push_str("{\n");
    format_block(&s.decls, out, indent + 1);
    out.push_str(&format!("{pad}}}"));
}

/// Writes a list in the shape it was written in.
fn format_list(l: &ListLit, out: &mut String, indent: usize, pad: &str) {
    let ellipsis = l.open.then_some(l.ellipsis.as_deref());
    if l.elements.is_empty() && ellipsis.is_none() {
        out.push_str("[]");
        return;
    }
    if l.form == ListForm::Inline {
        out.push('[');
        for (i, elem) in l.elements.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            format_expr(elem, out, indent);
        }
        if let Some(el) = ellipsis {
            if !l.elements.is_empty() {
                out.push_str(", ");
            }
            out.push_str("...");
            if let Some(el) = el {
                format_expr(el, out, indent);
            }
        }
        out.push(']');
        return;
    }
    let inner = INDENT.repeat(indent + 1);
    out.push_str("[\n");
    for elem in &l.elements {
        out.push_str(&inner);
        format_expr(elem, out, indent + 1);
        out.push_str(",\n");
    }
    if let Some(el) = ellipsis {
        out.push_str(&inner);
        out.push_str("...");
        if let Some(el) = el {
            format_expr(el, out, indent + 1);
        }
        out.push('\n');
    }
    out.push_str(&format!("{pad}]"));
}

/// Whether a declaration can be written next to its siblings on one line.
///
/// A comment runs to the end of its line and a blank line *is* a line, so either one
/// inside an inline struct would write a file that no longer parses. Neither can be
/// produced by the parser — a struct holding one has a newline in it and is therefore a
/// block — so this guards a struct built by hand.
fn fits_on_a_line(decl: &Decl) -> bool {
    !matches!(decl, Decl::Comment(_) | Decl::BlankLine)
}

/// A declaration rendered for output.
enum Rendered {
    /// A field on one line, split into the cells a column block aligns.
    ///
    /// `depth` is how many labels the field's path carries: `a: b: 1` is two. Upstream
    /// aligns the whole chain as a single cell and only against chains of the same depth,
    /// so the depth groups a run rather than adding cells to it.
    Cells { depth: usize, cells: Vec<String> },
    /// A line taking no part in alignment, which therefore ends every run around it.
    Opaque(String),
}

impl Rendered {
    fn cells(&self) -> &[String] {
        match self {
            Self::Cells { cells, .. } => cells,
            Self::Opaque(_) => &[],
        }
    }

    fn depth(&self) -> usize {
        match self {
            Self::Cells { depth, .. } => *depth,
            Self::Opaque(_) => 0,
        }
    }

    /// Whether a cell follows this one on the same line, which is what makes upstream pad
    /// it: the last cell of a line is written as it is and sizes no column.
    fn pads(&self, column: usize) -> bool {
        self.cells().len() > column + 1
    }
}

/// Writes a run of sibling declarations, aligning the columns upstream aligns.
///
/// Each declaration is written on its own line, and the alignment is `text/tabwriter`'s,
/// because that is the package upstream pipes its output through. A column is sized over
/// the widest cell in a maximal run of adjacent lines that all have a cell after it, so a
/// comment, a blank line, an embedding, or a field too complicated to sit in cells ends
/// the run by contributing no cells at all.
fn format_block(decls: &[Decl], out: &mut String, indent: usize) {
    let lines: Vec<Rendered> = decls.iter().map(|decl| render_decl(decl, indent)).collect();

    for (line, widths) in lines.iter().zip(column_widths(&lines)) {
        match line {
            Rendered::Opaque(text) => out.push_str(text),
            Rendered::Cells { cells, .. } => {
                for (column, cell) in cells.iter().enumerate() {
                    out.push_str(cell);
                    if column + 1 < cells.len() {
                        let width = widths[column].max(cell.chars().count() + 1);
                        out.push_str(&" ".repeat(width - cell.chars().count()));
                    }
                }
            }
        }
        out.push('\n');
    }
}

/// The width each cell is padded to, or zero where it is written as it is.
fn column_widths(lines: &[Rendered]) -> Vec<Vec<usize>> {
    let mut widths: Vec<Vec<usize>> = lines.iter().map(|l| vec![0; l.cells().len()]).collect();
    let columns = lines.iter().map(|l| l.cells().len()).max().unwrap_or(0);

    for column in 0..columns {
        let mut start = 0;
        while start < lines.len() {
            if !lines[start].pads(column) {
                start += 1;
                continue;
            }
            let depth = lines[start].depth();
            let end = (start..lines.len())
                .take_while(|&i| lines[i].pads(column) && lines[i].depth() == depth)
                .last()
                .map_or(start, |i| i + 1);
            let width = (start..end)
                .map(|i| lines[i].cells()[column].chars().count())
                .max()
                .unwrap_or(0)
                + 1;
            for line in widths.iter_mut().take(end).skip(start) {
                line[column] = width;
            }
            start = end;
        }
    }
    widths
}

fn render_decl(decl: &Decl, indent: usize) -> Rendered {
    let pad = INDENT.repeat(indent);
    match decl {
        Decl::Field(field) => render_field(field, indent, &pad),
        other => {
            let mut text = String::new();
            write_decl(other, &mut text, indent, &pad);
            Rendered::Opaque(text)
        }
    }
}

/// Splits a field into the cells a column block aligns, or gives back the whole line.
fn render_field(field: &FieldDecl, indent: usize, pad: &str) -> Rendered {
    let (path, last) = path_chain(field);
    let mut label = String::from(pad);
    for (i, segment) in path.iter().enumerate() {
        if i > 0 {
            label.push(' ');
        }
        format_label(&segment.label, &mut label);
        if segment.optional {
            label.push('?');
        }
        label.push(':');
    }

    let mut value = String::new();
    format_expr(&last.value, &mut value, indent);

    let attrs = last
        .attrs
        .iter()
        .map(|attr| match attr.body.is_empty() {
            true => format!("@{}", attr.name),
            false => format!("@{}({})", attr.name, attr.body),
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Upstream stops aligning at a field whose value holds a composite even when the
    // literal fits on the line, so `short: f({d: 1})` ends a run exactly as `short: {d: 1}`
    // does. A value written over several lines cannot be a cell at all.
    if value.contains('\n') || label.contains('\n') || holds_composite(&last.value) {
        let mut text = format!("{label} {value}");
        if !attrs.is_empty() {
            text.push(' ');
            text.push_str(&attrs);
        }
        return Rendered::Opaque(text);
    }

    let mut cells = vec![label, value];
    if !attrs.is_empty() {
        cells.push(attrs);
    }
    Rendered::Cells {
        depth: path.len(),
        cells,
    }
}

/// Walks the labels a field carries, and the field the last of them holds.
///
/// `a: b: 1` is one field holding a path struct holding another field, and writing it
/// back that way is the whole of the path form. The walk stops at attributes: they belong
/// to the innermost field, and a path has nowhere to put them, which is why upstream
/// gives `a: b: 1 @go(A)` its braces back.
fn path_chain(field: &FieldDecl) -> (Vec<&FieldDecl>, &FieldDecl) {
    let mut path = vec![field];
    let mut last = field;
    while let Expr::Struct(s) = &last.value {
        let [Decl::Field(inner)] = s.decls.as_slice() else {
            break;
        };
        if s.form != StructForm::Path {
            break;
        }
        path.push(inner);
        last = inner;
    }
    if path.len() > 1 && !last.attrs.is_empty() {
        return (vec![field], field);
    }
    (path, last)
}

/// Whether a value holds a struct or list literal anywhere inside it.
fn holds_composite(expr: &Expr) -> bool {
    match expr {
        Expr::Struct(_) | Expr::List(_) | Expr::ListComp(_) => true,
        Expr::Unary { expr, .. } => holds_composite(expr),
        Expr::Binary { left, right, .. } => holds_composite(left) || holds_composite(right),
        Expr::Disjunction { branches } => branches.iter().any(|b| holds_composite(&b.expr)),
        Expr::Selector { expr, .. } => holds_composite(expr),
        Expr::Index { expr, index } => holds_composite(expr) || holds_composite(index),
        Expr::Slice { expr, low, high } => {
            holds_composite(expr)
                || low.as_deref().is_some_and(holds_composite)
                || high.as_deref().is_some_and(holds_composite)
        }
        Expr::Call { func, args } => holds_composite(func) || args.iter().any(holds_composite),
        Expr::Interpolation { parts, .. } => parts.iter().any(|part| match part {
            InterpolationPart::Expr(expr) => holds_composite(expr),
            InterpolationPart::Lit(_) => false,
        }),
        _ => false,
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

fn format_expr(expr_node: &Expr, out: &mut String, indent: usize) {
    let pad = INDENT.repeat(indent);
    match expr_node {
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
        Expr::Struct(s) => format_struct(s, out, indent, &pad),
        Expr::List(l) => format_list(l, out, indent, &pad),
        Expr::Binary { op, left, right } => {
            let strength = binding_strength(expr_node);
            format_operand(left, strength, out, indent);
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
            // Every binary operator in CUE associates to the left, so an operand on the
            // right that binds exactly as tightly is one the source parenthesised.
            format_operand(right, strength + 1, out, indent);
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
            format_operand(expr, binding_strength(expr_node), out, indent);
        }
        Expr::Disjunction { branches } => {
            for (i, b) in branches.iter().enumerate() {
                if i > 0 {
                    out.push_str(" |");
                    match b.on_new_line {
                        true => out.push_str(&format!("\n{}", INDENT.repeat(indent + 1))),
                        false => out.push(' '),
                    }
                }
                if b.default {
                    out.push('*');
                }
                // A branch binds at least as tightly as the disjunction holding it, so
                // only something looser — another bare disjunction — needs the parentheses.
                format_operand(&b.expr, 2, out, indent);
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
            out.push('[');
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
            out.push('{');
            format_expr(&comp.expr, out, indent);
            out.push_str("}]");
        }
    }
}

/// Writes a literal back in the spelling it was read in.
///
/// The value in the AST is what the literal *means*; the form is how it was written. Both
/// are needed: re-emitting every string as `"…"` would turn a readable `postgresql.conf`
/// block into one line of `\n`-separated escapes, and would reinterpret a raw literal's
/// backslashes on the way back in.
/// Writes an operand, in parentheses when the tree would not survive without them.
///
/// The parser does not keep the parentheses a file was written with — `(a | b) & c` and
/// `a | b & c` reach here as different trees, and only one of them is what was written.
/// Emitting the operands bare wrote the second for both, which is not a layout change: it
/// moves `c` inside the disjunction, and `(1 | 2) & 2` stops being `2`.
///
/// So the parentheses are derived from the tree rather than remembered from the source.
/// The output reparses to the tree it was printed from whatever the author wrote, and a
/// pair the tree does not need is not written back — which is the one place this diverges
/// from upstream, in the safe direction.
fn format_operand(operand: &Expr, required: u8, out: &mut String, indent: usize) {
    if binding_strength(operand) < required {
        out.push('(');
        format_expr(operand, out, indent);
        out.push(')');
    } else {
        format_expr(operand, out, indent);
    }
}

/// How tightly an expression binds, on the CUE spec's scale.
///
/// Anything that is not an operator is a single term, and binds tighter than every
/// operator can pull.
fn binding_strength(expr: &Expr) -> u8 {
    match expr {
        Expr::Disjunction { .. } => 1,
        Expr::Binary { op, .. } => match op {
            BinaryOp::Disjoin => 1,
            BinaryOp::Unify => 2,
            BinaryOp::LogicalOr => 3,
            BinaryOp::LogicalAnd => 4,
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::RegexMatch
            | BinaryOp::RegexNotMatch => 5,
            BinaryOp::Add | BinaryOp::Sub => 6,
            BinaryOp::Mul | BinaryOp::Div => 7,
        },
        Expr::Unary { .. } => 8,
        _ => u8::MAX,
    }
}

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
