use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
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
    Alias { ident: String, expr: Expr },
    Let { ident: String, expr: Expr },
    Embedding(Expr),
    Ellipsis(Option<Expr>),
    Comprehension(ComprehensionDecl),
    Attribute(Attribute),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructLit {
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListLit {
    pub elements: Vec<Expr>,
    pub ellipsis: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisjunctionBranch {
    pub default: bool,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Bottom,
    Top,
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Bytes(String),
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
