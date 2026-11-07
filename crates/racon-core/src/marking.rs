use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

/// Represents a token count in a place.
/// `Omega` is used only in coverability graphs to represent unbounded growth.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenValue {
    Finite(u64),
    /// Exact finite value outside the legacy u64 representation.
    Exact(BigUint),
    Omega,
}

/// A marking assigns a token value to each place.
/// Length always equals the number of places in the net.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Marking {
    pub tokens: Vec<TokenValue>,
}

impl Marking {
    /// Create a new marking from token values.
    /// All tokens must be Finite (no Omega in initial markings).
    pub fn new(tokens: Vec<TokenValue>) -> Self {
        Self { tokens }
    }

    /// Convenience: create a marking from finite u64 values.
    pub fn from_finite(finites: &[u64]) -> Self {
        Self {
            tokens: finites.iter().map(|&t| TokenValue::Finite(t)).collect(),
        }
    }

    /// Number of places in this marking.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Whether this marking is empty (zero places).
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Get the token value at a given place index.
    pub fn get(&self, place: usize) -> Option<&TokenValue> {
        self.tokens.get(place)
    }

    /// Check if this marking contains any Omega values.
    pub fn has_omega(&self) -> bool {
        self.tokens.iter().any(|t| matches!(t, TokenValue::Omega))
    }
}

impl TokenValue {
    pub fn exact(value: BigUint) -> Self {
        if value <= BigUint::from(u64::MAX) {
            Self::Finite(value.try_into().expect("value checked against u64::MAX"))
        } else {
            Self::Exact(value)
        }
    }

    pub fn as_biguint(&self) -> Option<BigUint> {
        match self {
            Self::Finite(value) => Some(BigUint::from(*value)),
            Self::Exact(value) => Some(value.clone()),
            Self::Omega => None,
        }
    }

    /// Omega + anything = Omega.
    /// Finite(a) + Finite(b) is exact and may return `Exact` beyond u64.
    pub fn saturating_add(self, other: TokenValue) -> TokenValue {
        match (self, other) {
            (TokenValue::Omega, _) | (_, TokenValue::Omega) => TokenValue::Omega,
            (left, right) => {
                TokenValue::exact(left.as_biguint().unwrap() + right.as_biguint().unwrap())
            }
        }
    }

    /// Omega - anything = Omega.
    /// Finite(a) - Finite(b) = Finite(a - b), saturating at 0.
    pub fn saturating_sub(self, other: TokenValue) -> TokenValue {
        match (self, other) {
            (TokenValue::Omega, _) => TokenValue::Omega,
            (_, TokenValue::Omega) => TokenValue::Finite(0),
            (left, right) => {
                let left = left.as_biguint().unwrap();
                let right = right.as_biguint().unwrap();
                TokenValue::exact(if left >= right {
                    left - right
                } else {
                    BigUint::default()
                })
            }
        }
    }

    /// Omega >= everything. Finite obeys normal comparison.
    pub fn geq(&self, other: &TokenValue) -> bool {
        match (self, other) {
            (TokenValue::Omega, _) => true,
            (_, TokenValue::Omega) => false,
            (left, right) => left.as_biguint().unwrap() >= right.as_biguint().unwrap(),
        }
    }

    /// Strictly greater than.
    pub fn gt(&self, other: &TokenValue) -> bool {
        match (self, other) {
            (TokenValue::Omega, TokenValue::Omega) => false,
            (TokenValue::Omega, _) => true,
            (_, TokenValue::Omega) => false,
            (left, right) => left.as_biguint().unwrap() > right.as_biguint().unwrap(),
        }
    }
}

impl std::fmt::Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenValue::Finite(n) => write!(f, "{}", n),
            TokenValue::Exact(n) => write!(f, "{}", n),
            TokenValue::Omega => write!(f, "ω"),
        }
    }
}

// ---------------------------------------------------------------------------
// std::ops implementations for ergonomic marking arithmetic
// ---------------------------------------------------------------------------

impl std::ops::Add for TokenValue {
    type Output = TokenValue;
    fn add(self, rhs: TokenValue) -> TokenValue {
        self.saturating_add(rhs)
    }
}

impl std::ops::Sub for TokenValue {
    type Output = TokenValue;
    fn sub(self, rhs: TokenValue) -> TokenValue {
        self.saturating_sub(rhs)
    }
}

impl PartialOrd for Marking {
    fn partial_cmp(&self, other: &Marking) -> Option<std::cmp::Ordering> {
        if self.tokens.len() != other.tokens.len() {
            return None;
        }
        let all_geq = self.tokens.iter().zip(&other.tokens).all(|(a, b)| a.geq(b));
        let all_leq = self.tokens.iter().zip(&other.tokens).all(|(a, b)| b.geq(a));
        let equal = self.tokens.iter().zip(&other.tokens).all(|(a, b)| a == b);
        match (all_geq, all_leq, equal) {
            (_, _, true) => Some(std::cmp::Ordering::Equal),
            (true, false, _) => Some(std::cmp::Ordering::Greater),
            (false, true, _) => Some(std::cmp::Ordering::Less),
            _ => None, // incomparable
        }
    }
}
