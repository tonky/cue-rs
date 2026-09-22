use serde::{Deserialize, Serialize};

/// Which of CUE's four spellings a string or bytes literal was written in.
///
/// The value alone does not say: a value containing a newline may have been written as a
/// block string or as a `\n` escape, and a value containing a backslash may have been
/// written raw or escaped. Re-emitting therefore needs the spelling, and the parser is the
/// only place that still knows it — which is why the formatter used to turn every block
/// string into an unterminated `"…"` and every `\\.` into the invalid escape `\.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StringForm {
    /// `"…"` — escapes interpreted.
    Quoted,
    /// `"""` … `"""` — escapes interpreted, and the indentation of the closing delimiter
    /// is stripped from every line, as is the newline after the opening delimiter and the
    /// one before the closing line.
    Block,
    /// `#"…"#` — no escapes, `hashes` of them on each side.
    Raw { hashes: usize },
    /// `#"""` … `"""#` — no escapes, indentation stripped as for [`StringForm::Block`].
    RawBlock { hashes: usize },
}

impl StringForm {
    /// Whether a bare backslash starts an escape.
    ///
    /// A raw form guards its escapes with the same number of `#` as its delimiter, so in
    /// `#"…"#` the two characters `\n` are text and only `\#n` is a newline. That guard
    /// is the whole point of the form: it is how a Windows path or a regex is written
    /// without doubling every backslash.
    pub fn is_raw(self) -> bool {
        matches!(self, Self::Raw { .. } | Self::RawBlock { .. })
    }

    /// Whether the value spans lines and had its indentation stripped.
    pub fn is_block(self) -> bool {
        matches!(self, Self::Block | Self::RawBlock { .. })
    }

    /// How many `#` guard the delimiters.
    pub fn hashes(self) -> usize {
        match self {
            Self::Quoted | Self::Block => 0,
            Self::Raw { hashes } | Self::RawBlock { hashes } => hashes,
        }
    }
}

/// A string or bytes literal: what it means, and how it was written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StringLit {
    /// The decoded value — escapes resolved, block indentation stripped. This is what
    /// evaluation sees, and it is the same whichever spelling produced it.
    pub value: String,
    pub form: StringForm,
}

impl StringLit {
    /// A plain `"…"` literal holding `value`.
    pub fn quoted(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            form: StringForm::Quoted,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl AsRef<str> for StringLit {
    fn as_ref(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
    /// Comments and blank lines above `package` — a licence header, typically. They
    /// cannot live in `decls`, which comes after the imports, so they have their own
    /// field. Only `Decl::Comment` and `Decl::BlankLine` ever appear here.
    #[serde(default)]
    pub header: Vec<Decl>,
    pub package: Option<String>,
    pub imports: Vec<ImportDecl>,
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportDecl {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Decl {
    Field(FieldDecl),
    Alias {
        ident: String,
        expr: Expr,
    },
    Let {
        ident: String,
        expr: Expr,
    },
    Embedding(Expr),
    Ellipsis(Option<Expr>),
    Comprehension(ComprehensionDecl),
    Attribute(Attribute),
    /// A `//` comment, carrying the text after the slashes verbatim.
    ///
    /// Comments travel in `decls` in source order rather than as trivia hanging off each
    /// declaration, which is what keeps this to one new variant instead of a field on
    /// `FieldDecl`, `Label`, `StructLit` and every visitor. The cost is the limit: only a
    /// comment that stands where a declaration could stand is kept. One inside a list
    /// literal, between an expression's operands, or trailing a declaration on its own
    /// line is dropped, and `comment_positions.rs` names each of those.
    Comment(String),
    /// One blank line between declarations. A run of them collapses to this.
    BlankLine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub label: Label,
    pub optional: bool,
    pub value: Expr,
    pub attrs: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Label {
    Ident(String),
    DefIdent(String),
    HiddenIdent(String),
    HiddenDefIdent(String),
    String(String),
    Pattern(Expr),
    Dynamic(Expr),
}

impl Label {
    pub fn name(&self) -> Option<&str> {
        match self {
            Label::Ident(s)
            | Label::DefIdent(s)
            | Label::HiddenIdent(s)
            | Label::HiddenDefIdent(s)
            | Label::String(s) => Some(s.as_str()),
            Label::Pattern(_) | Label::Dynamic(_) => None,
        }
    }

    pub fn is_definition(&self) -> bool {
        matches!(self, Label::DefIdent(_) | Label::HiddenDefIdent(_))
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self, Label::HiddenIdent(_) | Label::HiddenDefIdent(_))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComprehensionDecl {
    pub clauses: Vec<ComprehensionClause>,
    pub struct_lit: StructLit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComprehensionClause {
    For {
        key: Option<String>,
        value: String,
        source: Expr,
    },
    If {
        condition: Expr,
    },
    Let {
        ident: String,
        expr: Expr,
    },
}

/// How a struct was written, so a format can give back the file it read.
///
/// The braces a file shows are not the braces its value has: `a: b: 1`, `a: {b: 1}` and
/// the same field spelled over three lines are one struct. The value cannot say which was
/// written and the parser is the only place that still knows, which is why this is
/// recorded rather than guessed — the same reason [`StringForm`] exists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StructForm {
    /// `{`, a declaration to a line, `}`. What a synthesised struct takes.
    #[default]
    Block,
    /// `{a: 1, b: 2}` — every declaration on the line the brace opened.
    Inline,
    /// `a: b: 1` — the braces the author never wrote. Always exactly one declaration,
    /// and only honoured while that declaration is a field that can carry the chain.
    Path,
}

/// How a list was written. A list has no path spelling, so it has two forms, not three.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListForm {
    /// `[`, an element to a line with a trailing comma, `]`.
    #[default]
    Block,
    /// `[1, 2, 3]` — every element on the line the bracket opened.
    Inline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructLit {
    pub decls: Vec<Decl>,
    #[serde(default)]
    pub form: StructForm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListLit {
    pub elements: Vec<Expr>,
    /// The type the elements past the written ones take: the `int` of `[...int]`.
    ///
    /// `None` alongside `open` is the bare `[...]`, which is why the two are separate
    /// fields. Folding them into one lost the difference between a list open to anything
    /// and a list closed at nothing, and `[...]` was written back as `[]` — a different
    /// value, in a file the formatter had been asked only to lay out.
    pub ellipsis: Option<Box<Expr>>,
    /// Whether `...` was written at all.
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub form: ListForm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisjunctionBranch {
    pub default: bool,
    pub expr: Expr,
    /// Whether the author put this branch on a line of its own.
    ///
    /// A disjunction of seven named constants is the CUE idiom for an enum, and it is
    /// written wrapped because one line of it is 150 characters. Nothing in the value
    /// says so, so the break is recorded where it is read, like every other shape.
    #[serde(default)]
    pub on_new_line: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Bottom,
    Top,
    Null,
    Bool(bool),
    Number(String),
    String(StringLit),
    Bytes(StringLit),
    Ident(String),
    DefIdent(String),
    HiddenIdent(String),
    HiddenDefIdent(String),
    Struct(StructLit),
    List(ListLit),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Disjunction {
        branches: Vec<DisjunctionBranch>,
    },
    Selector {
        expr: Box<Expr>,
        field: String,
    },
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        expr: Box<Expr>,
        low: Option<Box<Expr>>,
        high: Option<Box<Expr>>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    Interpolation {
        parts: Vec<InterpolationPart>,
        /// The spelling the interpolation was written in, so a `"""`-quoted one is not
        /// re-emitted as a single line.
        form: StringForm,
    },
    ListComp(ListComprehension),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListComprehension {
    pub clauses: Vec<ComprehensionClause>,
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterpolationPart {
    Lit(String),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Pos,
    Neg,
    Not,
    Default,
    // Bound operators in unary position (e.g. `> 0`, `<= 100`, `=~ "pattern"`)
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    NotEqual,
    RegexMatch,
    RegexNotMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Unify,         // &
    Disjoin,       // |
    LogicalAnd,    // &&
    LogicalOr,     // ||
    Add,           // +
    Sub,           // -
    Mul,           // *
    Div,           // /
    Equal,         // ==
    NotEqual,      // !=
    Less,          // <
    LessEqual,     // <=
    Greater,       // >
    GreaterEqual,  // >=
    RegexMatch,    // =~
    RegexNotMatch, // !~
}
