use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use racon_api::{router, ApiState};
use racon_engine::SessionRepository;
use racon_runtime::{RouterContext, Runtime, RuntimeConfig};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
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
            "racon-api-http-contract-{}-{unique}.mercury",
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
        SPN Model ApiCycle {
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

fn test_router(token: Option<&str>) -> Router {
    router(ApiState::new(
        Arc::new(SessionRepository::default()),
        RouterContext {
            token: token.map(str::to_owned),
        },
    ))
}

fn request(method: Method, uri: &str, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
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

async fn load_model(app: &Router, model: &TemporaryMercuryFile) -> Value {
    let (status, body) = json_response(
        app,
        request(
            Method::POST,
            "/v1/nets/load",
            json!({ "path": model.path() }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body
}

#[tokio::test]
async fn health_is_cheap_versioned_json_and_unversioned_route_is_absent() {
    let app = test_router(None);

    let (status, body) =
        json_response(&app, request(Method::GET, "/v1/health", Value::Null, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["protocol_version"], "1.0");
    assert_eq!(body["engine_version"], env!("CARGO_PKG_VERSION"));

    let response = app
        .oneshot(request(Method::GET, "/health", Value::Null, None))
        .await
        .expect("router should answer unknown routes");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn load_persists_a_session_for_structural_properties_and_invariants() {
    let model = cycle_model();
    let app = test_router(None);
    let load = load_model(&app, &model).await;

    assert!(load["session_id"]
        .as_str()
        .is_some_and(|session_id| !session_id.is_empty()));
    assert_eq!(load["source"]["type"], "mercury_script");
    assert_eq!(
        load["source"]["path"],
        model.path().to_string_lossy().as_ref()
    );
    assert_eq!(
        load["net"],
        json!({ "places": 2, "transitions": 2, "arcs": 4 })
    );
    assert_eq!(load["properties"][0]["code"], "ORD");

    let session_id = load["session_id"]
        .as_str()
        .expect("load should return a session id");
    let (status, properties) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/properties?scope=structural"),
            Value::Null,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(properties["scope"], "structural");
    assert_eq!(properties["properties"][0]["code"], "ORD");
    assert!(properties["properties"]
        .as_array()
        .is_some_and(|items| items.iter().all(|item| item["source"] == "STRUCTURAL")));

    let (status, invariants) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/invariants?kind=ALL&minimal=true"),
            Value::Null,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invariants["invariants"].as_array().map(Vec::len), Some(2));
    assert_eq!(invariants["invariants"][0]["minimal"], true);
    assert!(invariants["invariants"].as_array().is_some_and(|items| {
        items
            .iter()
            .all(|item| matches!(item["type"].as_str(), Some("PLACE" | "TRANSITION")))
    }));
}

#[tokio::test]
async fn missing_sessions_and_load_failures_use_structured_spec_errors() {
    let app = test_router(None);
    let (status, missing_session) = json_response(
        &app,
        request(
            Method::GET,
            "/v1/sessions/does-not-exist/properties?scope=structural",
            Value::Null,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing_session["error"]["code"], "SESSION_NOT_FOUND");
    assert!(missing_session["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("does-not-exist")));

    let missing_path = std::env::temp_dir().join(format!(
        "racon-api-missing-{}-{}.mercury",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos()
    ));
    let (status, missing_file) = json_response(
        &app,
        request(
            Method::POST,
            "/v1/nets/load",
            json!({ "path": missing_path }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing_file["error"]["code"], "FILE_NOT_FOUND");

    let invalid_script = TemporaryMercuryFile::new(
        r#"
        SPN Model Broken {
            place p0(tokens = 1);
            timedTransition missing_parenthesis;
        }
        "#,
    );
    let (status, invalid_script_body) = json_response(
        &app,
        request(
            Method::POST,
            "/v1/nets/load",
            json!({ "path": invalid_script.path() }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid_script_body["error"]["code"],
        "INVALID_MERCURY_SCRIPT"
    );

    let invalid_net = TemporaryMercuryFile::new("SPN Model Empty { }");
    let (status, invalid_net_body) = json_response(
        &app,
        request(
            Method::POST,
            "/v1/nets/load",
            json!({ "path": invalid_net.path() }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_net_body["error"]["code"], "INVALID_NET");
}

#[tokio::test]
async fn unsupported_invariant_filters_return_conflict_without_populating_a_result() {
    let model = cycle_model();
    let app = test_router(None);
    let load = load_model(&app, &model).await;
    let session_id = load["session_id"]
        .as_str()
        .expect("load should return a session id");

    let (status, body) = json_response(
        &app,
        request(
            Method::GET,
            &format!("/v1/sessions/{session_id}/invariants?sub=true"),
            Value::Null,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "NOT_IMPLEMENTED");
}

#[tokio::test]
async fn runtime_factory_applies_its_bearer_token_before_serving_protected_routes() {
    let sessions = Arc::new(SessionRepository::default());
    let runtime = Runtime::with_router(RuntimeConfig::default(), {
        let sessions = Arc::clone(&sessions);
        move |context| {
            let state = ApiState::new(Arc::clone(&sessions), context);
            async move { router(state) }
        }
    });
    let info = runtime
        .start_server()
        .await
        .expect("runtime with API router should start");
    let token = info
        .token
        .as_deref()
        .expect("default runtime should issue a token");

    let public = raw_http_request(info.port, "GET", "/v1/health", None).await;
    assert!(public.starts_with("HTTP/1.1 200 OK"));

    let unauthorized = raw_http_request(info.port, "POST", "/v1/nets/load", None).await;
    assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(unauthorized.contains("\"code\":\"UNAUTHORIZED\""));

    let authorized = raw_http_request(
        info.port,
        "GET",
        "/v1/sessions/missing/properties?scope=structural",
        Some(token),
    )
    .await;
    assert!(authorized.starts_with("HTTP/1.1 404 Not Found"));
    assert!(authorized.contains("\"code\":\"SESSION_NOT_FOUND\""));

    runtime
        .stop_server()
        .await
        .expect("runtime with API router should stop");
}

async fn raw_http_request(port: u16, method: &str, path: &str, token: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("runtime listener should accept connections");
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request should be written");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("response should be read");
    String::from_utf8(response).expect("HTTP response should be UTF-8")
}
