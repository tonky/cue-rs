//! Concrete serialization must have one policy across exports and encoders.
use cue_eval::{Evaluator, Value, eval_to_json, export::json_to_yaml};
use serde_json::json;

#[test]
fn integers_keep_their_exact_json_number_type_across_all_conversion_routes() {
    for literal in [
        "9223372036854775808",
        "18446744073709551617",
        "-18446744073709551617",
        "12345678901234567890123456789012345678901234567890",
    ] {
        let source = format!(
            r#"
import "encoding/json"
x: {literal}
encoded: json.Marshal(x)
decoded: json.Unmarshal("{literal}")
"#
        );
        let value = eval_to_json(&source).unwrap();
        assert!(value["x"].is_number());
        assert_eq!(value["x"].to_string(), literal);
        assert_eq!(value["decoded"], value["x"]);
        assert_eq!(value["encoded"], literal);
    }
}

#[test]
fn byte_literals_preserve_octets_through_formatting_unification_and_export() {
    let source = r#"
import "encoding/hex"
import "encoding/json"
x: '\x00\xff'
x: '\000\377'
length: len(x)
hexadecimal: hex.Encode(x)
encoded: json.Marshal(x)
unicode: '\u00ff'
literal: 'ÿ'
raw: #'\#xff'#
"#;
    let expected = json!({"x":"AP8=", "length":2, "hexadecimal":"00ff", "encoded":"\"AP8=\"", "unicode":"w78=", "literal":"w78=", "raw":"/w=="});
    assert_eq!(eval_to_json(source).unwrap(), expected);
    let formatted = cue_syntax::format_file(&cue_syntax::parse_file(source).unwrap());
    assert_eq!(eval_to_json(&formatted).unwrap(), expected);
    assert!(eval_to_json("x: '\\xff' & '\\u00ff'").is_err());
}

#[test]
fn encoders_share_default_and_optional_field_export_rules() {
    let value = eval_to_json(
        r#"
import "encoding/json"
import "encoding/yaml"
x: {a: *5 | int, omitted?: 42, nested: {present: "yes", absent?: int}}
encoded: json.Marshal(x)
yamlText: yaml.Marshal(x)
list: [1, ...int]
listText: json.Marshal(list)
"#,
    )
    .unwrap();
    assert_eq!(value["x"], json!({"a":5,"nested":{"present":"yes"}}));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value["encoded"].as_str().unwrap()).unwrap(),
        value["x"]
    );
    let yaml: serde_json::Value =
        serde_yaml_ng::from_str(value["yamlText"].as_str().unwrap()).unwrap();
    assert_eq!(yaml, value["x"]);
    assert!(!value["yamlText"].as_str().unwrap().contains("$serde_json"));
    assert_eq!(value["list"], json!([1]));
    assert_eq!(value["listText"], "[1]");
    for choice in [r#""a" | "b""#, r#"*"a" | *"b""#] {
        assert!(
            eval_to_json(&format!(
                "import \"encoding/json\"\nx: json.Marshal({choice})"
            ))
            .is_err()
        );
    }
}

#[test]
fn yaml_serializes_numeric_scalars_without_jsons_private_protocol() {
    let value = eval_to_json("x: 42\ny: 1.25\nz: -18446744073709551617\ntext: \"42\"").unwrap();
    let yaml = json_to_yaml(&value).unwrap();
    assert!(yaml.contains("x: 42\n"), "{yaml}");
    assert!(yaml.contains("z: -18446744073709551617\n"), "{yaml}");
    assert!(!yaml.contains("$serde_json"), "{yaml}");
    assert!(yaml.contains("text: '42'\n"), "{yaml}");
    let value = eval_to_json("x: 12345678901234567890123456789012345678901234567890").unwrap();
    let yaml_flow = json_to_yaml(&value).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&yaml_flow).unwrap(),
        value
    );
}

#[test]
fn nonfinite_runtime_values_are_errors_instead_of_json_null() {
    let mut evaluator = Evaluator::new();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let id = evaluator.arena.alloc(Value::Float(value));
        let error = evaluator.to_json(id).unwrap_err();
        assert!(error.contains("numeric range"), "{error}");
        assert!(cue_eval::stdlib::value_to_json(&evaluator.arena, id).is_err());
    }
}
