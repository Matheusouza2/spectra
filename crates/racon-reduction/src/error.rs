//! Structured errors for the reduction layer.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReductionError {
    /// The requested reduction is not implemented by this engine.
    NotSupported { name: &'static str },
    /// The reduction exists but its preconditions are not met on this net.
    NotApplicable { name: &'static str, reason: &'static str },
    /// A combination of reductions is not compatible.
    Incompatible { reason: String },
    /// The reduced exploration exceeded the configured node budget.
    LimitExceeded { limit: usize },
    /// The enclosing analysis was cancelled.
    Cancelled,
}

impl ReductionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotSupported { .. } => "REDUCTION_NOT_SUPPORTED",
            Self::NotApplicable { .. } => "REDUCTION_NOT_APPLICABLE",
            Self::Incompatible { .. } => "REDUCTION_NOT_APPLICABLE",
            Self::LimitExceeded { .. } => "RESOURCE_LIMIT_EXCEEDED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

impl fmt::Display for ReductionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSupported { name } => write!(formatter, "reduction '{name}' is not supported"),
            Self::NotApplicable { name, reason } => {
                write!(formatter, "reduction '{name}' is not applicable: {reason}")
            }
            Self::Incompatible { reason } => {
                write!(formatter, "incompatible reduction combination: {reason}")
            }
            Self::LimitExceeded { limit } => {
                write!(formatter, "reduced exploration exceeded {limit} nodes")
            }
            Self::Cancelled => write!(formatter, "reduction cancelled"),
        }
    }
}

impl std::error::Error for ReductionError {}
