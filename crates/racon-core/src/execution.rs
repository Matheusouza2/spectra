use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for analysis engines.
///
/// Controls memory limits, parallelism, timeouts, and incremental expansion.
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Maximum heap memory budget in MB (default: 4096).
    pub max_memory_mb: usize,
    /// Number of worker threads (default: num_cpus).
    pub thread_count: usize,
    /// Maximum wall-clock time before automatic cancellation (default: 30s).
    pub cancellation_timeout: Duration,
    /// How often to emit progress updates (default: 100ms).
    pub progress_interval: Duration,
    /// Number of graph levels to expand per request (default: 5).
    pub graph_expansion_levels: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 4096,
            thread_count: num_cpus::get(),
            cancellation_timeout: Duration::from_secs(30),
            progress_interval: Duration::from_millis(100),
            graph_expansion_levels: 5,
        }
    }
}

/// A lightweight, cloneable cancellation token.
///
/// Passed to analysis engines via `AnalysisContext`. Any engine can check
/// `is_cancelled()` and abort its computation.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal cancellation. All clones of this token will return `true`.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks heap memory consumption with a configurable limit.
///
/// Uses relaxed atomics — precise enough for budgeting, not accounting.
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    limit_bytes: usize,
    current_bytes: Arc<AtomicUsize>,
}

impl MemoryBudget {
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            limit_bytes,
            current_bytes: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Try to reserve `bytes`. Returns `MemoryError::LimitExceeded` if the
    /// budget would be exceeded.
    pub fn allocate(&self, bytes: usize) -> Result<(), MemoryError> {
        let mut current = self.current_bytes.load(Ordering::Acquire);
        loop {
            let Some(next) = current.checked_add(bytes) else {
                return Err(MemoryError::LimitExceeded {
                    required: bytes,
                    limit: self.limit_bytes,
                });
            };
            if next > self.limit_bytes {
                return Err(MemoryError::LimitExceeded {
                    required: bytes,
                    limit: self.limit_bytes,
                });
            }
            match self.current_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }

    /// Release previously-allocated bytes.
    pub fn deallocate(&self, bytes: usize) {
        self.current_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// Current usage in megabytes (rounded down).
    pub fn current_mb(&self) -> usize {
        self.current_bytes.load(Ordering::Relaxed) / (1024 * 1024)
    }

    /// Estimated bytes already reserved by consumers of this budget.
    pub fn current_bytes(&self) -> usize {
        self.current_bytes.load(Ordering::Acquire)
    }

    /// Budget limit in megabytes.
    pub fn limit_mb(&self) -> usize {
        self.limit_bytes / (1024 * 1024)
    }

    /// Maximum bytes consumers may reserve.
    pub fn limit_bytes(&self) -> usize {
        self.limit_bytes
    }
}

/// Errors from the memory budget subsystem.
#[derive(Debug, Clone)]
pub enum MemoryError {
    /// Allocation would exceed the configured limit.
    LimitExceeded { required: usize, limit: usize },
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LimitExceeded { required, limit } => {
                write!(
                    f,
                    "Memory limit exceeded: required {} bytes, limit {} bytes",
                    required, limit
                )
            }
        }
    }
}

impl std::error::Error for MemoryError {}

/// Periodic progress snapshot emitted during long-running analyses.
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub phase: String,
    pub processed_nodes: usize,
    pub generated_nodes: usize,
    pub memory_mb: usize,
    pub elapsed_ms: u64,
    pub active_threads: usize,
    pub progress: Option<f64>, // 0.0–1.0 if known
}

/// Runtime context shared with every analysis engine.
///
/// Contains configuration, cancellation, and memory tracking.
#[derive(Clone)]
pub struct AnalysisContext {
    pub config: AnalysisConfig,
    pub cancellation: CancellationToken,
    pub memory_budget: Arc<MemoryBudget>,
}

impl AnalysisContext {
    /// Create a new context from a configuration. Initializes a fresh
    /// cancellation token and memory budget.
    pub fn new(config: AnalysisConfig) -> Self {
        Self {
            memory_budget: Arc::new(MemoryBudget::new(config.max_memory_mb * 1024 * 1024)),
            cancellation: CancellationToken::new(),
            config,
        }
    }

    /// Convenience: check cancellation and return `Err(Cancelled)` if set.
    pub fn check_cancellation(&self) -> Result<(), Cancelled> {
        if self.cancellation.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Returned by `check_cancellation` when the operation has been cancelled.
#[derive(Debug, Clone)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation cancelled by user")
    }
}

impl std::error::Error for Cancelled {}

/// Generic trait for all analysis engines.
///
/// Every crate that performs analysis (invariants, reachability, CTL, etc.)
/// implements this trait.
pub trait AnalysisEngine {
    type Input;
    type Output;
    type Error: std::error::Error;

    fn analyze(
        &self,
        input: Self::Input,
        context: &AnalysisContext,
    ) -> Result<Self::Output, Self::Error>;
}
