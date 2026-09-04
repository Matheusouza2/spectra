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
    pub kind: String,
    pub algorithm: String,
    pub exact: bool,
    pub reduced: bool,
    pub reductions: Vec<String>,
    pub nodes: usize,
    pub edges: usize,
    pub omega_nodes: usize,
    pub elapsed_ms: u64,
    pub peak_memory: Option<usize>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadResponse(pub LoadSessionResponse);

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
