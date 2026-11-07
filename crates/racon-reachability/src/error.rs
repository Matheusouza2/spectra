use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ReachabilityError {
    #[error("Node limit exceeded: {current} nodes (limit: {limit})")]
    NodeLimitExceeded { limit: usize, current: usize },

    #[error("Edge limit exceeded: {current} edges (limit: {limit})")]
    EdgeLimitExceeded { limit: usize, current: usize },

    #[error("Estimated graph memory limit exceeded (reserved: {current} bytes, required total: {required} bytes, limit: {limit} bytes)")]
    MemoryLimitExceeded { current: usize, required: usize, limit: usize },

    #[error("Analysis cancelled by user")]
    Cancelled,

    #[error("Net appears unbounded at place {place}: cannot build finite reachability graph")]
    UnboundedNet { place: u32 },

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

impl From<racon_core::error::RaconError> for ReachabilityError {
    fn from(error: racon_core::error::RaconError) -> Self {
        // Only a genuine cancellation is reported as one; any other core error
        // keeps its own message instead of masquerading as a user cancel.
        match error {
            racon_core::error::RaconError::Cancelled => ReachabilityError::Cancelled,
            other => ReachabilityError::VerificationFailed(other.to_string()),
        }
    }
}
