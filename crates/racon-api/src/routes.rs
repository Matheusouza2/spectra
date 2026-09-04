use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_engine::{
    InvariantKind, InvariantQuery, PropertyDto, PropertySource, PropertyStatus, SessionRepository,
};
use racon_runtime::RouterContext;

use crate::events::{event_name, serialize, HEARTBEAT_INTERVAL_MS};
use crate::{
    AnalysisLifecycleResponse, AnalysisStatusDto, ApiError, HealthResponse, InvariantsQuery,
    InvariantsResponse, LoadRequest, PropertiesQuery, PropertiesResponse, PropertyResponseDto,
    StateSpaceMetadata,
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
        .route(
            "/v1/sessions/{session_id}/state-space",
            post(start_state_space),
        )
        .route("/v1/analyses/{analysis_id}/status", get(analysis_status))
        .route("/v1/analyses/{analysis_id}/events", get(analysis_events))
        .route("/v1/analyses/{analysis_id}/cancel", post(cancel_analysis))
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
        .is_some_and(|scope| scope != "structural" && scope != "behavioral" && scope != "all")
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_SCOPE",
            "scope must be structural, behavioral or all",
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
    let mut values: Vec<_> = if scope == "behavioral" {
        Vec::new()
    } else {
        session
            .properties()
            .into_iter()
            .map(PropertyResponseDto::available)
            .collect()
    };
    if scope == "structural" {
        values.retain(|property| property.property.source == PropertySource::Structural);
    } else {
        values.push(PropertyResponseDto::state_space_required(PropertyDto {
            code: "REV".to_owned(),
            name: "Reversible".to_owned(),
            status: PropertyStatus::NotComputed,
            value: None,
            source: PropertySource::StateSpace,
            witness: None,
        }));
    }
    Ok(Json(PropertiesResponse {
        scope,
        properties: values,
    }))
}

async fn start_state_space(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
) -> Result<(StatusCode, Json<AnalysisLifecycleResponse>), ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let analysis_id = state.sessions.create_state_space_analysis(session);

    let monitor_sessions = Arc::clone(&state.sessions);
    let monitor_analysis_id = analysis_id.clone();
    tokio::spawn(async move {
        let worker_sessions = Arc::clone(&monitor_sessions);
        let worker_analysis_id = monitor_analysis_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            worker_sessions.run_state_space_analysis(&worker_analysis_id)
        })
        .await;

        match result {
            Ok(true) => monitor_sessions.finish_state_space_analysis(&monitor_analysis_id),
            Ok(false) => monitor_sessions.fail_analysis(
                &monitor_analysis_id,
                "Analysis disappeared before execution".to_owned(),
            ),
            Err(error) => monitor_sessions.fail_analysis(
                &monitor_analysis_id,
                format!("State-space worker failed: {error}"),
            ),
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(AnalysisLifecycleResponse {
            analysis_id,
            status: "RUNNING".to_owned(),
        }),
    ))
}

async fn analysis_status(
    State(state): State<ApiState>,
    Path(analysis_id): Path<String>,
) -> Result<Json<AnalysisStatusDto>, ApiError> {
    let snapshot = state
        .sessions
        .analysis_status(&analysis_id)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "ANALYSIS_NOT_FOUND",
                format!("Analysis {analysis_id} was not found"),
            )
        })?;
    let metadata = snapshot.metadata().map(|metadata| StateSpaceMetadata {
        kind: metadata.kind().to_owned(),
        algorithm: metadata.algorithm().to_owned(),
        exact: metadata.exact(),
        reduced: metadata.reduced(),
        reductions: metadata.reductions().to_vec(),
        nodes: metadata.nodes(),
        edges: metadata.edges(),
        omega_nodes: metadata.omega_nodes(),
        elapsed_ms: metadata.elapsed_ms(),
        peak_memory: metadata.peak_memory(),
        completed: metadata.completed(),
    });

    Ok(Json(AnalysisStatusDto {
        analysis_id: snapshot.analysis_id().to_owned(),
        status: snapshot.status().to_owned(),
        phase: snapshot.phase().to_owned(),
        states_discovered: snapshot.states_discovered(),
        states_processed: snapshot.states_processed(),
        edges_discovered: snapshot.edges_discovered(),
        frontier_size: snapshot.frontier_size(),
        omega_nodes: snapshot.omega_nodes(),
        elapsed_ms: snapshot.elapsed_ms(),
        metadata,
        error: snapshot.error().map(str::to_owned),
    }))
}

async fn analysis_events(
    State(state): State<ApiState>,
    Path(analysis_id): Path<String>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError>
{
    let (initial, mut receiver) =
        state
            .sessions
            .subscribe_analysis(&analysis_id)
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    "ANALYSIS_NOT_FOUND",
                    format!("Analysis {analysis_id} was not found"),
                )
            })?;
    let sessions = Arc::clone(&state.sessions);
    let stream = async_stream::stream! {
        let initial_name = event_name(&initial);
        yield Ok(Event::default()
            .event(initial_name)
            .data(serde_json::to_string(&serialize(&initial, initial_name))
                .expect("analysis SSE event should serialize")));
        if initial.status() != "RUNNING" && initial.status() != "CANCEL_REQUESTED" {
            return;
        }

        let mut heartbeat = tokio::time::interval(
            std::time::Duration::from_millis(HEARTBEAT_INTERVAL_MS),
        );
        heartbeat.tick().await;
        loop {
            tokio::select! {
                received = receiver.recv() => match received {
                    Ok(snapshot) => {
                        let name = event_name(&snapshot);
                        yield Ok(Event::default()
                            .event(name)
                            .data(serde_json::to_string(&serialize(&snapshot, name))
                                .expect("analysis SSE event should serialize")));
                        if snapshot.status() != "RUNNING" && snapshot.status() != "CANCEL_REQUESTED" {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Some(snapshot) = sessions.analysis_status(&analysis_id) {
                            let name = event_name(&snapshot);
                            yield Ok(Event::default()
                                .event(name)
                                .data(serde_json::to_string(&serialize(&snapshot, name))
                                    .expect("analysis SSE event should serialize")));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
                _ = heartbeat.tick() => {
                    if let Some(snapshot) = sessions.analysis_status(&analysis_id) {
                        if snapshot.status() != "RUNNING" && snapshot.status() != "CANCEL_REQUESTED" {
                            let name = event_name(&snapshot);
                            yield Ok(Event::default()
                                .event(name)
                                .data(serde_json::to_string(&serialize(&snapshot, name))
                                    .expect("analysis SSE event should serialize")));
                            return;
                        }
                        yield Ok(Event::default()
                            .event("heartbeat")
                            .data(serde_json::to_string(&serialize(&snapshot, "heartbeat"))
                                .expect("analysis SSE event should serialize")));
                    } else {
                        return;
                    }
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn cancel_analysis(
    State(state): State<ApiState>,
    Path(analysis_id): Path<String>,
) -> Result<Json<AnalysisLifecycleResponse>, ApiError> {
    let status = state
        .sessions
        .cancel_analysis(&analysis_id)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "ANALYSIS_NOT_FOUND",
                format!("Analysis {analysis_id} was not found"),
            )
        })?;
    Ok(Json(AnalysisLifecycleResponse {
        analysis_id,
        status: status.to_owned(),
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
