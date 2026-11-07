//! Simple HTTP service benchmark for RACoN (§48).
//!
//! Measures request round-trip latency and JSON (de)serialization latency for
//! the cheap endpoints (`GET /v1/health`, `POST /v1/nets/load`,
//! `GET /v1/sessions/{id}/properties`). The goal is to show the HTTP layer is
//! not the bottleneck relative to state-space exploration, not to win a
//! microbenchmark contest.

use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use racon_api::{router, ApiState};
use racon_engine::SessionRepository;
use racon_runtime::RouterContext;
use serde_json::{json, Value};
use tower::ServiceExt;

const ITERATIONS: usize = 200;

struct Stats {
    samples: Vec<Duration>,
}

impl Stats {
    fn new() -> Self {
        Self { samples: Vec::with_capacity(ITERATIONS) }
    }

    fn record(&mut self, duration: Duration) {
        self.samples.push(duration);
    }

    fn summary(&self) -> (f64, f64, f64) {
        let mut values: Vec<f64> = self.samples.iter().map(|d| d.as_secs_f64() * 1e6).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        let p50 = values[values.len() / 2];
        let p99 = values[(values.len() * 99 / 100).min(values.len() - 1)];
        (avg, p50, p99)
    }
}

fn request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("valid request")
}

fn temporary_model() -> std::path::PathBuf {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("racon-http-bench-{unique}.mercury"));
    fs::write(
        &path,
        "SPN Model Bench {\n    place A(tokens = 1);\n    place B;\n    timedTransition T(inputs = [A], outputs = [B], delay = 0.0);\n    timedTransition U(inputs = [B], outputs = [A], delay = 0.0);\n}\n",
    )
    .unwrap();
    path
}

async fn timed_json(
    app: &Router,
    req: Request<Body>,
    request_stats: &mut Stats,
    serialize_stats: &mut Stats,
) -> (StatusCode, Value) {
    let started = Instant::now();
    let response = app.clone().oneshot(req).await.expect("router responds");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    request_stats.record(started.elapsed());

    let started = Instant::now();
    let value: Value = serde_json::from_slice(&body).expect("json body");
    serialize_stats.record(started.elapsed());
    (status, value)
}

#[tokio::main]
async fn main() {
    let app = router(ApiState::new(
        Arc::new(SessionRepository::default()),
        RouterContext { token: None },
    ));
    let model = temporary_model();

    let (mut health_req, mut health_ser) = (Stats::new(), Stats::new());
    for _ in 0..ITERATIONS {
        let (status, _) = timed_json(
            &app,
            request(Method::GET, "/v1/health", json!({})),
            &mut health_req,
            &mut health_ser,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (mut load_req, mut load_ser) = (Stats::new(), Stats::new());
    let mut last_session = String::new();
    for _ in 0..ITERATIONS {
        let (status, body) = timed_json(
            &app,
            request(Method::POST, "/v1/nets/load", json!({ "path": model })),
            &mut load_req,
            &mut load_ser,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        last_session = body["session_id"].as_str().unwrap().to_owned();
    }

    let (mut properties_req, mut properties_ser) = (Stats::new(), Stats::new());
    for _ in 0..ITERATIONS {
        let (status, _) = timed_json(
            &app,
            request(
                Method::GET,
                &format!("/v1/sessions/{last_session}/properties"),
                json!({}),
            ),
            &mut properties_req,
            &mut properties_ser,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let _ = fs::remove_file(&model);

    println!("RACoN HTTP benchmark ({ITERATIONS} iterations)");
    println!("{:<38} {:>10} {:>10} {:>10}", "endpoint / cost", "avg us", "p50 us", "p99 us");
    for (label, stats) in [
        ("GET /v1/health (request)", &health_req),
        ("GET /v1/health (deserialize)", &health_ser),
        ("POST /v1/nets/load (request)", &load_req),
        ("POST /v1/nets/load (deserialize)", &load_ser),
        ("GET .../properties (request)", &properties_req),
        ("GET .../properties (deserialize)", &properties_ser),
    ] {
        let (avg, p50, p99) = stats.summary();
        println!("{label:<38} {avg:>10.1} {p50:>10.1} {p99:>10.1}");
    }
}
