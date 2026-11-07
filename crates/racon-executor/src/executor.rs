use crate::metrics::MetricsCollector;
use crate::progress::ProgressTracker;
use racon_core::execution::{AnalysisConfig, AnalysisContext, CancellationToken, MemoryBudget};
use std::sync::Arc;

/// Central coordinator for parallel analysis execution.
///
/// Manages a Rayon thread pool, cancellation, memory budgeting, progress
/// tracking, and metrics collection.
pub struct AnalysisExecutor {
    thread_pool: rayon::ThreadPool,
    cancellation: CancellationToken,
    memory_budget: Arc<MemoryBudget>,
    progress: ProgressTracker,
    metrics: MetricsCollector,
}

impl AnalysisExecutor {
    /// Create a new executor from configuration.
    ///
    /// Returns an error if the Rayon thread pool cannot be built.
    pub fn new(config: AnalysisConfig) -> Result<Self, rayon::ThreadPoolBuildError> {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.thread_count)
            .thread_name(|i| format!("racon-worker-{}", i))
            .build()?;

        let memory_budget = Arc::new(MemoryBudget::new(config.max_memory_mb * 1024 * 1024));

        Ok(Self {
            thread_pool,
            cancellation: CancellationToken::new(),
            memory_budget,
            progress: ProgressTracker::new(),
            metrics: MetricsCollector::new(),
        })
    }

    /// Return an `AnalysisContext` wired to this executor's cancellation,
    /// memory budget, and a default config.
    pub fn context(&self, config: AnalysisConfig) -> AnalysisContext {
        AnalysisContext {
            config,
            cancellation: self.cancellation.clone(),
            memory_budget: self.memory_budget.clone(),
        }
    }

    /// Cancel all running analyses.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Reference to the underlying Rayon thread pool.
    pub fn thread_pool(&self) -> &rayon::ThreadPool {
        &self.thread_pool
    }

    /// Reference to the metrics collector.
    pub fn metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// Reference to the progress tracker.
    pub fn progress(&self) -> &ProgressTracker {
        &self.progress
    }
}
