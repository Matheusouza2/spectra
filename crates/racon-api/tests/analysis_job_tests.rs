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
    assert_eq!(cancelled["status"], "CANCELLED");

    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    let terminal = poll_terminal(&app, &analysis_id).await;
    assert_eq!(terminal["status"], "CANCELLED");
    assert!(terminal["metadata"].is_null());
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
    assert_eq!(body["properties"][0]["code"], "REV");
    assert_eq!(body["properties"][0]["status"], "NOT_COMPUTED");
    assert_eq!(body["properties"][0]["requires"], json!(["STATE_SPACE"]));
}
