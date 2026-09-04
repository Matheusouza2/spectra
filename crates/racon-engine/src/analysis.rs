use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_coverability::{
    CoverabilityEngine, CoverabilityEngineImpl, CoverabilityError, CoverabilityGraph,
    CoverabilityStrategy,
};
use racon_executor::AnalysisExecutor;
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
    ProvedUnbounded,
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

    fn from_coverability(graph: &CoverabilityGraph) -> Self {
        match graph.bounded_status() {
            Some(true) => Self::ProvedBounded,
            Some(false) => Self::ProvedUnbounded,
            None => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

impl JobStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::Failed => "FAILED",
        }
    }

    const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
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

    fn metadata(&self, elapsed_ms: u64) -> EngineStateSpaceMetadata {
        let counts = self.counts();
        match self {
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
            Self::Coverability(graph) => {
                let decision = BoundednessDecision::from_coverability(graph);
                EngineStateSpaceMetadata {
                    kind: if decision == BoundednessDecision::ProvedBounded {
                        "REACHABILITY"
                    } else {
                        "COVERABILITY"
                    },
                    algorithm: "KARP_MILLER",
                    exact: decision == BoundednessDecision::ProvedBounded,
                    reduced: false,
                    reductions: Vec::new(),
                    nodes: counts.states_discovered,
                    edges: counts.edges_discovered,
                    omega_nodes: counts.omega_nodes,
                    elapsed_ms,
                    peak_memory: None,
                    completed: true,
                }
            }
        }
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
    result: Option<CompletedStateSpace>,
    error: Option<String>,
}

struct JobState {
    analysis_id: String,
    session: Arc<AnalysisSession>,
    executor: AnalysisExecutor,
    context: AnalysisContext,
    data: Mutex<JobData>,
}

impl JobState {
    fn new(analysis_id: String, session: Arc<AnalysisSession>) -> Result<Self, String> {
        let config = AnalysisConfig::default();
        let executor = AnalysisExecutor::new(config.clone()).map_err(|error| error.to_string())?;
        let context = executor.context(config);
        Ok(Self {
            analysis_id,
            session,
            executor,
            context,
            data: Mutex::new(JobData {
                status: JobStatus::Running,
                phase: "STATE_SPACE_EXPLORATION",
                elapsed_ms: 0,
                result: None,
                error: None,
            }),
        })
    }

    fn snapshot(&self) -> AnalysisSnapshot {
        let data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed_ms = if data.status.is_terminal() {
            data.elapsed_ms
        } else {
            self.executor.metrics().elapsed_ms()
        };
        let counts = data
            .result
            .as_ref()
            .map(CompletedStateSpace::counts)
            .unwrap_or_default();
        let metadata = data
            .result
            .as_ref()
            .map(|result| result.metadata(elapsed_ms));

        AnalysisSnapshot {
            analysis_id: self.analysis_id.clone(),
            status: data.status,
            phase: data.phase,
            counts,
            elapsed_ms,
            metadata,
            error: data.error.clone(),
        }
    }

    fn cancel(&self) -> JobStatus {
        self.executor.cancel();
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if data.status == JobStatus::Running {
            data.status = JobStatus::Cancelled;
            data.phase = "CANCELLED";
            data.elapsed_ms = self.executor.metrics().elapsed_ms();
        }
        data.status
    }

    fn finish(&self, outcome: Result<CompletedStateSpace, AnalysisFailure>) {
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if data.status != JobStatus::Running {
            return;
        }

        data.elapsed_ms = self.executor.metrics().elapsed_ms();
        match outcome {
            Ok(result) => {
                data.status = JobStatus::Completed;
                data.phase = "COMPLETED";
                data.result = Some(result);
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
        if self.context.cancellation.is_cancelled() {
            self.finish(Err(AnalysisFailure::Cancelled));
            return;
        }

        let outcome = self.compute_state_space();
        self.finish(outcome);
    }

    fn compute_state_space(&self) -> Result<CompletedStateSpace, AnalysisFailure> {
        let invariants = self
            .session
            .invariant_cache
            .get_or_compute(&self.session.net, &self.context)
            .map_err(|error| {
                if self.context.cancellation.is_cancelled() {
                    AnalysisFailure::Cancelled
                } else {
                    AnalysisFailure::Failed(error.to_string())
                }
            })?;
        self.context
            .check_cancellation()
            .map_err(|_| AnalysisFailure::Cancelled)?;

        match BoundednessDecision::before_exploration(&self.session, &invariants) {
            BoundednessDecision::ProvedBounded => ReachabilityEngineImpl::default()
                .build_complete(&self.session.net, &self.context)
                .map(CompletedStateSpace::Reachability)
                .map_err(AnalysisFailure::from),
            BoundednessDecision::ProvedUnbounded | BoundednessDecision::Unknown => {
                CoverabilityEngineImpl::default()
                    .build_complete(
                        &self.session.net,
                        CoverabilityStrategy::KarpMiller,
                        &self.context,
                    )
                    .map(CompletedStateSpace::Coverability)
                    .map_err(AnalysisFailure::from)
            }
        }
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
    pub(crate) fn create(&self, session: Arc<AnalysisSession>) -> Result<String, String> {
        let analysis_id = new_analysis_id();
        let job = Arc::new(JobState::new(analysis_id.clone(), session)?);
        self.jobs
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(analysis_id.clone(), job);
        Ok(analysis_id)
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

    pub(crate) fn cancel(&self, analysis_id: &str) -> Option<&'static str> {
        self.get(analysis_id).map(|job| job.cancel().as_str())
    }

    pub(crate) fn fail(&self, analysis_id: &str, error: String) {
        if let Some(job) = self.get(analysis_id) {
            job.finish(Err(AnalysisFailure::Failed(error)));
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
