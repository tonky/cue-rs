use cue_derive::CueValidate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, CueValidate)]
#[cue(schema = "#ServerConfig: { host: string, port: int & >0 & <65535, tls: bool }")]
struct ServerConfig {
    host: String,
    port: u16,
    tls: bool,
}

#[test]
fn test_cue_validate_success() {
    let valid = ServerConfig {
        host: "localhost".to_string(),
        port: 8080,
        tls: true,
    };
    assert!(valid.cue_validate().is_ok());
}

#[test]
fn test_cue_validate_failure() {
    let invalid = ServerConfig {
        host: "localhost".to_string(),
        port: 0,
        tls: true,
    };
    assert!(invalid.cue_validate().is_err());
}

#[derive(Debug, Serialize, Deserialize, CueValidate)]
#[cue(schema = "#Container: { item: string, count: int & >=0 }")]
struct Container<T: Serialize> {
    item: T,
    count: u32,
}

fn validate_generic<T: cue_eval::CueValidate>(val: &T) -> Result<(), String> {
    val.cue_validate()
}

#[test]
fn test_cue_validate_generic() {
    let c = Container {
        item: "data".to_string(),
        count: 10,
    };
    assert!(validate_generic(&c).is_ok());
}
