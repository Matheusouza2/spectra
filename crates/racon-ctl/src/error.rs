use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CTLError {
    #[error("CTL evaluation cancelled by user")]
    Cancelled,

    #[error("Fixed-point iteration did not converge within {limit} iterations")]
    FixedPointConvergenceFailed { limit: usize },

    #[error("Invalid formula: {0}")]
    InvalidFormula(String),

    #[error("State space error: {0}")]
    StateSpaceError(String),

    #[error("CTL requires an exact reachability state space")]
    NonExactStateSpace,

    #[error("Witness extraction failed: no path found")]
    WitnessExtractionFailed,
}

impl From<racon_core::execution::Cancelled> for CTLError {
    fn from(_: racon_core::execution::Cancelled) -> Self {
        CTLError::Cancelled
    }
}
