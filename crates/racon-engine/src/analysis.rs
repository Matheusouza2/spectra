use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use racon_core::execution::{AnalysisConfig, AnalysisContext, CancellationToken};
use racon_coverability::{
    CoverabilityEngine, CoverabilityEngineImpl, CoverabilityError, CoverabilityGraph,
    CoverabilityStrategy,
};
use racon_invariants::{verify_structural_boundedness_certificate, InvariantAnalysisResult};
use racon_reachability::{
    ReachabilityEngine, ReachabilityEngineImpl, ReachabilityError, ReachabilityGraph,
};
use serde::Serialize;

use super::AnalysisSession;

static NEXT_ANALYSIS_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundednessDecision {
    ProvedBounded,
    Unknown,
}

impl BoundednessDecision {
    fn before_exploration(session: &AnalysisSession, invariants: &InvariantAnalysisResult) -> Self {
        let certified_by_inequality = invariants
            .structural_bounded_by_inequality
            .as_ref()
            .is_some_and(|certificate| {
                verify_structural_boundedness_certificate(
                    session.net.incidence_matrix(),
                    certificate,
                )
            });

        if invariants.is_cpi || invariants.is_conservative || certified_by_inequality {
            Self::ProvedBounded
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobStatus {
    Running,
    CancelRequested,
    Completed,
    Cancelled,
    Failed,
}

impl JobStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::CancelRequested => "CANCEL_REQUESTED",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::Failed => "FAILED",
        }
    }

    const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

enum CompletedStateSpace {
    Reachability(ReachabilityGraph),
    Coverability(CoverabilityGraph),
}

impl CompletedStateSpace {
    fn counts(&self) -> AnalysisCounts {
        match self {
            Self::Reachability(graph) => AnalysisCounts {
                states_discovered: graph.nodes.len(),
                states_processed: graph.nodes.iter().filter(|node| node.is_expanded).count(),
                edges_discovered: graph.edges.len(),
                frontier_size: graph.nodes.iter().filter(|node| !node.is_expanded).count(),
                omega_nodes: 0,
            },
            Self::Coverability(graph) => AnalysisCounts {
                states_discovered: graph.nodes.len(),
                states_processed: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && node.is_expanded)
                    .count(),
                edges_discovered: graph.edges.len(),
                frontier_size: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && !node.is_expanded)
                    .count(),
                omega_nodes: graph.stats.omega_nodes,
            },
        }
    }

    fn into_snapshot(self, elapsed_ms: u64) -> (AnalysisCounts, EngineStateSpaceMetadata) {
        let counts = self.counts();
        let metadata = match self {
            Self::Reachability(graph) => EngineStateSpaceMetadata {
                kind: "REACHABILITY",
                algorithm: "BREADTH_FIRST_SEARCH",
                exact: graph.complete,
                reduced: false,
                reductions: Vec::new(),
                nodes: counts.states_discovered,
                edges: counts.edges_discovered,
                omega_nodes: counts.omega_nodes,
                elapsed_ms,
                peak_memory: None,
                completed: graph.complete,
            },
            Self::Coverability(_) => EngineStateSpaceMetadata {
                kind: "COVERABILITY",
                algorithm: "KARP_MILLER",
                exact: false,
                reduced: false,
                reductions: Vec::new(),
                nodes: counts.states_discovered,
                edges: counts.edges_discovered,
                omega_nodes: counts.omega_nodes,
                elapsed_ms,
                peak_memory: None,
                completed: true,
            },
        };
        (counts, metadata)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AnalysisCounts {
    states_discovered: usize,
    states_processed: usize,
    edges_discovered: usize,
    frontier_size: usize,
    omega_nodes: usize,
}

struct JobData {
    status: JobStatus,
    phase: &'static str,
    elapsed_ms: u64,
    worker_outcome: Option<Result<CompletedStateSpace, AnalysisFailure>>,
    counts: AnalysisCounts,
    metadata: Option<EngineStateSpaceMetadata>,
    error: Option<String>,
}

struct JobState {
    analysis_id: String,
    session: Weak<AnalysisSession>,
    cancellation: CancellationToken,
    started_at: Instant,
    data: Mutex<JobData>,
}

impl JobState {
    fn new(analysis_id: String, session: Arc<AnalysisSession>) -> Self {
        Self {
            analysis_id,
            session: Arc::downgrade(&session),
            cancellation: CancellationToken::new(),
            started_at: Instant::now(),
            data: Mutex::new(JobData {
                status: JobStatus::Running,
                phase: "STATE_SPACE_EXPLORATION",
                elapsed_ms: 0,
                worker_outcome: None,
                counts: AnalysisCounts::default(),
                metadata: None,
                error: None,
            }),
        }
    }

    fn snapshot(&self) -> AnalysisSnapshot {
        let data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed_ms = if data.status.is_terminal() {
            data.elapsed_ms
        } else {
            self.elapsed_ms()
        };

        AnalysisSnapshot {
            analysis_id: self.analysis_id.clone(),
            status: data.status,
            phase: data.phase,
            counts: data.counts,
            elapsed_ms,
            metadata: data.metadata.clone(),
            error: data.error.clone(),
        }
    }

    fn cancel(&self) -> JobStatus {
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if data.status == JobStatus::Running {
            data.status = JobStatus::CancelRequested;
            data.phase = "CANCEL_REQUESTED";
            data.elapsed_ms = self.elapsed_ms();
            self.cancellation.cancel();
        }
        data.status
    }

    fn record_worker_outcome(&self, outcome: Result<CompletedStateSpace, AnalysisFailure>) {
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if data.status.is_terminal() {
            return;
        }
        data.worker_outcome = Some(outcome);
    }

    fn finish_after_worker(&self) {
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if data.status.is_terminal() {
            return;
        }

        data.elapsed_ms = self.elapsed_ms();
        if data.status == JobStatus::CancelRequested {
            data.status = JobStatus::Cancelled;
            data.phase = "CANCELLED";
            data.worker_outcome = None;
            return;
        }

        let outcome = data.worker_outcome.take().unwrap_or_else(|| {
            Err(AnalysisFailure::Failed(
                "State-space worker returned without an outcome".to_owned(),
            ))
        });
        match outcome {
            Ok(result) => {
                let (counts, metadata) = result.into_snapshot(data.elapsed_ms);
                data.status = JobStatus::Completed;
                data.phase = "COMPLETED";
                data.counts = counts;
                data.metadata = Some(metadata);
            }
            Err(AnalysisFailure::Cancelled) => {
                data.status = JobStatus::Cancelled;
                data.phase = "CANCELLED";
            }
            Err(AnalysisFailure::Failed(error)) => {
                data.status = JobStatus::Failed;
                data.phase = "FAILED";
                data.error = Some(error);
            }
        }
    }

    fn run(&self) {
        if self.cancellation.is_cancelled() {
            self.record_worker_outcome(Err(AnalysisFailure::Cancelled));
            return;
        }

        let Some(session) = self.session.upgrade() else {
            self.record_worker_outcome(Err(AnalysisFailure::Failed(
                "Analysis session is no longer available".to_owned(),
            )));
            return;
        };
        let mut context = AnalysisContext::new(AnalysisConfig::default());
        context.cancellation = self.cancellation.clone();
        let outcome = self.compute_state_space(&session, &context);
        self.record_worker_outcome(outcome);
    }

    fn compute_state_space(
        &self,
        session: &AnalysisSession,
        context: &AnalysisContext,
    ) -> Result<CompletedStateSpace, AnalysisFailure> {
        let invariants = session
            .invariant_cache
            .get_or_compute(&session.net, context)
            .map_err(|error| {
                if context.cancellation.is_cancelled() {
                    AnalysisFailure::Cancelled
                } else {
                    AnalysisFailure::Failed(error.to_string())
                }
            })?;
        context
            .check_cancellation()
            .map_err(|_| AnalysisFailure::Cancelled)?;

        match BoundednessDecision::before_exploration(session, &invariants) {
            BoundednessDecision::ProvedBounded => ReachabilityEngineImpl::default()
                .build_complete(&session.net, context)
                .map(CompletedStateSpace::Reachability)
                .map_err(AnalysisFailure::from),
            BoundednessDecision::Unknown => CoverabilityEngineImpl::default()
                .build_complete(&session.net, CoverabilityStrategy::KarpMiller, context)
                .map(CompletedStateSpace::Coverability)
                .map_err(AnalysisFailure::from),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

enum AnalysisFailure {
    Cancelled,
    Failed(String),
}

impl From<ReachabilityError> for AnalysisFailure {
    fn from(error: ReachabilityError) -> Self {
        match error {
            ReachabilityError::Cancelled => Self::Cancelled,
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<CoverabilityError> for AnalysisFailure {
    fn from(error: CoverabilityError) -> Self {
        match error {
            CoverabilityError::Cancelled => Self::Cancelled,
            other => Self::Failed(other.to_string()),
        }
    }
}

#[derive(Default)]
pub(crate) struct AnalysisJobRegistry {
    jobs: RwLock<HashMap<String, Arc<JobState>>>,
}

impl fmt::Debug for AnalysisJobRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let job_count = self
            .jobs
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len();
        formatter
            .debug_struct("AnalysisJobRegistry")
            .field("job_count", &job_count)
            .finish()
    }
}

impl AnalysisJobRegistry {
    pub(crate) fn create(&self, session: Arc<AnalysisSession>) -> String {
        let analysis_id = new_analysis_id();
        let job = Arc::new(JobState::new(analysis_id.clone(), session));
        self.jobs
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(analysis_id.clone(), job);
        analysis_id
    }

    pub(crate) fn run(&self, analysis_id: &str) -> bool {
        let Some(job) = self.get(analysis_id) else {
            return false;
        };
        job.run();
        true
    }

    pub(crate) fn snapshot(&self, analysis_id: &str) -> Option<AnalysisSnapshot> {
        self.get(analysis_id).map(|job| job.snapshot())
    }

    pub(crate) fn finish_after_worker(&self, analysis_id: &str) {
        if let Some(job) = self.get(analysis_id) {
            job.finish_after_worker();
        }
    }

    pub(crate) fn cancel(&self, analysis_id: &str) -> Option<&'static str> {
        self.get(analysis_id).map(|job| job.cancel().as_str())
    }

    pub(crate) fn fail(&self, analysis_id: &str, error: String) {
        if let Some(job) = self.get(analysis_id) {
            job.record_worker_outcome(Err(AnalysisFailure::Failed(error)));
            job.finish_after_worker();
        }
    }

    fn get(&self, analysis_id: &str) -> Option<Arc<JobState>> {
        self.jobs
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(analysis_id)
            .cloned()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStateSpaceMetadata {
    kind: &'static str,
    algorithm: &'static str,
    exact: bool,
    reduced: bool,
    reductions: Vec<String>,
    nodes: usize,
    edges: usize,
    omega_nodes: usize,
    elapsed_ms: u64,
    peak_memory: Option<usize>,
    completed: bool,
}

impl EngineStateSpaceMetadata {
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn algorithm(&self) -> &'static str {
        self.algorithm
    }

    pub fn exact(&self) -> bool {
        self.exact
    }

    pub fn reduced(&self) -> bool {
        self.reduced
    }

    pub fn reductions(&self) -> &[String] {
        &self.reductions
    }

    pub fn nodes(&self) -> usize {
        self.nodes
    }

    pub fn edges(&self) -> usize {
        self.edges
    }

    pub fn omega_nodes(&self) -> usize {
        self.omega_nodes
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    pub fn peak_memory(&self) -> Option<usize> {
        self.peak_memory
    }

    pub fn completed(&self) -> bool {
        self.completed
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisSnapshot {
    analysis_id: String,
    status: JobStatus,
    phase: &'static str,
    counts: AnalysisCounts,
    elapsed_ms: u64,
    metadata: Option<EngineStateSpaceMetadata>,
    error: Option<String>,
}

impl AnalysisSnapshot {
    pub fn analysis_id(&self) -> &str {
        &self.analysis_id
    }

    pub fn status(&self) -> &'static str {
        self.status.as_str()
    }

    pub fn phase(&self) -> &'static str {
        self.phase
    }

    pub fn states_discovered(&self) -> usize {
        self.counts.states_discovered
    }

    pub fn states_processed(&self) -> usize {
        self.counts.states_processed
    }

    pub fn edges_discovered(&self) -> usize {
        self.counts.edges_discovered
    }

    pub fn frontier_size(&self) -> usize {
        self.counts.frontier_size
    }

    pub fn omega_nodes(&self) -> usize {
        self.counts.omega_nodes
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    pub fn metadata(&self) -> Option<&EngineStateSpaceMetadata> {
        self.metadata.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

fn new_analysis_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_ANALYSIS_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "analysis-{timestamp:032x}-{:08x}-{sequence:016x}",
        std::process::id()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Arc, Mutex, Weak};
    use std::thread;
    use std::time::{Duration, Instant};

    use racon_core::execution::CancellationToken;

    use super::{AnalysisCounts, AnalysisFailure, JobData, JobState, JobStatus};
    use crate::AnalysisSession;

    fn running_job() -> Arc<JobState> {
        Arc::new(JobState {
            analysis_id: "analysis-cancellation-ordering".to_owned(),
            session: Weak::<AnalysisSession>::new(),
            cancellation: CancellationToken::new(),
            started_at: Instant::now(),
            data: Mutex::new(JobData {
                status: JobStatus::Running,
                phase: "STATE_SPACE_EXPLORATION",
                elapsed_ms: 0,
                worker_outcome: None,
                counts: AnalysisCounts::default(),
                metadata: None,
                error: None,
            }),
        })
    }

    #[test]
    fn cancel_publishes_requested_state_before_signalling_the_worker() {
        let job = running_job();
        let data_guard = job
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cancellation = job.cancellation.clone();
        let cancel_job = Arc::clone(&job);
        let (started_tx, started_rx) = mpsc::sync_channel(0);
        let cancel_thread = thread::spawn(move || {
            started_tx
                .send(())
                .expect("test should observe the cancellation thread");
            cancel_job.cancel()
        });

        started_rx.recv().expect("cancellation thread should start");
        let deadline = Instant::now() + Duration::from_millis(500);
        while !cancellation.is_cancelled() && Instant::now() < deadline {
            thread::yield_now();
        }
        let signalled_while_state_was_locked = cancellation.is_cancelled();
        assert_eq!(data_guard.status, JobStatus::Running);
        drop(data_guard);

        let response_status = cancel_thread
            .join()
            .expect("cancellation thread should finish");
        assert!(
            !signalled_while_state_was_locked,
            "worker cancellation must not be visible before CANCEL_REQUESTED can be published"
        );
        assert_eq!(response_status, JobStatus::CancelRequested);
        assert!(cancellation.is_cancelled());
        assert_eq!(job.snapshot().status, JobStatus::CancelRequested);

        job.record_worker_outcome(Err(AnalysisFailure::Cancelled));
        assert_eq!(job.snapshot().status, JobStatus::CancelRequested);
        job.finish_after_worker();
        assert_eq!(job.snapshot().status, JobStatus::Cancelled);
    }
}
