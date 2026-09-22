use crate::ast::StringForm;
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

    #[token("~")]
    Tilde,

    // Operators
    #[token("&")]
    Ampersand,

    #[token("&&")]
    AndAnd,

    #[token("|")]
    Pipe,

    #[token("||")]
    PipePipe,

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
    // Definition identifier: #Foo, #foo_bar, #
    #[regex(r"#[a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    DefIdent(String),

    // Hidden definition: _#Foo, _#
    #[regex(r"_#[a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    HiddenDefIdent(String),

    // Hidden identifier: _foo
    #[regex(r"_[a-zA-Z0-9_]+", |lex| lex.slice().to_string())]
    HiddenIdent(String),

    // Regular identifier (including $ identifiers like $type, $id, $)
    #[regex(r"(\$|[a-zA-Z])[a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Numbers (integer, float, hex, binary, octal, SI suffixes)
    #[regex(r"0x[0-9a-fA-F][0-9a-fA-F_]*", |lex| lex.slice().to_string())]
    #[regex(r"0b[01][01_]*", |lex| lex.slice().to_string())]
    #[regex(r"0o[0-7][0-7_]*", |lex| lex.slice().to_string())]
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?([KMGTP]i|[KMGTPk])?", |lex| lex.slice().to_string())]
    Number(String),

    // String literals. One rule per delimiter covers all four spellings — `"…"`,
    // `"""…"""`, and the raw forms guarded by any number of `#` — because the opening
    // delimiter is what tells them apart and the scanner reads the rest. The regexes this
    // replaced could not express a raw string containing a quote, and threw away which
    // spelling had been read.
    #[regex(r##"#*""##, lex_string)]
    StringLit(RawString),

    #[regex(r##"#*'"##, lex_bytes)]
    BytesLit(RawString),

    // Attribute: @tag(...) or @protobuf(1, int32)
    #[regex(r"@[a-zA-Z0-9_]+", lex_attribute)]
    Attribute(String),
}

/// A string literal exactly as it was written: the text between the delimiters, still
/// encoded, and the spelling it was written in. Decoding belongs to the parser; the lexer
/// only has to find where the literal ends, which is the part that differs between forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawString {
    pub text: String,
    pub form: StringForm,
}

fn lex_string(lex: &mut logos::Lexer<Token>) -> Option<RawString> {
    lex_delimited(lex, '"')
}

fn lex_bytes(lex: &mut logos::Lexer<Token>) -> Option<RawString> {
    lex_delimited(lex, '\'')
}

/// Scans one string or bytes literal, given that its opening delimiter — any number of
/// `#` followed by one quote — has just been matched.
///
/// Two more quotes immediately after make it a block literal, whose closing delimiter is
/// three quotes rather than one. In an unguarded literal `\` escapes the next character
/// and `\(` opens an interpolation whose parentheses may nest and may themselves contain
/// quotes, so the scan has to step over both. A guarded literal ends at the first literal
/// closer: its escapes are spelled `\#`, so a bare `\` cannot hide one.
fn lex_delimited(lex: &mut logos::Lexer<Token>, quote: char) -> Option<RawString> {
    let hashes = lex.slice().len() - quote.len_utf8();
    let rest = lex.remainder();

    let pair: String = [quote, quote].iter().collect();
    let block = rest.starts_with(&pair);
    let opened = if block { pair.len() } else { 0 };
    let body = &rest[opened..];

    let guard = "#".repeat(hashes);
    let closer = if block {
        format!("{pair}{quote}{guard}")
    } else {
        format!("{quote}{guard}")
    };

    let end = if hashes > 0 {
        body.find(&closer)?
    } else {
        find_closer(body, &closer)?
    };

    lex.bump(opened + end + closer.len());
    let form = match (block, hashes) {
        (false, 0) => StringForm::Quoted,
        (true, 0) => StringForm::Block,
        (false, hashes) => StringForm::Raw { hashes },
        (true, hashes) => StringForm::RawBlock { hashes },
    };
    Some(RawString {
        text: body[..end].to_string(),
        form,
    })
}

/// The byte offset of the closing delimiter in an interpreted literal, stepping over
/// escapes and interpolations.
fn find_closer(body: &str, closer: &str) -> Option<usize> {
    let mut chars = body.char_indices().peekable();
    let mut depth = 0usize;

    while let Some((i, c)) = chars.next() {
        if depth > 0 {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                '\\' => {
                    chars.next();
                }
                _ => {}
            }
        } else if c == '\\' {
            if let Some(&(_, '(')) = chars.peek() {
                chars.next();
                depth = 1;
            } else {
                chars.next();
            }
        } else if body[i..].starts_with(closer) {
            return Some(i);
        }
    }
    None
}

fn lex_attribute(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if remainder.starts_with('(') {
        let mut depth = 0;
        let mut end = 0;
        for (i, c) in remainder.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        }
        if depth == 0 && end > 0 {
            lex.bump(end);
        }
    }
    Some(lex.slice().to_string())
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
            Token::Tilde => write!(f, "~"),
            Token::Ampersand => write!(f, "&"),
            Token::AndAnd => write!(f, "&&"),
            Token::Pipe => write!(f, "|"),
            Token::PipePipe => write!(f, "||"),
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
            Token::StringLit(s) => write!(f, "\"{}\"", s.text),
            Token::BytesLit(s) => write!(f, "'{}'", s.text),
            Token::Attribute(s) => write!(f, "{}", s),
        }
    }
}
