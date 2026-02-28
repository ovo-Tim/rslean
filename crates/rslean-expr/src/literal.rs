use num_bigint::BigUint;
use std::fmt;

/// Lean 4 literal values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Nat(BigUint),
    Str(String),
}

impl Literal {
    pub fn nat(n: impl Into<BigUint>) -> Self {
        Literal::Nat(n.into())
    }

    pub fn nat_small(n: u64) -> Self {
        Literal::Nat(BigUint::from(n))
    }

    pub fn string(s: impl Into<String>) -> Self {
        Literal::Str(s.into())
    }

    pub fn is_nat(&self) -> bool {
        matches!(self, Literal::Nat(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Literal::Str(_))
    }

    pub fn get_nat(&self) -> &BigUint {
        match self {
            Literal::Nat(n) => n,
            _ => panic!("Literal.get_nat: not a Nat"),
        }
    }

    pub fn get_string(&self) -> &str {
        match self {
            Literal::Str(s) => s,
            _ => panic!("Literal.get_string: not a Str"),
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Nat(n) => write!(f, "{}", n),
            Literal::Str(s) => write!(f, "\"{}\"", s),
        }
    }
}
