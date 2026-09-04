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
    pub properties: Vec<PropertyDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvariantsResponse {
    pub invariants: Vec<InvariantDto>,
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
