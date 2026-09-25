//! Numeric literal decoding. Integer spellings never pass through machine integers.
use crate::value::Value;
use num_bigint::BigInt;
use num_traits::Zero;

pub(crate) fn literal(text: &str) -> Result<Value, String> {
    let cleaned = text.replace('_', "");
    let invalid = || format!("invalid number '{text}'");
    for (prefix, radix) in [("0x", 16), ("0b", 2), ("0o", 8)] {
        if let Some(digits) = cleaned.strip_prefix(prefix) {
            return BigInt::parse_bytes(digits.as_bytes(), radix)
                .map(Value::Int)
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
                return Err(format!("number '{text}' cannot be represented as int"));
            }
            return Ok(Value::Int(scaled / divisor));
        }
    }
    if cleaned.contains(['.', 'e', 'E']) {
        let number: f64 = cleaned.parse().map_err(|_| invalid())?;
        if !number.is_finite() {
            return Err("number exceeds the evaluator's floating-point range".into());
        }
        return Ok(Value::Float(number));
    }
    cleaned
        .parse::<BigInt>()
        .map(Value::Int)
        .map_err(|_| invalid())
}
