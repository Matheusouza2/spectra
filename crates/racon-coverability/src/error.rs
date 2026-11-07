use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CoverabilityError {
    #[error("Node limit exceeded: {current} nodes (limit: {limit})")]
    NodeLimitExceeded { limit: usize, current: usize },

    #[error("Edge limit exceeded: {current} edges (limit: {limit})")]
    EdgeLimitExceeded { limit: usize, current: usize },

    #[error("Estimated graph memory limit exceeded (reserved: {current} bytes, required total: {required} bytes, limit: {limit} bytes)")]
    MemoryLimitExceeded { current: usize, required: usize, limit: usize },

    #[error("Analysis cancelled by user")]
    Cancelled,

    /// The algorithm's hypotheses (monotonicity of the firing relation) are not
    /// established for this net, so no formal conclusion may be produced.
    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// A token count or incidence exceeded the exact integer range of the
    /// implementation. Reported instead of wrapping, saturating or guessing.
    #[error("Arithmetic range exceeded: {0}")]
    ArithmeticOverflow(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

impl From<racon_core::error::RaconError> for CoverabilityError {
    fn from(error: racon_core::error::RaconError) -> Self {
        // Only a genuine cancellation is reported as one; any other core error
        // keeps its own message instead of masquerading as a user cancel.
        match error {
            racon_core::error::RaconError::Cancelled => CoverabilityError::Cancelled,
            other => CoverabilityError::VerificationFailed(other.to_string()),
        }
    }
}

/// Message used whenever a monotonicity-based algorithm is refused.
pub const MONOTONICITY_REQUIRED: &str = "Karp-Miller/Finkel coverability and dominance-based boundedness require a monotone firing relation; nets with inhibitor arcs or immediate-transition precedence are not monotone (racon-extended-pt-v1). Use exact reachability with resource limits instead.";
