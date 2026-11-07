//! Integration tests for racon-executor.

use racon_core::execution::{
    AnalysisConfig, AnalysisContext, CancellationToken, MemoryBudget, MemoryError,
};
use racon_executor::{AnalysisExecutor, MetricsCollector, ProgressTracker};

// =============================================================================
// AnalysisConfig Tests
// =============================================================================

#[test]
fn test_analysis_config_default() {
    let cfg = AnalysisConfig::default();
    assert_eq!(cfg.max_memory_mb, 4096);
    assert!(cfg.thread_count >= 1, "Should detect at least 1 CPU");
    assert!(cfg.cancellation_timeout.as_secs() >= 30);
    assert_eq!(cfg.graph_expansion_levels, 5);
}

// =============================================================================
// CancellationToken Tests
// =============================================================================

#[test]
fn test_cancellation_token_new_not_cancelled() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn test_cancellation_token_cancel() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn test_cancellation_token_clone_shares_state() {
    let token = CancellationToken::new();
    let clone = token.clone();
    token.cancel();
    assert!(clone.is_cancelled(), "Clone should reflect cancelled state");
}

#[test]
fn test_cancellation_token_default() {
    let token = CancellationToken::default();
    assert!(!token.is_cancelled());
}

// =============================================================================
// MemoryBudget Tests
// =============================================================================

#[test]
fn test_memory_budget_allocate_deallocate() {
    let budget = MemoryBudget::new(1024);
    assert!(budget.allocate(512).is_ok());
    assert_eq!(budget.current_mb(), 0); // < 1 MB
    budget.deallocate(256);
    // After deallocating 256, 256 remain
    budget.deallocate(256);
}

#[test]
fn test_memory_budget_overflow() {
    let budget = MemoryBudget::new(100);
    let err = budget.allocate(200).expect_err("Should exceed limit");
    match err {
        MemoryError::LimitExceeded { required, limit } => {
            assert_eq!(required, 200);
            assert_eq!(limit, 100);
        }
    }
}

#[test]
fn test_memory_budget_exact_limit() {
    let budget = MemoryBudget::new(100);
    assert!(budget.allocate(100).is_ok(), "Should allow exact limit");
    let err = budget.allocate(1).expect_err("Should exceed after full");
    assert!(matches!(err, MemoryError::LimitExceeded { .. }));
}

#[test]
fn test_memory_budget_zero_limit() {
    let budget = MemoryBudget::new(0);
    let err = budget
        .allocate(1)
        .expect_err("Should reject with zero limit");
    assert!(matches!(err, MemoryError::LimitExceeded { .. }));
}

#[test]
fn test_memory_budget_limit_mb() {
    let budget = MemoryBudget::new(2 * 1024 * 1024);
    assert_eq!(budget.limit_mb(), 2);
}

// =============================================================================
// ProgressTracker Tests
// =============================================================================

#[test]
fn test_progress_tracker_build_update() {
    let tracker = ProgressTracker::new();
    let update = tracker.build_update(10, 20, 5, 4, Some(0.5));
    assert_eq!(update.processed_nodes, 10);
    assert_eq!(update.generated_nodes, 20);
    assert_eq!(update.memory_mb, 5);
    assert_eq!(update.active_threads, 4);
    assert_eq!(update.progress, Some(0.5));
}

#[test]
fn test_progress_tracker_set_phase() {
    let mut tracker = ProgressTracker::new();
    tracker.set_phase("computing invariants".into());
    let update = tracker.build_update(0, 0, 0, 0, None);
    assert_eq!(update.phase, "computing invariants");
}

// =============================================================================
// MetricsCollector Tests
// =============================================================================

#[test]
fn test_metrics_collector_increment() {
    let metrics = MetricsCollector::new();
    metrics.increment("nodes_processed");
    metrics.increment("nodes_processed");
    metrics.increment("edges_added");
    // Can't read counter values back (no getter), but at least no crash
}

#[test]
fn test_metrics_collector_elapsed() {
    let metrics = MetricsCollector::new();
    let ms = metrics.elapsed_ms();
    assert!(ms < 1000, "Elapsed should be small just after creation");
}

// =============================================================================
// AnalysisContext Tests
// =============================================================================

#[test]
fn test_analysis_context_creation() {
    let ctx = AnalysisContext::new(AnalysisConfig::default());
    assert!(!ctx.cancellation.is_cancelled());
    assert_eq!(ctx.memory_budget.limit_mb(), 4096);
}

#[test]
fn test_analysis_context_check_cancellation() {
    let ctx = AnalysisContext::new(AnalysisConfig::default());
    assert!(ctx.check_cancellation().is_ok());
    ctx.cancellation.cancel();
    assert!(ctx.check_cancellation().is_err());
}

// =============================================================================
// AnalysisExecutor Tests
// =============================================================================

#[test]
fn test_analysis_executor_creation() {
    let config = AnalysisConfig::default();
    let executor = AnalysisExecutor::new(config);
    assert!(executor.is_ok(), "Executor should create successfully");
}

#[test]
fn test_analysis_executor_context() {
    let config = AnalysisConfig::default();
    let executor = AnalysisExecutor::new(config).expect("Should create executor");
    let ctx = executor.context(AnalysisConfig::default());
    assert!(!ctx.cancellation.is_cancelled());
}

#[test]
fn test_analysis_executor_cancel() {
    let executor =
        AnalysisExecutor::new(AnalysisConfig::default()).expect("Should create executor");
    executor.cancel();
    let ctx = executor.context(AnalysisConfig::default());
    assert!(ctx.cancellation.is_cancelled());
}
