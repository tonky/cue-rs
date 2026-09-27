//! The eight bound operators, shared by syntax positions and values.
//!
//! `UnaryOp` and `BinaryOp` describe where an operator was written; [`Bound`]
//! describes what it means as a constraint. Evaluated `Bounds` values store
//! this type, and the position enums convert via [`TryFrom`] — so the `<`
//! symbol and the bound semantics each live in exactly one place.

use crate::ast::{BinaryOp, UnaryOp};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bound {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    RegexMatch,
    RegexNotMatch,
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Less => write!(f, "<"),
            Self::LessEqual => write!(f, "<="),
            Self::Greater => write!(f, ">"),
            Self::GreaterEqual => write!(f, ">="),
            Self::Equal => write!(f, "=="),
            Self::NotEqual => write!(f, "!="),
            Self::RegexMatch => write!(f, "=~"),
            Self::RegexNotMatch => write!(f, "!~"),
        }
    }
}

/// A position operator with no bound meaning (`+`, `-`, `!`, `*`, `&`, `+`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotABound;

impl fmt::Display for NotABound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operator is not a bound")
    }
}

impl std::error::Error for NotABound {}

impl TryFrom<UnaryOp> for Bound {
    type Error = NotABound;

    fn try_from(op: UnaryOp) -> Result<Self, Self::Error> {
        match op {
            UnaryOp::Less => Ok(Self::Less),
            UnaryOp::LessEqual => Ok(Self::LessEqual),
            UnaryOp::Greater => Ok(Self::Greater),
            UnaryOp::GreaterEqual => Ok(Self::GreaterEqual),
            UnaryOp::Equal => Ok(Self::Equal),
            UnaryOp::NotEqual => Ok(Self::NotEqual),
            UnaryOp::RegexMatch => Ok(Self::RegexMatch),
            UnaryOp::RegexNotMatch => Ok(Self::RegexNotMatch),
            UnaryOp::Pos | UnaryOp::Neg | UnaryOp::Not | UnaryOp::Default => Err(NotABound),
        }
    }
}

impl TryFrom<BinaryOp> for Bound {
    type Error = NotABound;

    fn try_from(op: BinaryOp) -> Result<Self, Self::Error> {
        match op {
            BinaryOp::Less => Ok(Self::Less),
            BinaryOp::LessEqual => Ok(Self::LessEqual),
            BinaryOp::Greater => Ok(Self::Greater),
            BinaryOp::GreaterEqual => Ok(Self::GreaterEqual),
            BinaryOp::NotEqual => Ok(Self::NotEqual),
            BinaryOp::RegexMatch => Ok(Self::RegexMatch),
            BinaryOp::RegexNotMatch => Ok(Self::RegexNotMatch),
            _ => Err(NotABound),
        }
    }
}

impl From<Bound> for UnaryOp {
    fn from(bound: Bound) -> Self {
        match bound {
            Bound::Less => Self::Less,
            Bound::LessEqual => Self::LessEqual,
            Bound::Greater => Self::Greater,
            Bound::GreaterEqual => Self::GreaterEqual,
            Bound::Equal => Self::Equal,
            Bound::NotEqual => Self::NotEqual,
            Bound::RegexMatch => Self::RegexMatch,
            Bound::RegexNotMatch => Self::RegexNotMatch,
        }
    }
}

impl From<Bound> for BinaryOp {
    fn from(bound: Bound) -> Self {
        match bound {
            Bound::Less => Self::Less,
            Bound::LessEqual => Self::LessEqual,
            Bound::Greater => Self::Greater,
            Bound::GreaterEqual => Self::GreaterEqual,
            Bound::Equal => Self::Equal,
            Bound::NotEqual => Self::NotEqual,
            Bound::RegexMatch => Self::RegexMatch,
            Bound::RegexNotMatch => Self::RegexNotMatch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unary_roundtrips() {
        for bound in [
            Bound::Less,
            Bound::LessEqual,
            Bound::Greater,
            Bound::GreaterEqual,
            Bound::Equal,
            Bound::NotEqual,
            Bound::RegexMatch,
            Bound::RegexNotMatch,
        ] {
            assert_eq!(Bound::try_from(UnaryOp::from(bound)), Ok(bound));
            // `==` is a bound only in unary position; binary `==` stays
            // an ordinary comparison operator (see `non_bounds_rejected`).
            if bound == Bound::Equal {
                assert_eq!(Bound::try_from(BinaryOp::from(bound)), Err(NotABound));
            } else {
                assert_eq!(Bound::try_from(BinaryOp::from(bound)), Ok(bound));
            }
        }
    }

    #[test]
    fn non_bounds_rejected() {
        for op in [UnaryOp::Pos, UnaryOp::Neg, UnaryOp::Not, UnaryOp::Default] {
            assert_eq!(Bound::try_from(op), Err(NotABound));
        }
        for op in [
            BinaryOp::Unify,
            BinaryOp::Add,
            BinaryOp::Equal,
            BinaryOp::LogicalAnd,
        ] {
            assert_eq!(Bound::try_from(op), Err(NotABound));
        }
    }

    #[test]
    fn symbols() {
        assert_eq!(Bound::GreaterEqual.to_string(), ">=");
        assert_eq!(Bound::RegexNotMatch.to_string(), "!~");
    }

    #[test]
    fn wire_format_is_the_variant_name() {
        assert_eq!(
            serde_json::to_string(&Bound::LessEqual).unwrap(),
            r#""LessEqual""#
        );
        assert_eq!(
            serde_json::from_str::<Bound>(r#""RegexMatch""#).unwrap(),
            Bound::RegexMatch
        );
    }
}
