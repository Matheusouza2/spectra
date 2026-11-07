use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use racon_api::{router, ApiState};
use racon_engine::SessionRepository;
use racon_runtime::RouterContext;
use serde_json::{json, Value};
use tower::ServiceExt;

struct TemporaryMercuryFile {
    path: PathBuf,
}

impl TemporaryMercuryFile {
    fn new(contents: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "racon-api-analysis-job-{}-{unique}.mercury",
            std::process::id()
        ));
        fs::write(&path, contents).expect("temporary MercuryScript should be written");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryMercuryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn cycle_model() -> TemporaryMercuryFile {
    TemporaryMercuryFile::new(
        r#"
        SPN Model AnalysisCycle {
            place Reservoir(tokens = 1);
            place Processing;
            timedTransition Start(inputs = [Reservoir], outputs = [Processing], delay = 0.0);
            timedTransition Finish(inputs = [Processing], outputs = [Reservoir], delay = 0.0);
        }

        main {
        }
        "#,
    )
}

fn unbounded_counter_model() -> TemporaryMercuryFile {
    TemporaryMercuryFile::new(
        r#"
        SPN Model UnboundedCounter {
            place Counter(tokens = 1);
            timedTransition Grow(
                inputs = [Counter],
                outputs = [Counter(2)],
                delay = 0.0
            );
        }

        main {
        }
        "#,
    )
}

fn bounded_without_structural_certificate_model() -> TemporaryMercuryFile {
    TemporaryMercuryFile::new(
        r#"
        SPN Model InitiallyDisabledGrowth {
            place Counter;
            timedTransition Grow(
                inputs = [Counter],
                outputs = [Counter(2)],
                delay = 0.0
            );
        }

        main {
        }
        "#,
    )
}

fn cancellation_model() -> TemporaryMercuryFile {
    let mut places = String::new();
    let mut transitions = String::new();
    for bit in 0..16 {
        places.push_str(&format!(
            "place Bit{bit}Off(tokens = 1);\nplace Bit{bit}On;\n"
        ));
        transitions.push_str(&format!(
            "timedTransition Enable{bit}(inputs = [Bit{bit}Off], outputs = [Bit{bit}On], delay = 0.0);\n\
             timedTransition Disable{bit}(inputs = [Bit{bit}On], outputs = [Bit{bit}Off], delay = 0.0);\n"
        ));
    }
    TemporaryMercuryFile::new(&format!(
        "SPN Model Cancellation {{\n{places}{transitions}}}\nmain {{}}"
    ))
}

fn test_router() -> Router {
    router(ApiState::new(
        Arc::new(SessionRepository::default()),
        RouterContext { token: None },
    ))
}

fn request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("test request should be valid")
}

async fn json_response(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("router should answer the request");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let json = serde_json::from_slice(&body).expect("response body should be JSON");
    (status, json)
}

#[tokio::test]
async fn sse_streams_structured_progress_heartbeat_and_terminal_events() {
    let app = test_router();
    let model = cancellation_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space(&app, &session_id).await;

    let response = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/v1/analyses/{analysis_id}/events"),
            Value::Null,
        ))
        .await
        .expect("router should answer the SSE request");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&"text/event-stream".parse().unwrap())
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("SSE body should be readable");
    let body = String::from_utf8(body.to_vec()).expect("SSE body should be UTF-8");
    let events: Vec<Value> = body
        .split("\n\n")
        .filter_map(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
        .map(|data| serde_json::from_str(data).expect("SSE data should be JSON"))
        .collect();

    assert!(events.iter().any(|event| event["type"] == "progress"));
    assert!(events.iter().any(|event| event["type"] == "heartbeat"));
    assert!(events
        .iter()
        .any(|event| event["type"] == "cancelled" || event["type"] == "completed"));
    for event in events {
        assert!(event["analysis_id"].is_string());
        assert!(event["data"].is_object());
        assert!(event["data"].get("percentage").is_none());
        assert!(event["data"].get("eta").is_none());
    }
}

async fn load_model(app: &Router, model: &TemporaryMercuryFile) -> String {
    let (status, body) = json_response(
        app,
        request(
            Method::POST,
            "/v1/nets/load",
            json!({ "path": model.path() }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["session_id"]
        .as_str()
        .expect("load should return a session id")
        .to_owned()
}

async fn start_state_space(app: &Router, session_id: &str) -> String {
    let (status, body) = json_response(
        app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/state-space"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "RUNNING");
    body["analysis_id"]
        .as_str()
        .expect("start should return an analysis id")
        .to_owned()
}

async fn poll_terminal(app: &Router, analysis_id: &str) -> Value {
    for _ in 0..20_000 {
        let (status, body) = json_response(
            app,
            request(
                Method::GET,
                &format!("/v1/analyses/{analysis_id}/status"),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            body["status"].as_str(),
            Some("COMPLETED" | "CANCELLED" | "FAILED")
        ) {
            return body;
        }
        tokio::task::yield_now().await;
    }
    panic!("analysis {analysis_id} did not reach a terminal state");
}

fn assert_no_invented_estimates(body: &Value) {
    let object = body.as_object().expect("status should be a JSON object");
    for forbidden in ["percentage", "progress", "eta", "eta_ms"] {
        assert!(
            !object.contains_key(forbidden),
            "status must not expose invented field {forbidden}"
        );
    }
}

#[tokio::test]
async fn certified_bounded_job_completes_as_exact_reachability_with_observable_counts() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space(&app, &session_id).await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["analysis_id"], analysis_id);
    assert_eq!(completed["status"], "COMPLETED");
    assert_eq!(completed["phase"], "COMPLETED");
    assert_eq!(completed["states_discovered"], 2);
    assert_eq!(completed["states_processed"], 2);
    assert_eq!(completed["edges_discovered"], 2);
    assert_eq!(completed["frontier_size"], 0);
    assert_eq!(completed["omega_nodes"], 0);
    assert!(completed["elapsed_ms"].is_u64());
    assert_no_invented_estimates(&completed);

    let metadata = &completed["metadata"];
    assert_eq!(metadata["kind"], "REACHABILITY");
    assert_eq!(metadata["algorithm"], "BREADTH_FIRST_SEARCH");
    assert_eq!(metadata["exact"], true);
    assert_eq!(metadata["reduced"], false);
    assert_eq!(metadata["reductions"], json!([]));
    assert_eq!(metadata["nodes"], 2);
    assert_eq!(metadata["edges"], 2);
    assert_eq!(metadata["omega_nodes"], 0);
    assert!(metadata["elapsed_ms"].is_u64());
    assert!(metadata["peak_memory"].is_null());
    assert_eq!(metadata["completed"], true);
    assert_no_invented_estimates(metadata);
}

#[tokio::test]
async fn missing_boundedness_certificate_uses_coverability_and_reports_omega() {
    let app = test_router();
    let model = unbounded_counter_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space(&app, &session_id).await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["status"], "COMPLETED");
    assert_eq!(completed["metadata"]["kind"], "COVERABILITY");
    assert_eq!(completed["metadata"]["algorithm"], "KARP_MILLER");
    assert_eq!(completed["metadata"]["exact"], false);
    assert_eq!(completed["metadata"]["nodes"], 3);
    assert_eq!(completed["metadata"]["edges"], 3);
    assert_eq!(completed["metadata"]["omega_nodes"], 1);
}

#[tokio::test]
async fn bounded_coverability_result_does_not_claim_reachability_or_exactness() {
    let app = test_router();
    let model = bounded_without_structural_certificate_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space(&app, &session_id).await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["status"], "COMPLETED");
    assert_eq!(completed["metadata"]["kind"], "COVERABILITY");
    assert_eq!(completed["metadata"]["algorithm"], "KARP_MILLER");
    assert_eq!(completed["metadata"]["exact"], false);
    assert_eq!(completed["metadata"]["nodes"], 1);
    assert_eq!(completed["metadata"]["edges"], 0);
    assert_eq!(completed["metadata"]["omega_nodes"], 0);

    let (status, repeated) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/analyses/{analysis_id}/status"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["metadata"], completed["metadata"]);
}

#[tokio::test]
async fn running_job_can_be_cancelled_and_worker_cannot_overwrite_terminal_status() {
    let app = test_router();
    let model = cancellation_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space(&app, &session_id).await;

    let (status, cancelled) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/analyses/{analysis_id}/cancel"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["analysis_id"], analysis_id);
    assert_eq!(cancelled["status"], "CANCEL_REQUESTED");

    let terminal = poll_terminal(&app, &analysis_id).await;
    assert_eq!(terminal["status"], "CANCELLED");
    assert_eq!(terminal["phase"], "CANCELLED");
    assert!(terminal["metadata"].is_null());

    let (status, repeated) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/analyses/{analysis_id}/status"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["status"], "CANCELLED");
}

#[tokio::test]
async fn behavioral_properties_do_not_start_state_space_implicitly() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;

    let (status, body) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/properties?scope=behavioral"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["scope"], "behavioral");
    let rev = body["properties"]
        .as_array()
        .expect("properties should be an array")
        .iter()
        .find(|property| property["code"] == "REV")
        .expect("behavioral scope should expose REV");
    assert_eq!(rev["status"], "NOT_COMPUTED");
    assert_eq!(rev["requires"], json!(["STATE_SPACE"]));
    // Behavioral properties must not silently start heavy exploration.
    assert!(body["properties"]
        .as_array()
        .expect("properties should be an array")
        .iter()
        .all(|property| property["value"].is_null()));
}

async fn start_state_space_with_body(app: &Router, session_id: &str, body: Value) -> String {
    let (status, response) = json_response(
        app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/state-space"),
            body,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(response["status"], "RUNNING");
    response["analysis_id"]
        .as_str()
        .expect("start should return an analysis id")
        .to_owned()
}

#[tokio::test]
async fn symmetry_reduction_is_reported_and_never_fabricates_behavioral_properties() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space_with_body(
        &app,
        &session_id,
        json!({ "reductions": ["SYMMETRY"] }),
    )
    .await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["status"], "COMPLETED");
    let metadata = &completed["metadata"];
    assert_eq!(metadata["kind"], "REACHABILITY");
    assert_eq!(metadata["algorithm"], "SYMMETRY_REDUCED_BFS");
    assert_eq!(metadata["reduced"], true);
    assert_eq!(metadata["reductions"], json!(["SYMMETRY"]));
    assert_eq!(metadata["variant"], "symmetry");

    let (status, body) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/properties?scope=behavioral"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rev = body["properties"]
        .as_array()
        .expect("properties should be an array")
        .iter()
        .find(|property| property["code"] == "REV")
        .expect("behavioral scope should expose REV");
    assert_eq!(rev["status"], "UNKNOWN");
    assert_eq!(rev["source"], "REDUCED_STATE_SPACE");
    assert_eq!(
        rev["rule"],
        "STATE_SPACE_PROPERTY_NOT_PRESERVED_BY_REDUCTION"
    );
    // State-space-derived properties must stay unproven under a reduction.
    assert!(body["properties"]
        .as_array()
        .expect("properties should be an array")
        .iter()
        .filter(|property| property["source"] == "REDUCED_STATE_SPACE")
        .all(|property| property["value"].is_null()));
}

#[tokio::test]
async fn stubborn_reduction_is_reported_with_its_variant() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space_with_body(
        &app,
        &session_id,
        json!({ "reductions": ["STUBBORN"] }),
    )
    .await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["status"], "COMPLETED");
    let metadata = &completed["metadata"];
    assert_eq!(metadata["algorithm"], "STUBBORN_REDUCED_BFS");
    assert_eq!(metadata["reduced"], true);
    assert_eq!(metadata["reductions"], json!(["STUBBORN"]));
    assert_eq!(metadata["variant"], "stubborn");
}

#[tokio::test]
async fn finkel_minimal_coverability_is_reported_as_its_own_algorithm() {
    let app = test_router();
    let model = unbounded_counter_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space_with_body(
        &app,
        &session_id,
        json!({ "reductions": ["FINKEL_MINIMAL_COVERABILITY"] }),
    )
    .await;

    let completed = poll_terminal(&app, &analysis_id).await;
    assert_eq!(completed["status"], "COMPLETED");
    let metadata = &completed["metadata"];
    assert_eq!(metadata["kind"], "COVERABILITY");
    assert_eq!(metadata["algorithm"], "FINKEL_MINIMAL_COVERABILITY");
    assert_eq!(metadata["reduced"], true);
    assert_eq!(metadata["reductions"], json!(["FINKEL_MINIMAL_COVERABILITY"]));
}

#[tokio::test]
async fn unsupported_and_incompatible_reduction_requests_are_rejected() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;

    let (status, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/state-space"),
            json!({ "reductions": ["NOT_A_REDUCTION"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "REDUCTION_NOT_SUPPORTED");

    let (status, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/state-space"),
            json!({ "reductions": ["SYMMETRY", "FINKEL_MINIMAL_COVERABILITY"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "REDUCTION_NOT_APPLICABLE");
}

fn inhibitor_guarded_model() -> TemporaryMercuryFile {
    TemporaryMercuryFile::new(
        r#"
        SPN Model Guarded {
            place gate(tokens = 1);
            place src(tokens = 1);
            place dst;
            timedTransition t0(inputs = [src], outputs = [dst], inhibitors = [gate], delay = 0.0);
            immediateTransition t1(inputs = [gate], outputs = [gate]);
        }

        main {
        }
        "#,
    )
}

#[tokio::test]
async fn http_api_exposes_extended_semantics_for_inhibitors_and_immediates() {
    let app = test_router();
    let model = inhibitor_guarded_model();
    let (status, loaded) = json_response(
        &app,
        request(Method::POST, "/v1/nets/load", json!({ "path": model.path() })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(loaded["net"]["inhibitor_arcs"], 1);
    assert_eq!(loaded["net"]["immediate_transitions"], 1);
    assert_eq!(loaded["net"]["extended"], true);
    assert_eq!(loaded["net"]["semantics_version"], "racon-extended-pt-v1");
    assert_eq!(loaded["net"]["priority_policy"], "immediate_over_normal");

    let session_id = loaded["session_id"].as_str().unwrap().to_owned();
    let analysis_id = start_state_space(&app, &session_id).await;
    let terminal = poll_terminal(&app, &analysis_id).await;
    assert_eq!(terminal["status"], "COMPLETED", "{terminal}");
    let metadata = &terminal["metadata"];
    assert_eq!(metadata["analysis_scope"], "extended_behavior");
    assert_eq!(metadata["semantics_version"], "racon-extended-pt-v1");
    assert_eq!(metadata["kind"], "REACHABILITY");
    assert_eq!(metadata["algorithm"], "BREADTH_FIRST_SEARCH");
    assert_eq!(metadata["exact"], true);
    // t0 is inhibited by `gate` and the immediate self-loop t1 preempts nothing
    // else: the only state is M0 and t1 loops on it.
    assert_eq!(metadata["nodes"], 1);

    let (_, props) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/properties?scope=behavioral"),
            Value::Null,
        ),
    )
    .await;
    let dtr = props["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["code"] == "DTr")
        .unwrap();
    assert_eq!(dtr["value"], true, "t0 is dead under the extended semantics");
    assert_eq!(dtr["analysis_scope"], "extended_behavior");

    // reductions are refused up front, with a structured error
    let (status, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/state-space"),
            json!({ "reductions": ["STUBBORN"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "UNSUPPORTED");
}

#[tokio::test]
async fn algorithm_catalogue_and_validation_endpoints_follow_the_capability_matrix() {
    let app = test_router();
    let (status, catalogue) =
        json_response(&app, request(Method::GET, "/v1/analysis/algorithms", Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    let algorithms = catalogue["algorithms"].as_array().unwrap();
    let mincov = algorithms.iter().find(|a| a["id"] == "coverability_mincov").unwrap();
    assert_eq!(mincov["status"], "experimental");
    assert_eq!(mincov["selectable"], true);
    assert!(algorithms.iter().any(|a| a["id"] == "coverability_mp" && a["selectable"] == false));
    assert!(catalogue["references"].as_array().unwrap().iter().any(|r| r["id"] == "R6"));

    let (_, ok) = json_response(
        &app,
        request(
            Method::POST,
            "/v1/analysis/config/validate",
            json!({"objective": "coverability", "exploration": "coverability_mincov",
                   "net_class": "PT_WEIGHTED"}),
        ),
    )
    .await;
    assert_eq!(ok["status"], "valid");
    assert_eq!(ok["experimental"], true);

    let (_, blocked) = json_response(
        &app,
        request(
            Method::POST,
            "/v1/analysis/config/validate",
            json!({"objective": "coverability", "exploration": "coverability_karp_miller",
                   "reductions": ["partial_order_stubborn"], "net_class": "PT_WEIGHTED"}),
        ),
    )
    .await;
    assert_eq!(blocked["status"], "unsupported_combination");
}

#[tokio::test]
async fn explicit_dfs_job_builds_the_exact_graph_and_reports_its_algorithm() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;
    let analysis_id = start_state_space_with_body(
        &app,
        &session_id,
        json!({"exploration": "reachability_dfs"}),
    )
    .await;
    let terminal = poll_terminal(&app, &analysis_id).await;
    assert_eq!(terminal["status"], "COMPLETED", "{terminal}");
    assert_eq!(terminal["metadata"]["algorithm"], "DEPTH_FIRST_SEARCH");
    assert_eq!(terminal["metadata"]["kind"], "REACHABILITY");
    assert_eq!(terminal["metadata"]["exact"], true);
    assert_eq!(terminal["metadata"]["nodes"], 2);
}

#[tokio::test]
async fn unknown_or_unavailable_exploration_ids_are_rejected_with_a_structured_error() {
    let app = test_router();
    let model = cycle_model();
    let session_id = load_model(&app, &model).await;
    for id in ["coverability_mp", "coverability_mincov", "nonsense"] {
        let (status, body) = json_response(
            &app,
            request(
                Method::POST,
                &format!("/v1/sessions/{session_id}/state-space"),
                json!({ "exploration": id }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{id}");
        assert_eq!(body["error"]["code"], "UNSUPPORTED_COMBINATION");
    }
}

#[tokio::test]
async fn mincov_endpoint_returns_the_minimal_coverability_set_and_marks_it_experimental() {
    let app = test_router();
    let model = unbounded_counter_model();
    let session_id = load_model(&app, &model).await;
    let (status, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/coverability-set"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["algorithm"], "coverability_mincov");
    assert_eq!(body["kind"], "MINIMAL_COVERABILITY_SET");
    assert_eq!(body["experimental"], true);
    assert_eq!(body["bounded"], false);
    assert!(!body["unbounded_places"].as_array().unwrap().is_empty());
    assert!(body["elements"].to_string().contains("omega"));
    assert!(body["does_not_preserve"].to_string().contains("deadlock"));

    // bounded model: no omega
    let bounded = cycle_model();
    let bounded_id = load_model(&app, &bounded).await;
    let (_, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{bounded_id}/coverability-set"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(body["bounded"], true);
    assert!(!body["elements"].to_string().contains("omega"));
}

#[tokio::test]
async fn mincov_is_refused_for_extended_nets() {
    let app = test_router();
    let model = inhibitor_guarded_model();
    let session_id = load_model(&app, &model).await;
    let (status, body) = json_response(
        &app,
        request(
            Method::POST,
            &format!("/v1/sessions/{session_id}/coverability-set"),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "UNSUPPORTED_COMBINATION");
}
