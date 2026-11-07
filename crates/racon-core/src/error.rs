use thiserror::Error;

/// Top-level error type for all RACoN operations.
///
/// Never panic in production code — always return `Result<T, RaconError>`.
#[derive(Debug, Error)]
pub enum RaconError {
    // --- System Errors ---
    #[error("I/O operation failed: {0}")]
    Io(#[from] std::io::Error),

    // --- Validation Errors ---
    #[error("Parsing failed ({format}): {reason}")]
    ParseError {
        format: String,
        reason: String,
        line: Option<usize>,
        col: Option<usize>,
    },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Structural violation: {0}")]
    StructuralViolation(String),

    // --- Analysis Limits ---
    #[error("Analysis cancelled by user")]
    Cancelled,

    #[error("Memory budget exceeded: required {required_mb}MB, limit {limit_mb}MB")]
    MemoryLimitExceeded { required_mb: usize, limit_mb: usize },

    // --- Mathematical / Domain Results ---
    #[error("Net is unbounded; Reachability Graph cannot be constructed")]
    UnboundedNet,

    #[error("Marking is unreachable: {certificate}")]
    UnreachableProven { certificate: String },

    // --- Persistence ---
    #[error("Unsupported project version: {version}")]
    UnsupportedVersion { version: String },

    #[error("Data corruption: {component}")]
    DataCorruption { component: String },
}
