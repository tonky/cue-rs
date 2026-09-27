//! Validated numeric literals: parse, don't validate.
//!
//! A [`NumberLit`] has been decoded once, at parse time, so evaluation maps it
//! to a value without re-parsing or failing. The original spelling rides along
//! because the formatter re-emits literals verbatim (`0x2A` stays `0x2A`).

use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A decoded numeric value. Integer spellings never pass through machine integers.
#[derive(Debug, Clone, PartialEq)]
pub enum NumberValue {
    Int(BigInt),
    Float(f64),
}

/// Why a numeric spelling is not a usable literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberError {
    /// Not a number in any supported spelling.
    Invalid { text: String },
    /// An SI-scaled fraction with no exact integer value, e.g. a fractional
    /// `Ki` that does not divide evenly.
    NotRepresentable { text: String },
    /// A float spelling outside the evaluator's finite `f64` range.
    FloatOutOfRange,
}

impl fmt::Display for NumberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { text } => write!(f, "invalid number '{text}'"),
            Self::NotRepresentable { text } => {
                write!(f, "number '{text}' cannot be represented as int")
            }
            Self::FloatOutOfRange => {
                write!(f, "number exceeds the evaluator's floating-point range")
            }
        }
    }
}

impl std::error::Error for NumberError {}

/// A numeric literal: its source spelling plus its decoded value.
///
/// Equality and hashing are syntactic — by spelling, not by value — matching
/// the AST rule that derived equality compares syntax (`0x2A` and `42` are
/// different spellings of one value). The decoded value is cargo, not key.
#[derive(Debug, Clone)]
pub struct NumberLit {
    text: String,
    value: NumberValue,
}

impl NumberLit {
    /// Decode a lexer-provided numeric spelling. Called by the parser only.
    pub fn parse(text: String) -> Result<Self, NumberError> {
        let value = decode(&text)?;
        Ok(Self { text, value })
    }

    /// The original spelling, for verbatim re-emission.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The decoded value, for evaluation.
    pub fn value(&self) -> &NumberValue {
        &self.value
    }
}

impl PartialEq for NumberLit {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for NumberLit {}

impl std::hash::Hash for NumberLit {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl AsRef<str> for NumberLit {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for NumberLit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The wire format is the spelling string, unchanged from `Number(String)`.
/// Deserializing re-parses, so a deserialized literal upholds the invariant too.
impl Serialize for NumberLit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for NumberLit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(text).map_err(serde::de::Error::custom)
    }
}

fn decode(text: &str) -> Result<NumberValue, NumberError> {
    let cleaned = text.replace('_', "");
    let invalid = || NumberError::Invalid {
        text: text.to_string(),
    };
    for (prefix, radix) in [("0x", 16), ("0b", 2), ("0o", 8)] {
        if let Some(digits) = cleaned.strip_prefix(prefix) {
            return BigInt::parse_bytes(digits.as_bytes(), radix)
                .map(NumberValue::Int)
                .ok_or_else(invalid);
        }
    }
    for (suffix, exponent) in [("K", 1), ("M", 2), ("G", 3), ("T", 4), ("P", 5)] {
        let binary_suffix = format!("{suffix}i");
        let scaled = cleaned
            .strip_suffix(&binary_suffix)
            .map(|base| (base, 1024u32))
            .or_else(|| cleaned.strip_suffix(suffix).map(|base| (base, 1000u32)));
        if let Some((base, scale)) = scaled {
            let (whole, fraction) = base.split_once('.').unwrap_or((base, ""));
            let digits = format!("{whole}{fraction}");
            if !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            let coefficient = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(invalid)?;
            let divisor =
                BigInt::from(10u8).pow(u32::try_from(fraction.len()).map_err(|_| invalid())?);
            let scaled = coefficient * BigInt::from(scale).pow(exponent);
            if !(&scaled % &divisor).is_zero() {
                return Err(NumberError::NotRepresentable {
                    text: text.to_string(),
                });
            }
            return Ok(NumberValue::Int(scaled / divisor));
        }
    }
    if cleaned.contains(['.', 'e', 'E']) {
        let number: f64 = cleaned.parse().map_err(|_| invalid())?;
        if !number.is_finite() {
            return Err(NumberError::FloatOutOfRange);
        }
        return Ok(NumberValue::Float(number));
    }
    cleaned
        .parse::<BigInt>()
        .map(NumberValue::Int)
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_and_underscores() {
        assert!(matches!(
            NumberLit::parse("42".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
        assert_eq!(
            NumberLit::parse("1_000".to_string()).unwrap().as_str(),
            "1_000"
        );
    }

    #[test]
    fn radix_spellings() {
        assert!(matches!(
            NumberLit::parse("0x2A".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
        assert!(matches!(
            NumberLit::parse("0b1100".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
        assert!(matches!(
            NumberLit::parse("0o755".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
        assert!(NumberLit::parse("0x".to_string()).is_err());
    }

    #[test]
    fn si_suffixes() {
        assert!(matches!(
            NumberLit::parse("4Ki".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
        assert!(matches!(
            NumberLit::parse("10M".to_string()).unwrap().value(),
            NumberValue::Int(_)
        ));
    }

    #[test]
    fn floats() {
        assert!(matches!(
            NumberLit::parse("12.5".to_string()).unwrap().value(),
            NumberValue::Float(_)
        ));
        assert!(matches!(
            NumberLit::parse("1e3".to_string()).unwrap().value(),
            NumberValue::Float(_)
        ));
        assert_eq!(
            NumberLit::parse("1e999".to_string()).unwrap_err(),
            NumberError::FloatOutOfRange
        );
    }

    #[test]
    fn invalid_spellings_rejected() {
        assert!(matches!(
            NumberLit::parse("abc".to_string()).unwrap_err(),
            NumberError::Invalid { .. }
        ));
        assert!(matches!(
            NumberLit::parse("12Kx".to_string()).unwrap_err(),
            NumberError::Invalid { .. }
        ));
    }

    #[test]
    fn equality_is_syntactic() {
        let hex = NumberLit::parse("0x2A".to_string()).unwrap();
        let dec = NumberLit::parse("42".to_string()).unwrap();
        assert_ne!(hex, dec);
        assert_eq!(hex, NumberLit::parse("0x2A".to_string()).unwrap());
    }

    #[test]
    fn wire_format_is_the_spelling_string() {
        let lit = NumberLit::parse("0x2A".to_string()).unwrap();
        assert_eq!(serde_json::to_string(&lit).unwrap(), r#""0x2A""#);
        let back: NumberLit = serde_json::from_str(r#""4Ki""#).unwrap();
        assert!(matches!(back.value(), NumberValue::Int(_)));
        assert!(serde_json::from_str::<NumberLit>(r#""abc""#).is_err());
    }
}
