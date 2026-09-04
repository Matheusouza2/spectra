use std::net::{Ipv4Addr, TcpListener as StdTcpListener};

use axum::{routing::get, Router as AxumRouter};
use racon_runtime::{
    server_status, start_server, stop_server, Runtime, RuntimeConfig, RuntimeState,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn reserve_available_port() -> (StdTcpListener, u16) {
    let listener = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("an available loopback port should be reservable");
    let port = listener
        .local_addr()
        .expect("reserved listener should have a local address")
        .port();
    (listener, port)
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

    assert_eq!(first, second);
    assert_eq!(first.status, "ready");
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
    let runtime = Runtime::with_router(RuntimeConfig::default(), router);
    let info = runtime
        .start_server()
        .await
        .expect("server should start with the injected router");

    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, info.port))
        .await
        .expect("server should accept a request");
    stream
        .write_all(b"GET /probe HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .await
        .expect("request should be written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("response should be read");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("router-ok"), "{response}");

    runtime
        .stop_server()
        .await
        .expect("server should stop after serving the route");
}
