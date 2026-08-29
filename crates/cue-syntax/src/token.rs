use logos::Logos;
use std::fmt;

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip r"//[^\r\n]*")]
pub enum Token {
    // Top and Bottom
    #[token("_|_")]
    Bottom,

    #[token("_")]
    Top,

    // Keywords
    #[token("package")]
    KwPackage,

    #[token("import")]
    KwImport,

    #[token("for")]
    KwFor,

    #[token("in")]
    KwIn,

    #[token("if")]
    KwIf,

    #[token("let")]
    KwLet,

    #[token("null")]
    KwNull,

    #[token("true")]
    KwTrue,

    #[token("false")]
    KwFalse,

    // Punctuation
    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,

    #[token(".")]
    Dot,

    #[token("...")]
    Ellipsis,

    #[token("..")]
    DotDot,

    #[token("?")]
    Question,

    #[token("!")]
    Bang,

    // Operators
    #[token("&")]
    Ampersand,

    #[token("|")]
    Pipe,

    #[token("*")]
    Star,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("/")]
    Slash,

    #[token("==")]
    EqualEqual,

    #[token("!=")]
    NotEqual,

    #[token("<")]
    Less,

    #[token("<=")]
    LessEqual,

    #[token(">")]
    Greater,

    #[token(">=")]
    GreaterEqual,

    #[token("=~")]
    RegexMatch,

    #[token("!~")]
    RegexNotMatch,

    #[token("=")]
    Equal,

    // Identifiers
    // Definition identifier: #Foo, #foo_bar
    #[regex(r"#[a-zA-Z0-9_]+", |lex| lex.slice().to_string())]
    DefIdent(String),

    // Hidden definition: _#Foo
    #[regex(r"_#[a-zA-Z0-9_]+", |lex| lex.slice().to_string())]
    HiddenDefIdent(String),

    // Hidden identifier: _foo
    #[regex(r"_[a-zA-Z0-9_]+", |lex| lex.slice().to_string())]
    HiddenIdent(String),

    // Regular identifier
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Numbers (integer, float, hex, binary, octal, SI suffixes)
    #[regex(r"0x[0-9a-fA-F][0-9a-fA-F_]*", |lex| lex.slice().to_string())]
    #[regex(r"0b[01][01_]*", |lex| lex.slice().to_string())]
    #[regex(r"0o[0-7][0-7_]*", |lex| lex.slice().to_string())]
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?([KMGTP]i|[KMGTPk])?", |lex| lex.slice().to_string())]
    Number(String),

    // String literals (single-line double quoted)
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    StringLit(String),

    // Multiline double quoted string: """ ... """
    #[regex(r#""""(?:[^"]|"[^"]|""[^"])*""""#, |lex| {
        let s = lex.slice();
        s[3..s.len()-3].to_string()
    })]
    MultiStringLit(String),

    // Single quoted string (bytes)
    #[regex(r#"'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    BytesLit(String),

    // Attribute: @tag(...) or @protobuf(1, int32)
    #[regex(r"@[a-zA-Z0-9_]+(?:\([^\)]*\))?", |lex| lex.slice().to_string())]
    Attribute(String),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Bottom => write!(f, "_|_"),
            Token::Top => write!(f, "_"),
            Token::KwPackage => write!(f, "package"),
            Token::KwImport => write!(f, "import"),
            Token::KwFor => write!(f, "for"),
            Token::KwIn => write!(f, "in"),
            Token::KwIf => write!(f, "if"),
            Token::KwLet => write!(f, "let"),
            Token::KwNull => write!(f, "null"),
            Token::KwTrue => write!(f, "true"),
            Token::KwFalse => write!(f, "false"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Ellipsis => write!(f, "..."),
            Token::DotDot => write!(f, ".."),
            Token::Question => write!(f, "?"),
            Token::Bang => write!(f, "!"),
            Token::Ampersand => write!(f, "&"),
            Token::Pipe => write!(f, "|"),
            Token::Star => write!(f, "*"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Slash => write!(f, "/"),
            Token::EqualEqual => write!(f, "=="),
            Token::NotEqual => write!(f, "!="),
            Token::Less => write!(f, "<"),
            Token::LessEqual => write!(f, "<="),
            Token::Greater => write!(f, ">"),
            Token::GreaterEqual => write!(f, ">="),
            Token::RegexMatch => write!(f, "=~"),
            Token::RegexNotMatch => write!(f, "!~"),
            Token::Equal => write!(f, "="),
            Token::DefIdent(s) => write!(f, "{}", s),
            Token::HiddenDefIdent(s) => write!(f, "{}", s),
            Token::HiddenIdent(s) => write!(f, "{}", s),
            Token::Ident(s) => write!(f, "{}", s),
            Token::Number(s) => write!(f, "{}", s),
            Token::StringLit(s) => write!(f, "\"{}\"", s),
            Token::MultiStringLit(s) => write!(f, "\"\"\"{}\"\"\"", s),
            Token::BytesLit(s) => write!(f, "'{}'", s),
            Token::Attribute(s) => write!(f, "{}", s),
        }
    }
}
