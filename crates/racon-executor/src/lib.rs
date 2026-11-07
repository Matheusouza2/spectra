//! RACoN Executor — parallel execution infrastructure.
//!
//! Provides `AnalysisExecutor` (Rayon thread pool manager),
//! `ProgressTracker`, and `MetricsCollector` for long-running analyses.

pub mod executor;
pub mod metrics;
pub mod progress;

pub use executor::AnalysisExecutor;
pub use metrics::MetricsCollector;
pub use progress::ProgressTracker;
