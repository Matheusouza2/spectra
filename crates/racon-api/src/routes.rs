use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_engine::{InvariantKind, InvariantQuery, SessionRepository};
use racon_runtime::RouterContext;

use crate::{
    ApiError, HealthResponse, InvariantsQuery, InvariantsResponse, LoadRequest, PropertiesQuery,
    PropertiesResponse,
};

#[derive(Clone)]
pub struct ApiState {
    pub sessions: Arc<SessionRepository>,
    pub token: Option<String>,
}

impl ApiState {
    pub fn new(sessions: Arc<SessionRepository>, context: RouterContext) -> Self {
        Self {
            sessions,
            token: context.token,
        }
    }
}

pub fn router(state: ApiState) -> Router {
    let protected = Router::new()
        .route("/v1/nets/load", post(load))
        .route("/v1/sessions/{session_id}/properties", get(properties))
        .route("/v1/sessions/{session_id}/invariants", get(invariants))
        .layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new()
        .route("/v1/health", get(health))
        .merge(protected)
        .with_state(state)
}

async fn authorize(
    State(state): State<ApiState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(expected) = state.token else {
        return next.run(request).await;
    };
    let valid = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|value| value == expected);
    if valid {
        next.run(request).await
    } else {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Bearer token is required",
        )
        .into_response()
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        protocol_version: "1.0",
        engine_version: env!("CARGO_PKG_VERSION"),
    })
}

async fn load(
    State(state): State<ApiState>,
    Json(request): Json<LoadRequest>,
) -> Result<(StatusCode, Json<racon_engine::LoadSessionResponse>), ApiError> {
    let session = racon_engine::AnalysisSession::load(&request.path)
        .map_err(|error| map_load_error(&request.path, error))?;
    let response = session.load_response();
    state.sessions.insert(session);
    Ok((StatusCode::CREATED, Json(response)))
}

async fn properties(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
    Query(query): Query<PropertiesQuery>,
) -> Result<Json<PropertiesResponse>, ApiError> {
    if query
        .scope
        .as_deref()
        .is_some_and(|scope| scope != "structural" && scope != "all")
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_SCOPE",
            "scope must be structural or all",
        ));
    }
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let scope = query.scope.unwrap_or_else(|| "all".to_owned());
    let mut values = session.properties();
    if scope == "structural" {
        values.retain(|property| property.source == racon_engine::PropertySource::Structural);
    }
    Ok(Json(PropertiesResponse {
        scope,
        properties: values,
    }))
}

async fn invariants(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
    Query(query): Query<InvariantsQuery>,
) -> Result<Json<InvariantsResponse>, ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let kind = match query.kind.as_deref().unwrap_or("ALL") {
        "PLACE" => InvariantKind::Place,
        "TRANSITION" => InvariantKind::Transition,
        "ALL" => InvariantKind::All,
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_INVARIANT_KIND",
                "kind must be PLACE, TRANSITION or ALL",
            ))
        }
    };
    let invariant_query = InvariantQuery {
        kind,
        minimal: query.minimal,
        sub: query.sub.unwrap_or(false),
        sur: query.sur.unwrap_or(false),
        positive: query.positive.unwrap_or(false),
        nonnegative: query.nonnegative.unwrap_or(false),
    };
    let context = AnalysisContext::new(AnalysisConfig::default());
    let values = session
        .query_invariants(&invariant_query, &context)
        .map_err(|error| {
            ApiError::new(
                if error.code() == "NOT_IMPLEMENTED" {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                error.code(),
                error.to_string(),
            )
        })?;
    Ok(Json(InvariantsResponse { invariants: values }))
}

fn map_load_error(path: &str, error: racon_core::RaconError) -> ApiError {
    match error {
        racon_core::RaconError::Io(_) => ApiError::new(
            StatusCode::NOT_FOUND,
            "FILE_NOT_FOUND",
            format!("File not found: {path}"),
        ),
        racon_core::RaconError::ParseError { .. } => ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_MERCURY_SCRIPT",
            error.to_string(),
        ),
        racon_core::RaconError::InvalidInput(_)
        | racon_core::RaconError::StructuralViolation(_) => {
            ApiError::new(StatusCode::BAD_REQUEST, "INVALID_NET", error.to_string())
        }
        _ => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            error.to_string(),
        ),
    }
}
