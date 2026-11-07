use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use racon_core::{PetriNet, RaconError};
use racon_parsers::{MercuryParser, MercuryParserConfig, MercuryParserImpl};
use racon_structural::{StructuralAnalyzer, StructuralProperties};
use serde::{Deserialize, Serialize};

use crate::inference::PropertyInferenceEngine;
use crate::invariants::{query_invariants, InvariantCache};
use crate::properties::{
    apply_conservativeness, properties_from_structural, structural_derived_properties, PropertyDto,
    PropertySource,
};
use crate::dead_transition::{decide_transition_dead, DeadTransitionDto, DeadTransitionError};
use crate::place_boundedness::{decide_place_boundedness, PlaceBoundednessDto, PlaceBoundednessError};
use crate::state_space::StateSpaceHandle;
use crate::{InvariantDto, InvariantQuery, InvariantQueryError};

#[path = "analysis.rs"]
mod analysis;

use analysis::AnalysisJobRegistry;
pub use analysis::{AnalysisRequest, AnalysisSnapshot, ExplorationChoice};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceDto {
    #[serde(rename = "type")]
    pub source_type: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetSummaryDto {
    pub places: usize,
    pub transitions: usize,
    /// Ordinary (token-moving) arcs only; inhibitor arcs are counted apart.
    pub arcs: usize,
    #[serde(default)]
    pub inhibitor_arcs: usize,
    #[serde(default)]
    pub immediate_transitions: usize,
    /// `true` when behavior depends on inhibitor arcs or immediate precedence.
    #[serde(default)]
    pub extended: bool,
    #[serde(default)]
    pub semantics_version: String,
    #[serde(default)]
    pub priority_policy: String,
}

/// Per-phase server-side timing breakdown (spec §44).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionTimingsDto {
    pub parse_ms: u64,
    pub structural_ms: u64,
    pub invariant_ms: u64,
    pub state_space_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoadSessionResponse {
    pub session_id: String,
    pub source: SourceDto,
    pub net: NetSummaryDto,
    pub properties: Vec<PropertyDto>,
    pub timings: SessionTimingsDto,
}

#[derive(Debug, Default)]
struct Timings {
    parse_ms: u64,
    structural_ms: u64,
    invariant_ms: u64,
    state_space_ms: u64,
}

#[derive(Debug)]
pub struct AnalysisSession {
    session_id: String,
    source: SourceDto,
    net: Arc<PetriNet>,
    structural_properties: StructuralProperties,
    invariant_cache: InvariantCache,
    state_space: RwLock<Option<Arc<StateSpaceHandle>>>,
    /// Early-exit boundedness verdict (Starke Satz 4.3); `None` if inconclusive.
    boundedness: RwLock<Option<bool>>,
    timings: RwLock<Timings>,
}

impl AnalysisSession {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RaconError> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(RaconError::InvalidInput(format!(
                "MercuryScript path is not a file: {}",
                path.display()
            )));
        }

        let parse_started = Instant::now();
        let parser = MercuryParserImpl::new(MercuryParserConfig::default());
        let net = Arc::new(parser.parse_file(path)?);
        let parse_ms = parse_started.elapsed().as_millis() as u64;

        let structural_started = Instant::now();
        let structural_properties = StructuralAnalyzer::analyze(&net);
        let structural_ms = structural_started.elapsed().as_millis() as u64;

        Ok(Self {
            session_id: new_session_id(),
            source: SourceDto {
                source_type: "mercury_script".to_owned(),
                path: path.to_string_lossy().into_owned(),
            },
            net,
            structural_properties,
            invariant_cache: InvariantCache::default(),
            state_space: RwLock::new(None),
            boundedness: RwLock::new(None),
            timings: RwLock::new(Timings {
                parse_ms,
                structural_ms,
                ..Timings::default()
            }),
        })
    }

    /// Stores the early-exit boundedness verdict computed by an analysis job.
    pub(crate) fn record_boundedness(&self, value: Option<bool>) {
        *self
            .boundedness
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
    }

    /// Boundedness of one place (by name or index) via the stubborn-set
    /// Karp–Miller tree. A net already known to be bounded short-circuits it.
    pub fn place_boundedness(
        &self,
        place: &str,
        context: &racon_core::execution::AnalysisContext,
    ) -> Result<PlaceBoundednessDto, PlaceBoundednessError> {
        let net_known_bounded = *self
            .boundedness
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            == Some(true);
        decide_place_boundedness(&self.net, place, net_known_bounded, context)
    }

    /// Whether a transition (name or index) is dead, via stubborn sets with
    /// attractor `{t}` (Schmidt 1999, Thm 6); a live answer carries a shortest
    /// enabling sequence.
    pub fn transition_dead(
        &self,
        transition: &str,
        context: &racon_core::execution::AnalysisContext,
    ) -> Result<DeadTransitionDto, DeadTransitionError> {
        decide_transition_dead(&self.net, transition, context)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn source(&self) -> &SourceDto {
        &self.source
    }

    pub fn net(&self) -> Arc<PetriNet> {
        Arc::clone(&self.net)
    }

    pub fn structural_properties(&self) -> &StructuralProperties {
        &self.structural_properties
    }

    /// All properties known without starting heavy exploration.
    pub fn properties(&self) -> Vec<PropertyDto> {
        let mut properties = properties_from_structural(&self.structural_properties);
        properties.extend(structural_derived_properties(
            &self.net,
            &self.structural_properties,
        ));
        let invariants = self.invariant_cache.cached();
        apply_conservativeness(&mut properties, invariants.as_deref());
        let state_space = self.cached_state_space();
        properties.extend(PropertyInferenceEngine::infer(
            &self.net,
            &self.structural_properties,
            invariants.as_deref(),
            state_space.as_deref(),
        ));
        let boundedness = *self
            .boundedness
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = boundedness {
            if let Some(bounded) = properties
                .iter_mut()
                .find(|p| p.code == "B" && p.status == crate::PropertyStatus::NotComputed)
            {
                *bounded = PropertyDto::boolean("B", "Bounded", value, PropertySource::StateSpace)
                    .with_rule("BOUNDEDNESS_EARLY_EXIT");
            }
        }
        if self.net.is_extended() {
            tag_analysis_scope(&mut properties);
        }
        properties
    }

    /// Scope-filtered properties as required by `GET .../properties` (§30).
    pub fn properties_for_scope(&self, scope: &str) -> Vec<PropertyDto> {
        let all = self.properties();
        match scope {
            "structural" => all
                .into_iter()
                .filter(|property| property.source == PropertySource::Structural)
                .collect(),
            "behavioral" => all
                .into_iter()
                .filter(|property| property.source != PropertySource::Structural)
                .collect(),
            _ => all,
        }
    }

    pub fn invariants_cached(&self) -> bool {
        self.invariant_cache.cached().is_some()
    }

    pub fn invariant_cache(&self) -> &InvariantCache {
        &self.invariant_cache
    }

    pub fn query_invariants(
        &self,
        query: &InvariantQuery,
        context: &racon_core::execution::AnalysisContext,
    ) -> Result<Vec<InvariantDto>, InvariantQueryError> {
        let started = Instant::now();
        let result = query_invariants(&self.net, &self.invariant_cache, query, context);
        self.record_invariant_time(started.elapsed().as_millis() as u64);
        result
    }

    pub fn cached_state_space(&self) -> Option<Arc<StateSpaceHandle>> {
        self.state_space
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn store_state_space(&self, state_space: Arc<StateSpaceHandle>) {
        *self
            .state_space
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(state_space);
    }

    pub fn record_state_space_time(&self, elapsed_ms: u64) {
        self.timings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .state_space_ms = elapsed_ms;
    }

    fn record_invariant_time(&self, elapsed_ms: u64) {
        self.timings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .invariant_ms = elapsed_ms;
    }

    pub fn timings(&self) -> SessionTimingsDto {
        let timings = self
            .timings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        SessionTimingsDto {
            parse_ms: timings.parse_ms,
            structural_ms: timings.structural_ms,
            invariant_ms: timings.invariant_ms,
            state_space_ms: timings.state_space_ms,
        }
    }

    pub fn load_response(&self) -> LoadSessionResponse {
        LoadSessionResponse {
            session_id: self.session_id.clone(),
            source: self.source.clone(),
            net: NetSummaryDto {
                places: self.net.place_count(),
                transitions: self.net.transition_count(),
                arcs: self.net.arcs().len(),
                inhibitor_arcs: self.net.inhibitor_arcs().len(),
                immediate_transitions: self.net.immediate_transitions().len(),
                extended: self.net.is_extended(),
                semantics_version: racon_core::SEMANTICS_VERSION.to_owned(),
                priority_policy: racon_core::PRIORITY_POLICY.to_owned(),
            },
            properties: self.properties(),
            timings: self.timings(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SessionRepository {
    sessions: RwLock<HashMap<String, Arc<AnalysisSession>>>,
    analyses: AnalysisJobRegistry,
}

impl SessionRepository {
    pub fn insert(&self, session: AnalysisSession) -> Arc<AnalysisSession> {
        let session = Arc::new(session);
        self.sessions
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(session.session_id.clone(), Arc::clone(&session));
        session
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<AnalysisSession>> {
        self.sessions
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_id)
            .cloned()
    }

    pub fn create_state_space_analysis(
        &self,
        session: Arc<AnalysisSession>,
        request: AnalysisRequest,
    ) -> String {
        self.analyses.create(session, request)
    }

    pub fn run_state_space_analysis(&self, analysis_id: &str) -> bool {
        self.analyses.run(analysis_id)
    }

    pub fn analysis_status(&self, analysis_id: &str) -> Option<AnalysisSnapshot> {
        self.analyses.snapshot(analysis_id)
    }

    pub fn subscribe_analysis(
        &self,
        analysis_id: &str,
    ) -> Option<(
        AnalysisSnapshot,
        tokio::sync::broadcast::Receiver<AnalysisSnapshot>,
    )> {
        self.analyses.subscribe(analysis_id)
    }

    pub fn finish_state_space_analysis(&self, analysis_id: &str) {
        self.analyses.finish_after_worker(analysis_id);
    }

    pub fn cancel_analysis(&self, analysis_id: &str) -> Option<&'static str> {
        self.analyses.cancel(analysis_id)
    }

    pub fn fail_analysis(&self, analysis_id: &str, error: impl Into<String>) {
        self.analyses.fail(analysis_id, error.into());
    }
}

/// Marks the scope of every property of an extended net (spec §6.2):
/// topological/algebraic results concern the ordinary-arc projection (or the
/// incidence matrix, which inhibitors never alter), whereas state-space
/// results concern the extended behavior (inhibitors + immediate precedence).
fn tag_analysis_scope(properties: &mut [PropertyDto]) {
    for property in properties {
        property.analysis_scope = Some(
            match property.source {
                PropertySource::Structural | PropertySource::Algebraic => "structural_projection",
                _ => "extended_behavior",
            }
            .to_owned(),
        );
    }
}

fn new_session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{timestamp:032x}-{:08x}-{sequence:016x}",
        std::process::id()
    )
}
