use std::sync::Arc;

use axum::{
    body::Bytes,
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
    AnalysisRequest, ExplorationChoice, InvariantKind, InvariantQuery, PropertySource, PropertyStatus,
    SessionRepository,
};
use racon_reduction::catalog::{
    algorithm as catalog_algorithm, validate as validate_catalog, ConfigRequest, NetClass, Objective,
    ALGORITHMS, CATALOG_VERSION, REFERENCES,
};
use racon_reduction::{plan, ReductionKind, SymmetryOptions};
use racon_runtime::RouterContext;

use crate::events::{event_name, serialize, HEARTBEAT_INTERVAL_MS};
use crate::{
    AnalysisLifecycleResponse, AnalysisStatusDto, ApiError, HealthResponse, InvariantsQuery,
    CoverabilitySetResponse, InvariantsResponse, LoadRequest, PropertiesQuery, PropertiesResponse, PropertyResponseDto,
    StartStateSpaceRequest, StateSpaceMetadata,
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
        .route(
            "/v1/sessions/{session_id}/coverability-set",
            post(coverability_set),
        )
        .route(
            "/v1/sessions/{session_id}/places/{place}/boundedness",
            get(place_boundedness),
        )
        .route(
            "/v1/sessions/{session_id}/transitions/{transition}/dead",
            get(transition_dead),
        )
        .route("/v1/analysis/algorithms", get(algorithm_catalog))
        .route("/v1/analysis/config/validate", post(validate_config))
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
    let values = session
        .properties_for_scope(&scope)
        .into_iter()
        .map(|property| {
            if property.status == PropertyStatus::NotComputed
                && property.source == PropertySource::StateSpace
            {
                PropertyResponseDto::state_space_required(property)
            } else {
                PropertyResponseDto::available(property)
            }
        })
        .collect();
    Ok(Json(PropertiesResponse {
        scope,
        properties: values,
    }))
}

async fn start_state_space(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<AnalysisLifecycleResponse>), ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let request = parse_state_space_request(&body, net_class_of(&session.net()))?;
    if session.net().is_extended() && !request.reductions.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNSUPPORTED",
            format!(
                "Reductions are not defined for nets with inhibitor arcs or immediate transitions ({})",
                racon_core::SEMANTICS_VERSION
            ),
        ));
    }
    let analysis_id = state.sessions.create_state_space_analysis(session, request);

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
        semantics_version: metadata.semantics_version().to_owned(),
        priority_policy: metadata.priority_policy().to_owned(),
        analysis_scope: metadata.analysis_scope().to_owned(),
        kind: metadata.kind().to_owned(),
        algorithm: metadata.algorithm().to_owned(),
        exact: metadata.exact(),
        reduced: metadata.reduced(),
        reductions: metadata.reductions().to_vec(),
        variant: metadata.variant().to_owned(),
        nodes: metadata.nodes(),
        edges: metadata.edges(),
        omega_nodes: metadata.omega_nodes(),
        elapsed_ms: metadata.elapsed_ms(),
        peak_frontier: metadata.peak_frontier(),
        peak_memory: metadata.peak_memory(),
        threads: metadata.threads(),
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

        // §25: emit an immediate heartbeat so clients can confirm the stream is
        // live without waiting for the first periodic interval.
        yield Ok(Event::default()
            .event("heartbeat")
            .data(serde_json::to_string(&serialize(&initial, "heartbeat"))
                .expect("analysis SSE event should serialize")));

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
        "SUB" => InvariantKind::Sub,
        "SUR" => InvariantKind::Sur,
        "ALL" => InvariantKind::All,
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_INVARIANT_KIND",
                "kind must be PLACE, TRANSITION, SUB, SUR or ALL",
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

fn parse_state_space_request(
    body: &[u8],
    net_class: NetClass,
) -> Result<AnalysisRequest, ApiError> {
    if body.is_empty() || body == b"null" {
        return Ok(AnalysisRequest::default());
    }
    let parsed: StartStateSpaceRequest = serde_json::from_slice(body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            format!("Invalid state-space request: {error}"),
        )
    })?;
    let exploration = match parsed.exploration.as_deref() {
        None => None,
        Some(id) => Some(ExplorationChoice::from_catalog_id(id).ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNSUPPORTED_COMBINATION",
                format!("Exploration algorithm '{id}' cannot be run as a state-space job"),
            )
        })?),
    };
    if let Some(id) = parsed.exploration.as_deref() {
        // Validate against the capability matrix; the objective follows the
        // algorithm family (coverability backends vs exact reachability).
        let objective = if id.starts_with("coverability") {
            Objective::Coverability
        } else {
            Objective::Reachability
        };
        let report = validate_catalog(&ConfigRequest {
            objective,
            mode: "manual".to_owned(),
            exploration: Some(id.to_owned()),
            reductions: Vec::new(),
            net_class,
        });
        if !report.is_valid() {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNSUPPORTED_COMBINATION",
                report
                    .issues
                    .iter()
                    .map(|issue| issue.message.clone())
                    .collect::<Vec<_>>()
                    .join(" "),
            ));
        }
    }
    let mut reductions = Vec::new();
    for name in parsed.reductions.unwrap_or_default() {
        let kind = ReductionKind::from_wire(&name).ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "REDUCTION_NOT_SUPPORTED",
                format!("Reduction {name} is not supported"),
            )
        })?;
        reductions.push(kind);
    }
    plan(&reductions).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            error.code(),
            error.to_string(),
        )
    })?;
    let symmetry = parsed
        .symmetry
        .map(|dto| SymmetryOptions {
            respect_initial_marking: dto.respect_initial_marking.unwrap_or(true),
            fixed_places: dto.fixed_places.unwrap_or_default(),
            fixed_transitions: dto.fixed_transitions.unwrap_or_default(),
        })
        .unwrap_or_default();
    Ok(AnalysisRequest {
        reductions,
        symmetry,
        exploration,
    })
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

fn net_class_of(net: &racon_core::PetriNet) -> NetClass {
    if net.is_extended() {
        NetClass::ExtendedPt
    } else {
        NetClass::PtWeighted
    }
}

/// `GET /v1/sessions/{id}/places/{place}/boundedness` — boundedness of one
/// place by the stubborn-set Karp–Miller tree (R9, Thm 2). `place` is a name
/// or an index. Budget overruns answer 422 `RESOURCE_LIMIT_EXCEEDED`
/// (UNKNOWN), never a verdict.
async fn place_boundedness(
    State(state): State<ApiState>,
    Path((session_id, place)): Path<(String, String)>,
) -> Result<Json<racon_engine::PlaceBoundednessDto>, ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let outcome = tokio::task::spawn_blocking(move || {
        let context = AnalysisContext::new(AnalysisConfig::default());
        session.place_boundedness(&place, &context)
    })
    .await
    .map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            format!("boundedness worker failed: {error}"),
        )
    })?;
    outcome.map(Json).map_err(|error| {
        let status = match error {
            racon_engine::PlaceBoundednessError::PlaceNotFound(_) => StatusCode::NOT_FOUND,
            racon_engine::PlaceBoundednessError::Cancelled => StatusCode::CONFLICT,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        ApiError::new(status, error.code(), error.to_string())
    })
}

/// `GET /v1/sessions/{id}/transitions/{transition}/dead` — dead-transition
/// test through stubborn sets (R9, Thm 6). `transition` is a name or an index.
/// Budget overruns answer 422 `RESOURCE_LIMIT_EXCEEDED` (UNKNOWN).
async fn transition_dead(
    State(state): State<ApiState>,
    Path((session_id, transition)): Path<(String, String)>,
) -> Result<Json<racon_engine::DeadTransitionDto>, ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let outcome = tokio::task::spawn_blocking(move || {
        let context = AnalysisContext::new(AnalysisConfig::default());
        session.transition_dead(&transition, &context)
    })
    .await
    .map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            format!("dead-transition worker failed: {error}"),
        )
    })?;
    outcome.map(Json).map_err(|error| {
        let status = match error {
            racon_engine::DeadTransitionError::TransitionNotFound(_) => StatusCode::NOT_FOUND,
            racon_engine::DeadTransitionError::Cancelled => StatusCode::CONFLICT,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        ApiError::new(status, error.code(), error.to_string())
    })
}

/// `GET /v1/analysis/algorithms` — versioned catalogue with references.
async fn algorithm_catalog() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "catalog_version": CATALOG_VERSION,
        "algorithms": ALGORITHMS,
        "references": REFERENCES,
    }))
}

/// `POST /v1/analysis/config/validate` — checks a configuration without
/// executing anything.
async fn validate_config(
    Json(request): Json<ConfigRequest>,
) -> Json<racon_reduction::catalog::ValidationReport> {
    Json(validate_catalog(&request))
}

/// `POST /v1/sessions/{id}/coverability-set` — MinCov (R6, Algorithm 1).
async fn coverability_set(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
) -> Result<Json<CoverabilitySetResponse>, ApiError> {
    let session = state.sessions.get(&session_id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "SESSION_NOT_FOUND",
            format!("Session {session_id} was not found"),
        )
    })?;
    let net = session.net();
    let report = validate_catalog(&ConfigRequest {
        objective: Objective::Coverability,
        mode: "manual".to_owned(),
        exploration: Some("coverability_mincov".to_owned()),
        reductions: Vec::new(),
        net_class: net_class_of(&net),
    });
    if !report.is_valid() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNSUPPORTED_COMBINATION",
            report
                .issues
                .iter()
                .map(|issue| issue.message.clone())
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    let entry = catalog_algorithm("coverability_mincov").expect("catalogued");
    let started = std::time::Instant::now();
    let worker_net = Arc::clone(&net);
    let outcome = tokio::task::spawn_blocking(move || {
        let context = AnalysisContext::new(AnalysisConfig::default());
        racon_coverability::MinCov::default().compute(&worker_net, &context)
    })
    .await
    .map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            format!("MinCov worker failed: {error}"),
        )
    })?;
    let set = outcome.map_err(|error| match error {
        racon_coverability::CoverabilityError::Unsupported(message) => {
            ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "UNSUPPORTED", message)
        }
        // A truncated run is inconclusive, never a negative verdict.
        other => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "RESOURCE_LIMIT_EXCEEDED",
            format!("UNKNOWN: {other}"),
        ),
    })?;
    let names: Vec<String> = net.places().iter().map(|p| p.name.clone()).collect();
    Ok(Json(CoverabilitySetResponse {
        algorithm: entry.id,
        status: "completed",
        reference_ids: entry.reference_ids.to_vec(),
        experimental: true,
        kind: "MINIMAL_COVERABILITY_SET",
        bounded: set.is_bounded(),
        unbounded_places: set
            .unbounded_places()
            .into_iter()
            .map(|index| names[index].clone())
            .collect(),
        elements: set
            .elements()
            .iter()
            .map(|marking| {
                marking
                    .iter()
                    .map(|value| match value {
                        racon_coverability::OmegaValue::Finite(n) => serde_json::json!(n),
                        racon_coverability::OmegaValue::Omega => serde_json::json!("omega"),
                    })
                    .collect()
            })
            .collect(),
        tree_vertices: set.tree_vertices,
        accelerations: set.accelerations,
        iterations: set.iterations,
        elapsed_ms: started.elapsed().as_millis() as u64,
        does_not_preserve: entry.does_not_preserve.to_vec(),
    }))
}
