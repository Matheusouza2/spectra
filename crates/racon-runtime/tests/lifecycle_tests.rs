use std::net::{Ipv4Addr, TcpListener as StdTcpListener};
use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router as AxumRouter};
use racon_runtime::{
    server_status, start_server, stop_server, RouterContext, Runtime, RuntimeConfig, RuntimeState,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::time::timeout;

fn reserve_available_port() -> (StdTcpListener, u16) {
    let listener = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("an available loopback port should be reservable");
    let port = listener
        .local_addr()
        .expect("reserved listener should have a local address")
        .port();
    (listener, port)
}

async fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .expect("server should accept a request");
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request should be written");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("response should be read");
    response
}

async fn wait_for_state(runtime: &Runtime, expected: RuntimeState) {
    timeout(Duration::from_secs(2), async {
        loop {
            if runtime.server_status().await == expected {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("runtime did not reach {}", expected.as_str()));
}

#[tokio::test]
async fn constructing_runtime_does_not_open_a_listener() {
    let (reservation, port) = reserve_available_port();
    drop(reservation);

    let runtime = Runtime::new(RuntimeConfig {
        port: Some(port),
        token_enabled: true,
    });

    let probe = StdTcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .expect("constructing Runtime must leave the configured port free");

    assert_eq!(server_status(&runtime).await, RuntimeState::Stopped);
    drop(probe);
}

#[tokio::test]
async fn start_binds_an_ephemeral_loopback_port_and_is_idempotent() {
    let runtime = Runtime::new(RuntimeConfig::default());

    let first = start_server(&runtime)
        .await
        .expect("server should start on an ephemeral port");
    let second = runtime
        .start_server()
        .await
        .expect("starting a running server should be idempotent");

    assert_eq!(first.status, "ready");
    assert_eq!(second.status, "already_running");
    assert_eq!(first.host, second.host);
    assert_eq!(first.port, second.port);
    assert_eq!(first.token, second.token);
    assert_eq!(first.host, "127.0.0.1");
    assert_ne!(first.port, 0);
    assert_eq!(first.protocol_version, "1.0");
    assert_eq!(first.pid, std::process::id());
    assert!(first.token.as_ref().is_some_and(|token| !token.is_empty()));
    assert_eq!(runtime.server_status().await, RuntimeState::Running);
    TcpStream::connect((Ipv4Addr::LOCALHOST, first.port))
        .await
        .expect("reported server address should accept connections");

    stop_server(&runtime)
        .await
        .expect("server should stop cleanly");
}

#[tokio::test]
async fn stop_is_idempotent_and_releases_the_listener() {
    let runtime = Runtime::new(RuntimeConfig::default());
    let info = runtime
        .start_server()
        .await
        .expect("server should start before it is stopped");

    runtime
        .stop_server()
        .await
        .expect("first stop should succeed");
    runtime
        .stop_server()
        .await
        .expect("repeated stop should be idempotent");

    assert_eq!(runtime.server_status().await, RuntimeState::Stopped);
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, info.port))
        .expect("graceful shutdown should release the listener");
}

#[tokio::test]
async fn separate_runtime_instances_receive_different_tokens() {
    let first_runtime = Runtime::new(RuntimeConfig::default());
    let second_runtime = Runtime::new(RuntimeConfig::default());

    let first = first_runtime
        .start_server()
        .await
        .expect("first server should start");
    let second = second_runtime
        .start_server()
        .await
        .expect("second server should start");

    assert_ne!(first.token, second.token);

    first_runtime
        .stop_server()
        .await
        .expect("first server should stop");
    second_runtime
        .stop_server()
        .await
        .expect("second server should stop");
}

#[tokio::test]
async fn occupied_explicit_port_reports_port_unavailable() {
    let (_reservation, port) = reserve_available_port();
    let runtime = Runtime::new(RuntimeConfig {
        port: Some(port),
        token_enabled: false,
    });

    let error = runtime
        .start_server()
        .await
        .expect_err("an occupied explicit port must not silently fall back");

    assert_eq!(error.code(), "PORT_UNAVAILABLE");
    assert_eq!(runtime.server_status().await, RuntimeState::Failed);
}

#[tokio::test]
async fn runtime_serves_the_injected_api_router() {
    let router = AxumRouter::new().route("/probe", get(|| async { "router-ok" }));
    let runtime = Runtime::with_router(RuntimeConfig::default(), move |_| {
        let router = router.clone();
        async move { router }
    });
    let info = runtime
        .start_server()
        .await
        .expect("server should start with the injected router");

    let response = http_get(info.port, "/probe").await;

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("router-ok"), "{response}");

    runtime
        .stop_server()
        .await
        .expect("server should stop after serving the route");
}

#[tokio::test]
async fn router_factory_receives_the_server_token_before_bind_and_serve() {
    let (reservation, port) = reserve_available_port();
    drop(reservation);
    let runtime = Runtime::with_router(
        RuntimeConfig {
            port: Some(port),
            token_enabled: true,
        },
        move |context: RouterContext| async move {
            let token = context
                .token
                .expect("factory should receive the enabled token");
            let port_was_free = StdTcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok();
            let body = format!("{token}|port_free={port_was_free}");

            AxumRouter::new().route(
                "/bootstrap-context",
                get(move || {
                    let body = body.clone();
                    async move { body }
                }),
            )
        },
    );

    let info = runtime
        .start_server()
        .await
        .expect("server should start with a router factory");
    let response = http_get(info.port, "/bootstrap-context").await;
    let body = response
        .split_once("\r\n\r\n")
        .expect("HTTP response should contain a body")
        .1;

    assert_eq!(
        body,
        format!(
            "{}|port_free=true",
            info.token
                .as_deref()
                .expect("server info should expose the factory token")
        )
    );

    runtime
        .stop_server()
        .await
        .expect("server should stop after factory test");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_start_caller_does_not_abandon_starting_transition() {
    let factory_entered = Arc::new(Notify::new());
    let release_factory = Arc::new(Notify::new());
    let runtime = Runtime::with_router(RuntimeConfig::default(), {
        let factory_entered = Arc::clone(&factory_entered);
        let release_factory = Arc::clone(&release_factory);
        move |_| {
            let factory_entered = Arc::clone(&factory_entered);
            let release_factory = Arc::clone(&release_factory);
            async move {
                factory_entered.notify_one();
                release_factory.notified().await;
                AxumRouter::new()
            }
        }
    });

    let entered = factory_entered.notified();
    let start_task = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.start_server().await }
    });
    entered.await;
    assert_eq!(runtime.server_status().await, RuntimeState::Starting);

    start_task.abort();
    release_factory.notify_one();
    assert!(
        start_task
            .await
            .expect_err("start caller should be cancelled")
            .is_cancelled(),
        "join error should report caller cancellation"
    );

    let info = timeout(Duration::from_secs(2), runtime.start_server())
        .await
        .expect("inner start transition must survive caller cancellation")
        .expect("surviving start transition should succeed");
    assert_eq!(runtime.server_status().await, RuntimeState::Running);

    runtime
        .stop_server()
        .await
        .expect("server should stop after cancellation regression");
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, info.port))
        .expect("shutdown should release the listener");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_stop_caller_does_not_abandon_stopping_transition() {
    let request_entered = Arc::new(Notify::new());
    let release_request = Arc::new(Notify::new());
    let router = AxumRouter::new().route(
        "/hold",
        get({
            let request_entered = Arc::clone(&request_entered);
            let release_request = Arc::clone(&release_request);
            move || {
                let request_entered = Arc::clone(&request_entered);
                let release_request = Arc::clone(&release_request);
                async move {
                    request_entered.notify_one();
                    release_request.notified().await;
                    "released"
                }
            }
        }),
    );
    let runtime = Runtime::with_router(RuntimeConfig::default(), move |_| {
        let router = router.clone();
        async move { router }
    });
    let info = runtime
        .start_server()
        .await
        .expect("server should start before stop cancellation test");

    let entered = request_entered.notified();
    let request_task = tokio::spawn(async move { http_get(info.port, "/hold").await });
    entered.await;

    let stop_task = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.stop_server().await }
    });
    wait_for_state(&runtime, RuntimeState::Stopping).await;
    stop_task.abort();
    assert!(
        stop_task
            .await
            .expect_err("stop caller should be cancelled")
            .is_cancelled(),
        "join error should report caller cancellation"
    );

    release_request.notify_one();
    let response = request_task.await.expect("request task should finish");
    assert!(response.ends_with("released"), "{response}");

    timeout(Duration::from_secs(2), runtime.stop_server())
        .await
        .expect("inner stop transition must survive caller cancellation")
        .expect("surviving stop transition should succeed");
    assert_eq!(runtime.server_status().await, RuntimeState::Stopped);
}
