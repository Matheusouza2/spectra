use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex, Notify};
use tokio::task::JoinHandle;
use uuid::Uuid;

const LOOPBACK_HOST: &str = "127.0.0.1";
const PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl RuntimeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Stopping => "STOPPING",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub port: Option<u16>,
    pub token_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            port: None,
            token_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub status: String,
    pub host: String,
    pub port: u16,
    pub protocol_version: String,
    pub pid: u32,
    pub token: Option<String>,
}

#[derive(Debug)]
pub enum RuntimeError {
    PortUnavailable { port: u16, source: io::Error },
    BindFailed(io::Error),
    ServerFailed(io::Error),
    ServerTaskFailed(String),
}

impl RuntimeError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PortUnavailable { .. } => "PORT_UNAVAILABLE",
            Self::BindFailed(_) | Self::ServerFailed(_) | Self::ServerTaskFailed(_) => {
                "INTERNAL_ERROR"
            }
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortUnavailable { port, source } => {
                write!(
                    formatter,
                    "PORT_UNAVAILABLE: cannot bind 127.0.0.1:{port}: {source}"
                )
            }
            Self::BindFailed(source) => write!(formatter, "failed to bind local server: {source}"),
            Self::ServerFailed(source) => write!(formatter, "local server failed: {source}"),
            Self::ServerTaskFailed(message) => {
                write!(formatter, "local server task failed: {message}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PortUnavailable { source, .. }
            | Self::BindFailed(source)
            | Self::ServerFailed(source) => Some(source),
            Self::ServerTaskFailed(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    config: RuntimeConfig,
    router: Router,
    lifecycle: Mutex<Lifecycle>,
    state_changed: Notify,
}

struct Lifecycle {
    state: RuntimeState,
    info: Option<ServerInfo>,
    shutdown: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<io::Result<()>>>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            state: RuntimeState::Stopped,
            info: None,
            shutdown: None,
            server_task: None,
        }
    }
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self::with_router(config, Router::new())
    }

    pub fn with_router(config: RuntimeConfig, router: Router) -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                config,
                router,
                lifecycle: Mutex::new(Lifecycle::default()),
                state_changed: Notify::new(),
            }),
        }
    }

    pub async fn start_server(&self) -> Result<ServerInfo, RuntimeError> {
        loop {
            let state_changed = self.inner.state_changed.notified();
            let mut lifecycle = self.inner.lifecycle.lock().await;

            match lifecycle.state {
                RuntimeState::Running => {
                    return Ok(lifecycle
                        .info
                        .clone()
                        .expect("RUNNING runtime must retain its server information"));
                }
                RuntimeState::Starting | RuntimeState::Stopping => {
                    drop(lifecycle);
                    state_changed.await;
                }
                RuntimeState::Stopped | RuntimeState::Failed => {
                    lifecycle.state = RuntimeState::Starting;
                    lifecycle.info = None;
                    drop(lifecycle);
                    break;
                }
            }
        }

        let configured_port = self.inner.config.port.unwrap_or(0);
        let bind_address = SocketAddr::from(([127, 0, 0, 1], configured_port));
        let listener = match TcpListener::bind(bind_address).await {
            Ok(listener) => listener,
            Err(source) => {
                self.set_failed().await;
                return Err(if configured_port == 0 {
                    RuntimeError::BindFailed(source)
                } else {
                    RuntimeError::PortUnavailable {
                        port: configured_port,
                        source,
                    }
                });
            }
        };

        let local_address = match listener.local_addr() {
            Ok(address) => address,
            Err(source) => {
                self.set_failed().await;
                return Err(RuntimeError::BindFailed(source));
            }
        };
        let info = ServerInfo {
            status: "ready".to_owned(),
            host: LOOPBACK_HOST.to_owned(),
            port: local_address.port(),
            protocol_version: PROTOCOL_VERSION.to_owned(),
            pid: std::process::id(),
            token: self
                .inner
                .config
                .token_enabled
                .then(|| Uuid::new_v4().to_string()),
        };

        let (shutdown, shutdown_requested) = oneshot::channel();
        let router = self.inner.router.clone();
        let server_task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_requested.await;
                })
                .await
        });

        let mut lifecycle = self.inner.lifecycle.lock().await;
        lifecycle.info = Some(info.clone());
        lifecycle.shutdown = Some(shutdown);
        lifecycle.server_task = Some(server_task);
        lifecycle.state = RuntimeState::Running;
        drop(lifecycle);
        self.inner.state_changed.notify_waiters();

        Ok(info)
    }

    pub async fn stop_server(&self) -> Result<(), RuntimeError> {
        let (shutdown, server_task) = loop {
            let state_changed = self.inner.state_changed.notified();
            let mut lifecycle = self.inner.lifecycle.lock().await;

            match lifecycle.state {
                RuntimeState::Stopped => return Ok(()),
                RuntimeState::Failed => {
                    lifecycle.state = RuntimeState::Stopped;
                    lifecycle.info = None;
                    lifecycle.shutdown = None;
                    lifecycle.server_task = None;
                    drop(lifecycle);
                    self.inner.state_changed.notify_waiters();
                    return Ok(());
                }
                RuntimeState::Starting | RuntimeState::Stopping => {
                    drop(lifecycle);
                    state_changed.await;
                }
                RuntimeState::Running => {
                    lifecycle.state = RuntimeState::Stopping;
                    let shutdown = lifecycle
                        .shutdown
                        .take()
                        .expect("RUNNING runtime must retain a shutdown sender");
                    let server_task = lifecycle
                        .server_task
                        .take()
                        .expect("RUNNING runtime must retain its server task");
                    drop(lifecycle);
                    break (shutdown, server_task);
                }
            }
        };

        let _ = shutdown.send(());
        let result = match server_task.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(source)) => Err(RuntimeError::ServerFailed(source)),
            Err(error) => Err(RuntimeError::ServerTaskFailed(error.to_string())),
        };

        let mut lifecycle = self.inner.lifecycle.lock().await;
        lifecycle.info = None;
        lifecycle.state = if result.is_ok() {
            RuntimeState::Stopped
        } else {
            RuntimeState::Failed
        };
        drop(lifecycle);
        self.inner.state_changed.notify_waiters();

        result
    }

    pub async fn server_status(&self) -> RuntimeState {
        self.inner.lifecycle.lock().await.state
    }

    async fn set_failed(&self) {
        let mut lifecycle = self.inner.lifecycle.lock().await;
        lifecycle.state = RuntimeState::Failed;
        lifecycle.info = None;
        lifecycle.shutdown = None;
        lifecycle.server_task = None;
        drop(lifecycle);
        self.inner.state_changed.notify_waiters();
    }
}

pub async fn start_server(runtime: &Runtime) -> Result<ServerInfo, RuntimeError> {
    runtime.start_server().await
}

pub async fn stop_server(runtime: &Runtime) -> Result<(), RuntimeError> {
    runtime.stop_server().await
}

pub async fn server_status(runtime: &Runtime) -> RuntimeState {
    runtime.server_status().await
}
