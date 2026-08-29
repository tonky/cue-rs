use crate::ast::*;
use crate::token::Token;
use logos::Logos;
use std::ops::Range;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("Unexpected end of file")]
    UnexpectedEof,
    #[error("Unexpected token '{found}' at {span:?}, expected {expected}")]
    UnexpectedToken {
        found: String,
        expected: String,
        span: Range<usize>,
    },
    #[error("Lexer error at {span:?}")]
    LexerError { span: Range<usize> },
}

pub struct Parser<'a> {
    tokens: Vec<(Token, Range<usize>)>,
    pos: usize,
    _source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Token::lexer(source);
        let mut raw_tokens = Vec::new();

        while let Some(token_res) = lexer.next() {
            let span = lexer.span();
            match token_res {
                Ok(tok) => raw_tokens.push((tok, span)),
                Err(_) => return Err(ParseError::LexerError { span }),
            }
        }

        let mut tokens = Vec::with_capacity(raw_tokens.len() * 2);
        for i in 0..raw_tokens.len() {
            let (ref tok, ref span) = raw_tokens[i];
            tokens.push((tok.clone(), span.clone()));

            if i + 1 < raw_tokens.len() {
                let next_span = &raw_tokens[i + 1].1;
                if span.end < next_span.start {
                    let inter_slice = &source[span.end..next_span.start];
                    if inter_slice.contains('\n') && Self::can_end_statement(tok) {
                        tokens.push((Token::Comma, span.end..span.end));
                    }
                }
            }
        }

        Ok(Self {
            tokens,
            pos: 0,
            _source: source,
        })
    }

    fn can_end_statement(tok: &Token) -> bool {
        matches!(
            tok,
            Token::Ident(_)
                | Token::DefIdent(_)
                | Token::HiddenIdent(_)
                | Token::HiddenDefIdent(_)
                | Token::Number(_)
                | Token::StringLit(_)
                | Token::MultiStringLit(_)
                | Token::BytesLit(_)
                | Token::KwTrue
                | Token::KwFalse
                | Token::KwNull
                | Token::Top
                | Token::Bottom
                | Token::RParen
                | Token::RBracket
                | Token::RBrace
                | Token::Question
        )
    }

    pub fn parse_file(&mut self) -> Result<SourceFile, ParseError> {
        let package = self.parse_package_opt()?;
        let imports = self.parse_imports_opt()?;
        let decls = self.parse_decls_until(|p| p.is_eof())?;

        Ok(SourceFile {
            package,
            imports,
            decls,
        })
    }

    pub fn parse_expr_str(source: &'a str) -> Result<Expr, ParseError> {
        let mut parser = Parser::new(source)?;
        let expr = parser.parse_expr()?;
        if !parser.is_eof() {
            let (tok, span) = parser.peek_token()?;
            return Err(ParseError::UnexpectedToken {
                found: format!("{}", tok),
                expected: "end of expression".to_string(),
                span,
            });
        }
        Ok(expr)
    }

    // --- Helpers ---

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn peek_token(&self) -> Result<(Token, Range<usize>), ParseError> {
        self.tokens
            .get(self.pos)
            .cloned()
            .ok_or(ParseError::UnexpectedEof)
    }

    fn advance(&mut self) -> Result<(Token, Range<usize>), ParseError> {
        let tok = self.peek_token()?;
        self.pos += 1;
        Ok(tok)
    }

    fn match_token(&mut self, expected: &Token) -> bool {
        if let Some(tok) = self.peek()
            && tok == expected {
                self.pos += 1;
                return true;
            }
        false
    }

    fn expect(&mut self, expected: Token) -> Result<Range<usize>, ParseError> {
        let (found, span) = self.peek_token()?;
        if found == expected {
            self.pos += 1;
            Ok(span)
        } else {
            Err(ParseError::UnexpectedToken {
                found: format!("{}", found),
                expected: format!("{}", expected),
                span,
            })
        }
    }

    // --- Declarations ---

    fn parse_package_opt(&mut self) -> Result<Option<String>, ParseError> {
        if self.match_token(&Token::KwPackage) {
            let (tok, span) = self.advance()?;
            match tok {
                Token::Ident(name) => {
                    self.match_token(&Token::Comma); // optional comma/newline
                    Ok(Some(name))
                }
                _ => Err(ParseError::UnexpectedToken {
                    found: format!("{}", tok),
                    expected: "package identifier".to_string(),
                    span,
                }),
            }
        } else {
            Ok(None)
        }
    }

    fn parse_imports_opt(&mut self) -> Result<Vec<ImportDecl>, ParseError> {
        let mut imports = Vec::new();
        while self.match_token(&Token::KwImport) {
            if self.match_token(&Token::LParen) {
                while !self.match_token(&Token::RParen) && !self.is_eof() {
                    imports.push(self.parse_import_spec()?);
                    self.match_token(&Token::Comma);
                }
            } else {
                imports.push(self.parse_import_spec()?);
                self.match_token(&Token::Comma);
            }
        }
        Ok(imports)
    }

    fn parse_import_spec(&mut self) -> Result<ImportDecl, ParseError> {
        let (tok, span) = self.advance()?;
        match tok {
            Token::StringLit(path) => Ok(ImportDecl { path, alias: None }),
            Token::Ident(alias) => {
                let (path_tok, path_span) = self.advance()?;
                if let Token::StringLit(path) = path_tok {
                    Ok(ImportDecl {
                        path,
                        alias: Some(alias),
                    })
                } else {
                    Err(ParseError::UnexpectedToken {
                        found: format!("{}", path_tok),
                        expected: "import path string".to_string(),
                        span: path_span,
                    })
                }
            }
            _ => Err(ParseError::UnexpectedToken {
                found: format!("{}", tok),
                expected: "import path".to_string(),
                span,
            }),
        }
    }

    fn parse_decls_until<F>(&mut self, until: F) -> Result<Vec<Decl>, ParseError>
    where
        F: Fn(&Parser) -> bool,
    {
        let mut decls = Vec::new();
        while !until(self) && !self.is_eof() {
            // Optional separator commas
            if self.match_token(&Token::Comma) {
                continue;
            }
            decls.push(self.parse_decl()?);
            self.match_token(&Token::Comma);
        }
        Ok(decls)
    }

    pub fn parse_decl(&mut self) -> Result<Decl, ParseError> {
        // 1. Let clause: `let x = expr`
        if self.match_token(&Token::KwLet) {
            let (tok, span) = self.advance()?;
            let ident = match tok {
                Token::Ident(id) | Token::DefIdent(id) => id,
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        found: format!("{}", tok),
                        expected: "identifier after let".to_string(),
                        span,
                    })
                }
            };
            self.expect(Token::Equal)?;
            let expr = self.parse_expr()?;
            return Ok(Decl::Let { ident, expr });
        }

        // 2. Ellipsis: `...` or `...string`
        if self.match_token(&Token::Ellipsis) {
            let val = if self.is_eof()
                || matches!(
                    self.peek(),
                    Some(Token::RBrace | Token::RBracket | Token::Comma)
                ) {
                None
            } else {
                Some(self.parse_expr()?)
            };
            return Ok(Decl::Ellipsis(val));
        }

        // 3. For comprehension: `for k, v in src if cond { ... }`
        if self.match_token(&Token::KwFor) {
            return self.parse_for_comprehension();
        }

        // 4. If clause: `if cond { ... }`
        if self.match_token(&Token::KwIf) {
            let cond = self.parse_expr()?;
            return self.parse_comprehension_clauses(ComprehensionClause::If { condition: cond });
        }

        // 5. Lookahead for Field, Alias, or Embedding
        // An alias is: `X = expr` (Capitalized ident followed by `=`)
        if let Some((Token::Ident(id), _)) = self.tokens.get(self.pos)
            && self.tokens.get(self.pos + 1).map(|(t, _)| t) == Some(&Token::Equal) {
                let id = id.clone();
                self.pos += 2; // consume ident and '='
                let expr = self.parse_expr()?;
                return Ok(Decl::Alias { ident: id, expr });
            }

        // Check if this looks like a field label: `label: ...` or `label?: ...`
        if self.is_label_ahead() {
            return self.parse_field_decl();
        }

        // Otherwise, it's an embedded expression
        let expr = self.parse_expr()?;
        Ok(Decl::Embedding(expr))
    }

    fn is_label_ahead(&self) -> bool {
        let mut idx = self.pos;
        if idx >= self.tokens.len() {
            return false;
        }

        // Dynamic or pattern label: `[expr]:`
        if self.tokens[idx].0 == Token::LBracket {
            let mut depth = 1;
            idx += 1;
            while idx < self.tokens.len() && depth > 0 {
                match &self.tokens[idx].0 {
                    Token::LBracket => depth += 1,
                    Token::RBracket => depth -= 1,
                    _ => {}
                }
                idx += 1;
            }
            if idx < self.tokens.len() {
                if self.tokens[idx].0 == Token::Question {
                    idx += 1;
                }
                return idx < self.tokens.len() && self.tokens[idx].0 == Token::Colon;
            }
            return false;
        }

        // Dynamic evaluated label: `(expr):`
        if self.tokens[idx].0 == Token::LParen {
            let mut depth = 1;
            idx += 1;
            while idx < self.tokens.len() && depth > 0 {
                match &self.tokens[idx].0 {
                    Token::LParen => depth += 1,
                    Token::RParen => depth -= 1,
                    _ => {}
                }
                idx += 1;
            }
            if idx < self.tokens.len() {
                if self.tokens[idx].0 == Token::Question {
                    idx += 1;
                }
                return idx < self.tokens.len() && self.tokens[idx].0 == Token::Colon;
            }
            return false;
        }

        // Simple label: ident, string, def_ident
        match &self.tokens[idx].0 {
            Token::Ident(_)
            | Token::DefIdent(_)
            | Token::HiddenIdent(_)
            | Token::HiddenDefIdent(_)
            | Token::StringLit(_) => {
                idx += 1;
                if idx < self.tokens.len() && self.tokens[idx].0 == Token::Question {
                    idx += 1;
                }
                idx < self.tokens.len() && self.tokens[idx].0 == Token::Colon
            }
            _ => false,
        }
    }

    fn parse_field_decl(&mut self) -> Result<Decl, ParseError> {
        let label = self.parse_label()?;
        let optional = self.match_token(&Token::Question);
        self.expect(Token::Colon)?;

        // Support label syntactic sugar nesting: `a: b: c: 1`
        let value = if self.is_label_ahead() {
            let inner_field = self.parse_field_decl()?;
            Expr::Struct(StructLit {
                decls: vec![inner_field],
            })
        } else {
            self.parse_expr()?
        };

        // Parse optional trailing attributes: @tag(val)
        let mut attrs = Vec::new();
        while let Some((Token::Attribute(raw), _)) = self.tokens.get(self.pos) {
            let raw = raw.clone();
            self.pos += 1;
            let (name, body) = if let Some(open) = raw.find('(') {
                let name = raw[1..open].to_string();
                let body = if raw.ends_with(')') {
                    raw[open + 1..raw.len() - 1].to_string()
                } else {
                    raw[open + 1..].to_string()
                };
                (name, body)
            } else {
                let name = raw[1..].to_string();
                (name, String::new())
            };
            attrs.push(Attribute { name, body });
        }

        Ok(Decl::Field(FieldDecl {
            label,
            optional,
            value,
            attrs,
        }))
    }

    fn parse_label(&mut self) -> Result<Label, ParseError> {
        if self.match_token(&Token::LBracket) {
            let expr = self.parse_expr()?;
            self.expect(Token::RBracket)?;
            return Ok(Label::Pattern(expr));
        }

        if self.match_token(&Token::LParen) {
            let expr = self.parse_expr()?;
            self.expect(Token::RParen)?;
            return Ok(Label::Dynamic(expr));
        }

        let (tok, span) = self.advance()?;
        match tok {
            Token::Ident(s) => Ok(Label::Ident(s)),
            Token::DefIdent(s) => Ok(Label::DefIdent(s)),
            Token::HiddenIdent(s) => Ok(Label::HiddenIdent(s)),
            Token::HiddenDefIdent(s) => Ok(Label::HiddenDefIdent(s)),
            Token::StringLit(s) => {
                if s.contains(r"\(") {
                    let expr = Self::parse_string_lit(s)?;
                    Ok(Label::Dynamic(expr))
                } else {
                    Ok(Label::String(s))
                }
            }
            _ => Err(ParseError::UnexpectedToken {
                found: format!("{}", tok),
                expected: "field label".to_string(),
                span,
            }),
        }
    }

    fn parse_for_comprehension(&mut self) -> Result<Decl, ParseError> {
        let (first_tok, span) = self.advance()?;
        let (key, val) = match first_tok {
            Token::Ident(k_or_v) => {
                if self.match_token(&Token::Comma) {
                    let (val_tok, val_span) = self.advance()?;
                    if let Token::Ident(v) = val_tok {
                        (Some(k_or_v), v)
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            found: format!("{}", val_tok),
                            expected: "value identifier in for-loop".to_string(),
                            span: val_span,
                        });
                    }
                } else {
                    (None, k_or_v)
                }
            }
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: format!("{}", first_tok),
                    expected: "identifier in for-loop".to_string(),
                    span,
                });
            }
        };

        self.expect(Token::KwIn)?;
        let source = self.parse_expr()?;
        self.parse_comprehension_clauses(ComprehensionClause::For {
            key,
            value: val,
            source,
        })
    }

    fn parse_comprehension_clauses(
        &mut self,
        first_clause: ComprehensionClause,
    ) -> Result<Decl, ParseError> {
        let mut clauses = vec![first_clause];

        while !self.is_eof() && self.peek() != Some(&Token::LBrace) {
            if self.match_token(&Token::KwFor) {
                let (first_tok, span) = self.advance()?;
                let (key, val) = match first_tok {
                    Token::Ident(k_or_v) => {
                        if self.match_token(&Token::Comma) {
                            let (val_tok, val_span) = self.advance()?;
                            if let Token::Ident(v) = val_tok {
                                (Some(k_or_v), v)
                            } else {
                                return Err(ParseError::UnexpectedToken {
                                    found: format!("{}", val_tok),
                                    expected: "value identifier in for-loop".to_string(),
                                    span: val_span,
                                });
                            }
                        } else {
                            (None, k_or_v)
                        }
                    }
                    _ => {
                        return Err(ParseError::UnexpectedToken {
                            found: format!("{}", first_tok),
                            expected: "identifier in for-loop".to_string(),
                            span,
                        });
                    }
                };
                self.expect(Token::KwIn)?;
                let source = self.parse_expr()?;
                clauses.push(ComprehensionClause::For {
                    key,
                    value: val,
                    source,
                });
            } else if self.match_token(&Token::KwIf) {
                let condition = self.parse_expr()?;
                clauses.push(ComprehensionClause::If { condition });
            } else if self.match_token(&Token::KwLet) {
                let (tok, span) = self.advance()?;
                let ident = match tok {
                    Token::Ident(id) | Token::DefIdent(id) => id,
                    _ => {
                        return Err(ParseError::UnexpectedToken {
                            found: format!("{}", tok),
                            expected: "identifier after let".to_string(),
                            span,
                        });
                    }
                };
                self.expect(Token::Equal)?;
                let expr = self.parse_expr()?;
                clauses.push(ComprehensionClause::Let { ident, expr });
            } else {
                break;
            }
        }

        self.expect(Token::LBrace)?;
        let decls = self.parse_decls_until(|p| p.peek() == Some(&Token::RBrace))?;
        self.expect(Token::RBrace)?;

        Ok(Decl::Comprehension(ComprehensionDecl {
            clauses,
            struct_lit: StructLit { decls },
        }))
    }

    // --- Expressions (Pratt Precedence) ---

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_disjunction()
    }

    fn parse_disjunction(&mut self) -> Result<Expr, ParseError> {
        let mut branches = Vec::new();

        loop {
            let default = self.match_token(&Token::Star);
            let expr = self.parse_unification()?;
            branches.push(DisjunctionBranch { default, expr });

            if !self.match_token(&Token::Pipe) {
                break;
            }
        }

        if branches.len() == 1 && !branches[0].default {
            Ok(branches.pop().unwrap().expr)
        } else {
            Ok(Expr::Disjunction { branches })
        }
    }

    fn parse_unification(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        while self.match_token(&Token::Ampersand) {
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: BinaryOp::Unify,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;

        while let Some(op) = self.peek_cmp_op() {
            self.pos += 1; // consume op
            let right = self.parse_addition()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn peek_cmp_op(&self) -> Option<BinaryOp> {
        match self.peek()? {
            Token::EqualEqual => Some(BinaryOp::Equal),
            Token::NotEqual => Some(BinaryOp::NotEqual),
            Token::Less => Some(BinaryOp::Less),
            Token::LessEqual => Some(BinaryOp::LessEqual),
            Token::Greater => Some(BinaryOp::Greater),
            Token::GreaterEqual => Some(BinaryOp::GreaterEqual),
            Token::RegexMatch => Some(BinaryOp::RegexMatch),
            Token::RegexNotMatch => Some(BinaryOp::RegexNotMatch),
            _ => None,
        }
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.parse_multiplication()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if let Some(tok) = self.peek() {
            let unary_op = match tok {
                Token::Plus => Some(UnaryOp::Pos),
                Token::Minus => Some(UnaryOp::Neg),
                Token::Bang => Some(UnaryOp::Not),
                Token::Star => Some(UnaryOp::Default),
                Token::Less => Some(UnaryOp::Less),
                Token::LessEqual => Some(UnaryOp::LessEqual),
                Token::Greater => Some(UnaryOp::Greater),
                Token::GreaterEqual => Some(UnaryOp::GreaterEqual),
                Token::NotEqual => Some(UnaryOp::NotEqual),
                Token::RegexMatch => Some(UnaryOp::RegexMatch),
                Token::RegexNotMatch => Some(UnaryOp::RegexNotMatch),
                _ => None,
            };

            if let Some(op) = unary_op {
                self.pos += 1;
                let expr = self.parse_unary()?;
                return Ok(Expr::Unary {
                    op,
                    expr: Box::new(expr),
                });
            }
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&Token::Dot) {
                // Selector
                let (tok, span) = self.advance()?;
                match tok {
                    Token::Ident(field)
                    | Token::DefIdent(field)
                    | Token::HiddenIdent(field)
                    | Token::HiddenDefIdent(field)
                    | Token::StringLit(field) => {
                        expr = Expr::Selector {
                            expr: Box::new(expr),
                            field,
                        };
                    }
                    _ => {
                        return Err(ParseError::UnexpectedToken {
                            found: format!("{}", tok),
                            expected: "field selector name".to_string(),
                            span,
                        })
                    }
                }
            } else if self.match_token(&Token::LBracket) {
                // Index or Slice
                let low = if self.peek() == Some(&Token::Colon) {
                    None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };

                if self.match_token(&Token::Colon) {
                    let high = if self.peek() == Some(&Token::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(Token::RBracket)?;
                    expr = Expr::Slice {
                        expr: Box::new(expr),
                        low,
                        high,
                    };
                } else {
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index {
                        expr: Box::new(expr),
                        index: low.unwrap(),
                    };
                }
            } else if self.match_token(&Token::LParen) {
                // Function call
                let mut args = Vec::new();
                while !self.match_token(&Token::RParen) && !self.is_eof() {
                    args.push(self.parse_expr()?);
                    self.match_token(&Token::Comma);
                }
                expr = Expr::Call {
                    func: Box::new(expr),
                    args,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let (tok, span) = self.advance()?;
        match tok {
            Token::Bottom => Ok(Expr::Bottom),
            Token::Top => Ok(Expr::Top),
            Token::KwNull => Ok(Expr::Null),
            Token::KwTrue => Ok(Expr::Bool(true)),
            Token::KwFalse => Ok(Expr::Bool(false)),
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::StringLit(s) => Self::parse_string_lit(s),
            Token::MultiStringLit(s) => Self::parse_string_lit(s),
            Token::BytesLit(b) => Ok(Expr::Bytes(b)),
            Token::Ident(id) => Ok(Expr::Ident(id)),
            Token::DefIdent(id) => Ok(Expr::DefIdent(id)),
            Token::HiddenIdent(id) => Ok(Expr::HiddenIdent(id)),
            Token::HiddenDefIdent(id) => Ok(Expr::HiddenDefIdent(id)),
            Token::LBrace => {
                let decls = self.parse_decls_until(|p| p.peek() == Some(&Token::RBrace))?;
                self.expect(Token::RBrace)?;
                Ok(Expr::Struct(StructLit { decls }))
            }
            Token::LBracket => {
                let mut elements = Vec::new();
                let mut ellipsis = None;

                while !self.match_token(&Token::RBracket) && !self.is_eof() {
                    if self.match_token(&Token::Ellipsis) {
                        let elem_type = if self.peek() == Some(&Token::RBracket) {
                            None
                        } else {
                            Some(Box::new(self.parse_expr()?))
                        };
                        ellipsis = elem_type;
                        self.match_token(&Token::Comma);
                        self.expect(Token::RBracket)?;
                        break;
                    }
                    elements.push(self.parse_expr()?);
                    self.match_token(&Token::Comma);
                }
                Ok(Expr::List(ListLit { elements, ellipsis }))
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            _ => Err(ParseError::UnexpectedToken {
                found: format!("{}", tok),
                expected: "primary expression (literal, identifier, struct, or list)".to_string(),
                span,
            }),
        }
    }

    fn parse_string_lit(s: String) -> Result<Expr, ParseError> {
        if !s.contains(r"\(") {
            return Ok(Expr::String(s));
        }

        let mut parts = Vec::new();
        let mut cur = s.as_str();

        while let Some(start) = cur.find(r"\(") {
            let prefix = &cur[..start];
            if !prefix.is_empty() {
                parts.push(InterpolationPart::Lit(prefix.to_string()));
            }

            let after_open = &cur[start + 2..];
            let mut depth = 1;
            let mut end_idx = None;

            for (idx, ch) in after_open.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(idx);
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(end) = end_idx {
                let inner_expr_str = &after_open[..end];
                let expr = Parser::parse_expr_str(inner_expr_str)?;
                parts.push(InterpolationPart::Expr(Box::new(expr)));
                cur = &after_open[end + 1..];
            } else {
                parts.push(InterpolationPart::Lit(cur.to_string()));
                break;
            }
        }

        if !cur.is_empty() {
            parts.push(InterpolationPart::Lit(cur.to_string()));
        }

        Ok(Expr::Interpolation { parts })
    }
}
