use racon_engine::{InvariantDto, LoadSessionResponse, PropertyDto};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub protocol_version: &'static str,
    pub engine_version: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct LoadRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertiesResponse {
    pub scope: String,
    pub properties: Vec<PropertyResponseDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertyResponseDto {
    #[serde(flatten)]
    pub property: PropertyDto,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<&'static str>,
}

impl PropertyResponseDto {
    pub fn available(property: PropertyDto) -> Self {
        Self {
            property,
            requires: Vec::new(),
        }
    }

    pub fn state_space_required(property: PropertyDto) -> Self {
        Self {
            property,
            requires: vec!["STATE_SPACE"],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InvariantsResponse {
    pub invariants: Vec<InvariantDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisLifecycleResponse {
    pub analysis_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisStatusDto {
    pub analysis_id: String,
    pub status: String,
    pub phase: String,
    pub states_discovered: usize,
    pub states_processed: usize,
    pub edges_discovered: usize,
    pub frontier_size: usize,
    pub omega_nodes: usize,
    pub elapsed_ms: u64,
    pub metadata: Option<StateSpaceMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateSpaceMetadata {
    pub semantics_version: String,
    pub priority_policy: String,
    pub analysis_scope: String,
    pub kind: String,
    pub algorithm: String,
    pub exact: bool,
    pub reduced: bool,
    pub reductions: Vec<String>,
    pub variant: String,
    pub nodes: usize,
    pub edges: usize,
    pub omega_nodes: usize,
    pub elapsed_ms: u64,
    pub peak_frontier: usize,
    pub peak_memory: Option<usize>,
    pub threads: usize,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadResponse(pub LoadSessionResponse);

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StartStateSpaceRequest {
    /// Catalogue id of the exploration algorithm (`reachability_bfs`,
    /// `reachability_dfs`, `coverability_karp_miller`).
    pub exploration: Option<String>,
    pub reductions: Option<Vec<String>>,
    pub symmetry: Option<SymmetryOptionsDto>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SymmetryOptionsDto {
    pub respect_initial_marking: Option<bool>,
    pub fixed_places: Option<Vec<u32>>,
    pub fixed_transitions: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PropertiesQuery {
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct InvariantsQuery {
    pub kind: Option<String>,
    pub minimal: Option<bool>,
    pub sub: Option<bool>,
    pub sur: Option<bool>,
    pub positive: Option<bool>,
    pub nonnegative: Option<bool>,
}

/// Result of `POST /v1/sessions/{id}/coverability-set` (MinCov, R6).
#[derive(Debug, Clone, Serialize)]
pub struct CoverabilitySetResponse {
    pub algorithm: &'static str,
    pub status: &'static str,
    pub reference_ids: Vec<&'static str>,
    /// `true` when the result comes from an algorithm still marked experimental.
    pub experimental: bool,
    pub kind: &'static str,
    pub bounded: bool,
    pub unbounded_places: Vec<String>,
    /// Maximal ω-markings; each entry is a number or the string `"omega"`.
    pub elements: Vec<Vec<serde_json::Value>>,
    pub tree_vertices: usize,
    pub accelerations: usize,
    pub iterations: usize,
    pub elapsed_ms: u64,
    /// Properties that must not be concluded from this artefact.
    pub does_not_preserve: Vec<&'static str>,
}
