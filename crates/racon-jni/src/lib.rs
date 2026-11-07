//! RACoN JNI bootstrap bridge.
//!
//! This crate deliberately contains **no analysis logic**. It only exposes the
//! process lifecycle of the local HTTP service so a JVM host can start it,
//! stop it, and query its status. Every analytical operation is expected to go
//! through the HTTP/JSON contract on `127.0.0.1`.

use std::sync::{Arc, Mutex, OnceLock};

use jni::objects::JClass;
use jni::sys::jstring;
use jni::JNIEnv;
use racon_api::{router, ApiState};
use racon_engine::SessionRepository;
use racon_runtime::{RouterContext, Runtime, RuntimeConfig, ServerInfo};
use serde_json::json;

const PROTOCOL_VERSION: &str = "1.0";

struct ServerHandle {
    runtime: Runtime,
    info: Option<ServerInfo>,
}

static TOKIO: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static SERVER: OnceLock<Mutex<Option<ServerHandle>>> = OnceLock::new();

fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    TOKIO.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build the RACoN JNI runtime")
    })
}

fn server_slot() -> &'static Mutex<Option<ServerHandle>> {
    SERVER.get_or_init(|| Mutex::new(None))
}

fn server_info_json(info: &ServerInfo) -> String {
    let mut value = json!({
        "status": info.status,
        "host": info.host,
        "port": info.port,
        "protocol_version": info.protocol_version,
        "pid": info.pid,
    });
    if let Some(token) = &info.token {
        value["token"] = json!(token);
    }
    value.to_string()
}

fn error_json(code: &str, message: &str) -> String {
    json!({"error": {"code": code, "message": message}}).to_string()
}

fn to_java_string(env: &mut JNIEnv<'_>, value: String) -> jstring {
    match env.new_string(value) {
        Ok(output) => output.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn start_server_json() -> Result<String, (&'static str, String)> {
    let mut slot = server_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if slot.is_none() {
        let sessions = Arc::new(SessionRepository::default());
        let runtime = Runtime::with_router(
            RuntimeConfig {
                port: None,
                token_enabled: true,
            },
            move |context: RouterContext| {
                let sessions = Arc::clone(&sessions);
                Box::pin(async move { router(ApiState::new(sessions, context)) })
            },
        );
        *slot = Some(ServerHandle {
            runtime,
            info: None,
        });
    }

    let handle = slot.as_mut().expect("server handle was just created");
    let info = tokio_runtime()
        .block_on(handle.runtime.start_server())
        .map_err(|error| (error.code(), error.to_string()))?;
    handle.info = Some(info.clone());
    Ok(server_info_json(&info))
}

fn stop_server_json() -> String {
    let mut slot = server_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match slot.take() {
        Some(handle) => match tokio_runtime().block_on(handle.runtime.stop_server()) {
            Ok(()) => json!({"status": "STOPPED", "protocol_version": PROTOCOL_VERSION}).to_string(),
            Err(error) => error_json(error.code(), &error.to_string()),
        },
        None => json!({"status": "STOPPED", "protocol_version": PROTOCOL_VERSION}).to_string(),
    }
}

fn server_status_json() -> String {
    let slot = server_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match slot.as_ref() {
        Some(handle) => {
            let state = tokio_runtime().block_on(handle.runtime.server_status());
            let mut value = json!({
                "status": state.as_str(),
                "protocol_version": PROTOCOL_VERSION,
            });
            if let Some(info) = &handle.info {
                value["host"] = json!(info.host);
                value["port"] = json!(info.port);
                value["pid"] = json!(info.pid);
            }
            value.to_string()
        }
        None => json!({
            "status": "STOPPED",
            "protocol_version": PROTOCOL_VERSION,
        })
        .to_string(),
    }
}

/// `racon.RaconRuntime.startServer()` — starts the local service and returns a
/// JSON-encoded [`ServerInfo`], or `{"error": {...}}`.
#[no_mangle]
pub extern "system" fn Java_racon_RaconRuntime_startServer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    let output = start_server_json().unwrap_or_else(|(code, message)| error_json(&code, &message));
    to_java_string(&mut env, output)
}

/// `racon.RaconRuntime.stopServer()` — stops the local service. Idempotent.
#[no_mangle]
pub extern "system" fn Java_racon_RaconRuntime_stopServer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java_string(&mut env, stop_server_json())
}

/// `racon.RaconRuntime.serverStatus()` — returns the lifecycle state as JSON.
#[no_mangle]
pub extern "system" fn Java_racon_RaconRuntime_serverStatus(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java_string(&mut env, server_status_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_lifecycle_json_round_trips_through_the_bridge() {
        let started = start_server_json().expect("server should start");
        let value: serde_json::Value = serde_json::from_str(&started).unwrap();
        assert_eq!(value["protocol_version"], PROTOCOL_VERSION);
        assert!(value["port"].as_u64().unwrap() > 0);
        assert!(value["token"].as_str().unwrap().len() > 8);

        let status: serde_json::Value = serde_json::from_str(&server_status_json()).unwrap();
        assert_eq!(status["status"], "RUNNING");

        let stopped: serde_json::Value = serde_json::from_str(&stop_server_json()).unwrap();
        assert_eq!(stopped["status"], "STOPPED");

        let stopped_again = stop_server_json();
        let value: serde_json::Value = serde_json::from_str(&stopped_again).unwrap();
        assert_eq!(value["status"], "STOPPED");
    }
}
