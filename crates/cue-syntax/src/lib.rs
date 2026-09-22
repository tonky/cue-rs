pub mod ast;
pub mod formatter;
pub mod parser;
pub mod token;
pub mod visitor;

pub use ast::*;
pub use formatter::format_file;
pub use parser::{ParseError, Parser};
pub use token::Token;
pub use visitor::{Folder, Visitor};

/// Parse a CUE source string into a `SourceFile`.
pub fn parse_file(source: &str) -> Result<SourceFile, ParseError> {
    let mut parser = Parser::new(source)?;
    parser.parse_file()
}

/// Parse a CUE expression string.
pub fn parse_expr(source: &str) -> Result<Expr, ParseError> {
    Parser::parse_expr_str(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_roundtrip() {
        let src = r#"package test

#Config: {
    port: int & >1024
    tls: *true | false
}
"#;
        let file = parse_file(src).unwrap();
        let formatted = format_file(&file);
        assert!(formatted.contains("package test"));
        assert!(formatted.contains("#Config: {"));
        assert!(formatted.contains("port: int & >1024"));
    }

    #[test]
    fn test_parse_simple_struct() {
        let src = r#"
            package test

            #Schema: {
                name: string
                age?: int
                role: *"user" | "admin"
            }

            user1: #Schema & {
                name: "Alice"
                age: 30
            }
        "#;
        let file = parse_file(src).unwrap();
        assert_eq!(file.package, Some("test".to_string()));
        let fields = file
            .decls
            .iter()
            .filter(|d| matches!(d, Decl::Field(_)))
            .count();
        assert_eq!(fields, 2);
        // The blank line between the two is a declaration of its own now, so that the
        // formatter can put it back.
        assert!(file.decls.contains(&Decl::BlankLine));
    }

    #[test]
    fn test_parse_nested_fields() {
        let src = "a: b: c: 42";
        let file = parse_file(src).unwrap();
        assert_eq!(file.decls.len(), 1);
        if let Decl::Field(f) = &file.decls[0] {
            assert_eq!(f.label.name(), Some("a"));
            if let Expr::Struct(inner) = &f.value {
                assert_eq!(inner.decls.len(), 1);
            } else {
                panic!("Expected nested struct");
            }
        } else {
            panic!("Expected field");
        }
    }

    #[test]
    fn test_parse_disjunction_and_unification() {
        let expr_src = r#"int & >0 & <=100 | *"default""#;
        let expr = parse_expr(expr_src).unwrap();
        match expr {
            Expr::Disjunction { branches } => {
                assert_eq!(branches.len(), 2);
                assert!(!branches[0].default);
                assert!(branches[1].default);
            }
            _ => panic!("Expected disjunction"),
        }
    }

    #[test]
    fn test_parse_list_and_ellipsis() {
        let src = r#"[1, 2, 3, ...int]"#;
        let expr = parse_expr(src).unwrap();
        if let Expr::List(l) = expr {
            assert_eq!(l.elements.len(), 3);
            assert!(l.ellipsis.is_some());
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_rich_diagnostic_formatting() {
        let bad_src = "a: 1\nb: @invalid\nc: 3";
        let err = parse_file(bad_src).unwrap_err();
        let diag = err.format_with_source(bad_src, Some("config.cue"));
        assert!(diag.contains("--> config.cue:2:"));
        assert!(diag.contains("b: @invalid"));
        assert!(diag.contains("^"));
    }

    #[test]
    fn test_parse_keyword_labels() {
        let src = "package: \"postgresql\"\nimport: \"pkg\"\nlet: 42\nfor: true\n";
        let file = parse_file(src).expect("should parse keywords as field labels");
        assert_eq!(file.decls.len(), 4);
    }
}
