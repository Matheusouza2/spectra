use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use racon_core::execution::{AnalysisConfig, AnalysisContext, CancellationToken};
use racon_coverability::{decide_boundedness, 
    CoverabilityEngine, CoverabilityEngineImpl, CoverabilityError, CoverabilityStrategy,
};
use racon_invariants::{verify_structural_boundedness_certificate, InvariantAnalysisResult};
use racon_reachability::{
    ReachabilityEngine, ReachabilityEngineImpl, ReachabilityError,
};
use racon_reduction::{
    decide_boundedness_stubborn, BoundSearchConfig, BoundTarget, BoundVerdict,
    ReducedExplorationConfig, ReducedReachabilityGraph, ReductionError, ReductionKind,
    SymmetryOptions,
};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::state_space::StateSpaceHandle;

use super::AnalysisSession;

static NEXT_ANALYSIS_ID: AtomicU64 = AtomicU64::new(1);

/// User-selected state-space construction options (spec §31-38).
///
/// Incompatible reduction combinations are rejected by
/// `racon_reduction::plan` at the API boundary, so the worker only ever sees a
/// validated set.
#[derive(Debug, Clone, Default)]
pub struct AnalysisRequest {
    pub reductions: Vec<ReductionKind>,
    pub symmetry: SymmetryOptions,
    /// Explicit exploration backend (algorithm catalogue). `None` keeps the
    /// legacy automatic choice.
    pub exploration: Option<ExplorationChoice>,
}

/// Exploration algorithms selectable for a state-space job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorationChoice {
    /// `reachability_bfs`
    Bfs,
    /// `reachability_dfs`
    Dfs,
    /// `coverability_karp_miller`
    KarpMiller,
}

impl ExplorationChoice {
    pub fn from_catalog_id(id: &str) -> Option<Self> {
        match id {
            "reachability_bfs" => Some(Self::Bfs),
            "reachability_dfs" => Some(Self::Dfs),
            "coverability_karp_miller" => Some(Self::KarpMiller),
            _ => None,
        }
    }
}

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

#[derive(Debug, Clone, Copy, Default)]
struct AnalysisCounts {
    states_discovered: usize,
    states_processed: usize,
    edges_discovered: usize,
    frontier_size: usize,
    omega_nodes: usize,
}

fn counts_of(handle: &StateSpaceHandle) -> AnalysisCounts {
    let counts = handle.counts();
    AnalysisCounts {
        states_discovered: counts.states_discovered,
        states_processed: counts.states_processed,
        edges_discovered: counts.edges_discovered,
        frontier_size: counts.frontier_size,
        omega_nodes: counts.omega_nodes,
    }
}

fn metadata_of(
    handle: &StateSpaceHandle,
    extended: bool,
    elapsed_ms: u64,
) -> EngineStateSpaceMetadata {
    let counts = handle.counts();
    EngineStateSpaceMetadata {
        semantics_version: racon_core::SEMANTICS_VERSION,
        priority_policy: racon_core::PRIORITY_POLICY,
        analysis_scope: if extended {
            "extended_behavior"
        } else {
            "plain_pt_behavior"
        },
        kind: handle.kind(),
        algorithm: handle.algorithm(),
        exact: handle.is_exact(),
        reduced: handle.reduced(),
        reductions: handle.reductions(),
        variant: handle.variant(),
        nodes: counts.states_discovered,
        edges: counts.edges_discovered,
        omega_nodes: counts.omega_nodes,
        peak_frontier: counts.frontier_size,
        threads: std::thread::available_parallelism()
            .map(|threads| threads.get())
            .unwrap_or(1),
        elapsed_ms,
        peak_memory: None,
        completed: handle.completed(),
    }
}

struct JobData {
    status: JobStatus,
    phase: &'static str,
    elapsed_ms: u64,
    worker_outcome: Option<Result<StateSpaceHandle, AnalysisFailure>>,
    counts: AnalysisCounts,
    metadata: Option<EngineStateSpaceMetadata>,
    error: Option<String>,
}

struct JobState {
    analysis_id: String,
    session: Weak<AnalysisSession>,
    request: AnalysisRequest,
    cancellation: CancellationToken,
    started_at: Instant,
    data: Mutex<JobData>,
    events: broadcast::Sender<AnalysisSnapshot>,
}

impl JobState {
    fn new(
        analysis_id: String,
        session: Arc<AnalysisSession>,
        request: AnalysisRequest,
    ) -> Self {
        let (events, _) = broadcast::channel(32);
        Self {
            analysis_id,
            session: Arc::downgrade(&session),
            request,
            cancellation: CancellationToken::new(),
            started_at: Instant::now(),
            events,
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
            let snapshot = self.snapshot_from_data(&data);
            drop(data);
            let _ = self.events.send(snapshot);
            return JobStatus::CancelRequested;
        }
        data.status
    }

    fn snapshot_from_data(&self, data: &JobData) -> AnalysisSnapshot {
        AnalysisSnapshot {
            analysis_id: self.analysis_id.clone(),
            status: data.status,
            phase: data.phase,
            counts: data.counts,
            elapsed_ms: if data.status.is_terminal() {
                data.elapsed_ms
            } else {
                self.elapsed_ms()
            },
            metadata: data.metadata.clone(),
            error: data.error.clone(),
        }
    }

    fn record_worker_outcome(&self, outcome: Result<StateSpaceHandle, AnalysisFailure>) {
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
            let snapshot = self.snapshot_from_data(&data);
            drop(data);
            let _ = self.events.send(snapshot);
            return;
        }

        let outcome = data.worker_outcome.take().unwrap_or_else(|| {
            Err(AnalysisFailure::Failed(
                "State-space worker returned without an outcome".to_owned(),
            ))
        });
        match outcome {
            Ok(result) => {
                if let Some(session) = self.session.upgrade() {
                    session.record_state_space_time(data.elapsed_ms);
                    session.store_state_space(Arc::new(result.clone()));
                }
                data.status = JobStatus::Completed;
                data.phase = "COMPLETED";
                data.counts = counts_of(&result);
                let extended = self
                    .session
                    .upgrade()
                    .map(|session| session.net.is_extended())
                    .unwrap_or(false);
                data.metadata = Some(metadata_of(&result, extended, data.elapsed_ms));
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
        let snapshot = self.snapshot_from_data(&data);
        drop(data);
        let _ = self.events.send(snapshot);
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
    ) -> Result<StateSpaceHandle, AnalysisFailure> {
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

        if session.net.is_extended() {
            return self.compute_extended_state_space(session, &invariants, context);
        }

        // Explicit algorithm choice from the catalogue: never replaced silently.
        if let Some(choice) = self.request.exploration {
            if self.request.reductions.is_empty() {
                return match choice {
                    ExplorationChoice::Bfs => ReachabilityEngineImpl::default()
                        .build_complete(&session.net, context)
                        .map(StateSpaceHandle::Reachability)
                        .map_err(AnalysisFailure::from),
                    ExplorationChoice::Dfs => ReachabilityEngineImpl::default()
                        .build_complete_dfs(&session.net, context)
                        .map(StateSpaceHandle::DepthFirstReachability)
                        .map_err(AnalysisFailure::from),
                    ExplorationChoice::KarpMiller => CoverabilityEngineImpl::default()
                        .build_complete(&session.net, CoverabilityStrategy::KarpMiller, context)
                        .map(StateSpaceHandle::Coverability)
                        .map_err(AnalysisFailure::from),
                };
            }
        }

        if self
            .request
            .reductions
            .contains(&ReductionKind::FinkelMinimalCoverability)
        {
            return CoverabilityEngineImpl::default()
                .build_complete(
                    &session.net,
                    CoverabilityStrategy::FinkelMinimalCoverability,
                    context,
                )
                .map(StateSpaceHandle::MinimalCoverability)
                .map_err(AnalysisFailure::from);
        }

        // Symmetry/stubborn reductions are applied while the reachability graph
        // is constructed (§33, §34), never by pruning a full exploration.
        if !self.request.reductions.is_empty() {
            return ReducedReachabilityGraph::build(
                &session.net,
                &self.request.reductions,
                &self.request.symmetry,
                ReducedExplorationConfig::default(),
                context,
            )
            .map(StateSpaceHandle::ReducedReachability)
            .map_err(AnalysisFailure::from);
        }

        match BoundednessDecision::before_exploration(session, &invariants) {
            BoundednessDecision::ProvedBounded => ReachabilityEngineImpl::default()
                .build_complete(&session.net, context)
                .map(StateSpaceHandle::Reachability)
                .map_err(AnalysisFailure::from),
            BoundednessDecision::Unknown => {
                // Starke Satz 4.3: decide boundedness first, halting at the
                // first marking that strictly covers an ancestor, and publish
                // the verdict right away so `Bounded` does not wait for the
                // (possibly huge) coverability graph. The graph kind and
                // exactness contract of the job is unchanged.
                // The stubborn-reduced search (Schmidt 1999, Thm 1) decides the
                // same question on a smaller graph; both of its verdicts are
                // sound. When it cannot decide within budget (or the net is
                // extended) the classical early test takes over.
                let stubborn = decide_boundedness_stubborn(
                    &session.net,
                    BoundTarget::Net,
                    BoundSearchConfig {
                        max_nodes: 100_000,
                        use_stubborn: true,
                    },
                    context,
                );
                match stubborn {
                    Ok(report) => session
                        .record_boundedness(Some(matches!(report.verdict, BoundVerdict::Bounded))),
                    Err(ReductionError::Cancelled) => return Err(AnalysisFailure::Cancelled),
                    Err(_) => {
                        if let Ok(verdict) = decide_boundedness(&session.net, 100_000, context) {
                            session.record_boundedness(verdict.as_bool());
                        }
                    }
                }
                context
                    .check_cancellation()
                    .map_err(|_| AnalysisFailure::Cancelled)?;
                CoverabilityEngineImpl::default()
                    .build_complete(&session.net, CoverabilityStrategy::KarpMiller, context)
                    .map(StateSpaceHandle::Coverability)
                    .map_err(AnalysisFailure::from)
            }
        }
    }

    /// Exact exploration for nets with inhibitor arcs or immediate transitions.
    ///
    /// Karp-Miller/Finkel acceleration, dominance pruning, the dominance-based
    /// boundedness test and every state-space reduction assume a monotone
    /// firing relation, which inhibitors and immediate precedence break
    /// (spec §5.3). The only conclusive procedure available is exhaustive
    /// exploration of the extended graph; if resources run out, the outcome is
    /// reported as UNKNOWN, never as a negative proof.
    fn compute_extended_state_space(
        &self,
        session: &AnalysisSession,
        invariants: &InvariantAnalysisResult,
        context: &AnalysisContext,
    ) -> Result<StateSpaceHandle, AnalysisFailure> {
        if !self.request.reductions.is_empty() {
            return Err(AnalysisFailure::Failed(format!(
                "UNSUPPORTED: reductions {:?} are not defined for nets with inhibitor arcs or immediate transitions ({})",
                self.request
                    .reductions
                    .iter()
                    .map(|kind| kind.as_str())
                    .collect::<Vec<_>>(),
                racon_core::SEMANTICS_VERSION
            )));
        }

        // Boundedness certificates from P-invariants / Murata inequalities are
        // algebraic and remain valid: the extended firing relation is a
        // restriction of the ordinary one, so it only reaches fewer markings.
        let certified_bounded =
            BoundednessDecision::before_exploration(session, invariants)
                == BoundednessDecision::ProvedBounded;
        let engine = if certified_bounded {
            ReachabilityEngineImpl::default()
        } else {
            ReachabilityEngineImpl {
                config: racon_reachability::ReachabilityConfig {
                    max_nodes: EXTENDED_UNCERTIFIED_NODE_LIMIT,
                    ..racon_reachability::ReachabilityConfig::default()
                },
            }
        };
        let dfs = self.request.exploration == Some(ExplorationChoice::Dfs);
        let built = if dfs {
            engine
                .build_complete_dfs(&session.net, context)
                .map(StateSpaceHandle::DepthFirstReachability)
        } else {
            engine
                .build_complete(&session.net, context)
                .map(StateSpaceHandle::Reachability)
        };
        built.map_err(|error| match error {
                ReachabilityError::Cancelled => AnalysisFailure::Cancelled,
                limit @ (ReachabilityError::NodeLimitExceeded { .. }
                | ReachabilityError::EdgeLimitExceeded { .. }
                | ReachabilityError::MemoryLimitExceeded { .. }) => {
                    AnalysisFailure::Failed(format!(
                        "UNKNOWN (state_limit_reached): exact exploration of the extended net was truncated; no conclusion about deadlock, boundedness, liveness or reversibility is valid. {limit}"
                    ))
                }
                other => AnalysisFailure::Failed(other.to_string()),
            })
    }

    fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

/// Node budget for extended nets that are not certified bounded algebraically.
const EXTENDED_UNCERTIFIED_NODE_LIMIT: usize = 1_000_000;

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

impl From<ReductionError> for AnalysisFailure {
    fn from(error: ReductionError) -> Self {
        match error {
            ReductionError::Cancelled => Self::Cancelled,
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
    pub(crate) fn create(
        &self,
        session: Arc<AnalysisSession>,
        request: AnalysisRequest,
    ) -> String {
        let analysis_id = new_analysis_id();
        let job = Arc::new(JobState::new(analysis_id.clone(), session, request));
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

    pub(crate) fn subscribe(
        &self,
        analysis_id: &str,
    ) -> Option<(AnalysisSnapshot, broadcast::Receiver<AnalysisSnapshot>)> {
        self.get(analysis_id)
            .map(|job| (job.snapshot(), job.events.subscribe()))
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
    semantics_version: &'static str,
    priority_policy: &'static str,
    analysis_scope: &'static str,
    kind: &'static str,
    algorithm: &'static str,
    exact: bool,
    reduced: bool,
    reductions: Vec<String>,
    variant: &'static str,
    nodes: usize,
    edges: usize,
    omega_nodes: usize,
    peak_frontier: usize,
    threads: usize,
    elapsed_ms: u64,
    peak_memory: Option<usize>,
    completed: bool,
}

impl EngineStateSpaceMetadata {
    /// Identifier of the enabling/firing semantics the graph was built with.
    pub fn semantics_version(&self) -> &'static str {
        self.semantics_version
    }

    /// Precedence policy between transition kinds.
    pub fn priority_policy(&self) -> &'static str {
        self.priority_policy
    }

    /// `extended_behavior` when inhibitors/immediates were evaluated,
    /// `plain_pt_behavior` otherwise.
    pub fn analysis_scope(&self) -> &'static str {
        self.analysis_scope
    }

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

    pub fn variant(&self) -> &'static str {
        self.variant
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

    pub fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }

    pub fn threads(&self) -> usize {
        self.threads
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

    use super::{AnalysisCounts, AnalysisFailure, AnalysisRequest, JobData, JobState, JobStatus};
    use crate::AnalysisSession;

    fn running_job() -> Arc<JobState> {
        Arc::new(JobState {
            analysis_id: "analysis-cancellation-ordering".to_owned(),
            session: Weak::<AnalysisSession>::new(),
            request: AnalysisRequest::default(),
            cancellation: CancellationToken::new(),
            started_at: Instant::now(),
            events: tokio::sync::broadcast::channel(32).0,
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
