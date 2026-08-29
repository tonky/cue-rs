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
