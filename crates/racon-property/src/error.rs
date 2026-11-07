use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum PropertyError {
    #[error("ILP Solver timed out")]
    SolverTimeout,

    #[error("Invalid predicate: {0}")]
    InvalidPredicate(String),

    #[error("CTL evaluation failed: {0}")]
    CtlEvaluationFailed(String),

    #[error("Property evaluation cancelled by user")]
    Cancelled,
    #[error("property evaluation requires an exact reachability state space")]
    NonExactStateSpace,
}

impl From<racon_ctl::CTLError> for PropertyError {
    fn from(e: racon_ctl::CTLError) -> Self {
        match e {
            racon_ctl::CTLError::Cancelled => PropertyError::Cancelled,
            other => PropertyError::CtlEvaluationFailed(other.to_string()),
        }
    }
}

impl From<racon_core::execution::Cancelled> for PropertyError {
    fn from(_: racon_core::execution::Cancelled) -> Self {
        PropertyError::Cancelled
    }
}
