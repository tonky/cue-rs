use cue_eval::eval_to_json;
use cue_syntax::parse_file;
use cue_syntax::parser::ParseError;

#[test]
fn test_parser_recursion_depth_limit() {
    let depth = 70;
    let payload = format!("x: {}1{}", "(".repeat(depth), ")".repeat(depth));
    let result = parse_file(&payload);
    assert!(
        matches!(result, Err(ParseError::RecursionDepthExceeded { .. })),
        "expected RecursionDepthExceeded, got {result:?}"
    );
}

#[test]
fn test_list_repeat_allocation_limit() {
    let src = r#"
        import "list"
        x: list.Repeat(["a"], 2000000)
    "#;
    let result = eval_to_json(src);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds maximum allowed elements"),
        "unexpected error message: {err_msg}"
    );
}

#[test]
fn test_strings_repeat_allocation_limit() {
    let src = r#"
        import "strings"
        x: strings.Repeat("a", 20000000)
    "#;
    let result = eval_to_json(src);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds maximum allowed bytes"),
        "unexpected error message: {err_msg}"
    );
}

#[test]
fn test_validate_json_with_rederived_defaults() {
    let schema = r#"
        #Config: {
            port: int | *8080
            url: "http://localhost:\(port)"
        }
    "#;
    let data = serde_json::json!({
        "port": 9000
    });
    if let Err(e) = cue_eval::validate_json(schema, &data) {
        panic!("validate_json failed: {e}");
    }
}
