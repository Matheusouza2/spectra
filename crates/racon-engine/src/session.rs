use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use racon_core::{PetriNet, RaconError};
use racon_parsers::{MercuryParser, MercuryParserConfig, MercuryParserImpl};
use racon_structural::{StructuralAnalyzer, StructuralProperties};
use serde::{Deserialize, Serialize};

use crate::invariants::{query_invariants, InvariantCache};
use crate::properties::{properties_from_structural, properties_with_invariants};
use crate::{InvariantDto, InvariantQuery, InvariantQueryError, PropertyDto};

#[path = "analysis.rs"]
mod analysis;

use analysis::AnalysisJobRegistry;
pub use analysis::AnalysisSnapshot;

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
    pub arcs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoadSessionResponse {
    pub session_id: String,
    pub source: SourceDto,
    pub net: NetSummaryDto,
    pub properties: Vec<PropertyDto>,
}

#[derive(Debug)]
pub struct AnalysisSession {
    session_id: String,
    source: SourceDto,
    net: Arc<PetriNet>,
    structural_properties: StructuralProperties,
    properties: Vec<PropertyDto>,
    invariant_cache: InvariantCache,
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

        let parser = MercuryParserImpl::new(MercuryParserConfig::default());
        let net = Arc::new(parser.parse_file(path)?);
        let structural_properties = StructuralAnalyzer::analyze(&net);
        let properties = properties_from_structural(&structural_properties);

        Ok(Self {
            session_id: new_session_id(),
            source: SourceDto {
                source_type: "mercury_script".to_owned(),
                path: path.to_string_lossy().into_owned(),
            },
            net,
            structural_properties,
            properties,
            invariant_cache: InvariantCache::default(),
        })
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

    pub fn properties(&self) -> Vec<PropertyDto> {
        let invariants = self.invariant_cache.cached();
        properties_with_invariants(&self.properties, invariants.as_deref())
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
        query_invariants(&self.net, &self.invariant_cache, query, context)
    }

    pub fn load_response(&self) -> LoadSessionResponse {
        LoadSessionResponse {
            session_id: self.session_id.clone(),
            source: self.source.clone(),
            net: NetSummaryDto {
                places: self.net.place_count(),
                transitions: self.net.transition_count(),
                arcs: self.net.arcs().len(),
            },
            properties: self.properties(),
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

    pub fn create_state_space_analysis(&self, session: Arc<AnalysisSession>) -> String {
        self.analyses.create(session)
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
