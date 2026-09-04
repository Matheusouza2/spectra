use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

const LOOPBACK_HOST: &str = "127.0.0.1";
const PROTOCOL_VERSION: &str = "1.0";

type RouterFuture = Pin<Box<dyn Future<Output = Router> + Send>>;
type RouterFactory = dyn Fn(RouterContext) -> RouterFuture + Send + Sync;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterContext {
    pub token: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RuntimeError {
    PortUnavailable { port: u16, source: Arc<io::Error> },
    BindFailed(Arc<io::Error>),
    ServerFailed(Arc<io::Error>),
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
            | Self::ServerFailed(source) => Some(source.as_ref()),
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
    router_factory: Arc<RouterFactory>,
    lifecycle: Mutex<Lifecycle>,
}

struct Lifecycle {
    state: RuntimeState,
    info: Option<ServerInfo>,
    shutdown: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<io::Result<()>>>,
    transition: Option<watch::Receiver<TransitionOutcome>>,
}

#[derive(Clone)]
enum TransitionOutcome {
    Pending,
    Started(Result<ServerInfo, RuntimeError>),
    Stopped(Result<(), RuntimeError>),
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            state: RuntimeState::Stopped,
            info: None,
            shutdown: None,
            server_task: None,
            transition: None,
        }
    }
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self::with_router(config, |_| async { Router::new() })
    }

    pub fn with_router<F, Fut>(config: RuntimeConfig, router_factory: F) -> Self
    where
        F: Fn(RouterContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Router> + Send + 'static,
    {
        Self {
            inner: Arc::new(RuntimeInner {
                config,
                router_factory: Arc::new(move |context| Box::pin(router_factory(context))),
                lifecycle: Mutex::new(Lifecycle::default()),
            }),
        }
    }

    pub async fn start_server(&self) -> Result<ServerInfo, RuntimeError> {
        loop {
            let action = {
                let mut lifecycle = self.inner.lifecycle.lock().await;

                match lifecycle.state {
                    RuntimeState::Running => StartAction::Return(
                        lifecycle
                            .info
                            .clone()
                            .expect("RUNNING runtime must retain its server information"),
                    ),
                    RuntimeState::Starting => StartAction::WaitForStart(
                        lifecycle
                            .transition
                            .as_ref()
                            .expect("STARTING runtime must retain its transition")
                            .clone(),
                    ),
                    RuntimeState::Stopping => StartAction::WaitForOther(
                        lifecycle
                            .transition
                            .as_ref()
                            .expect("STOPPING runtime must retain its transition")
                            .clone(),
                    ),
                    RuntimeState::Stopped | RuntimeState::Failed => {
                        let (completion, transition) = watch::channel(TransitionOutcome::Pending);
                        lifecycle.state = RuntimeState::Starting;
                        lifecycle.info = None;
                        lifecycle.transition = Some(transition.clone());
                        StartAction::Begin {
                            completion,
                            transition,
                        }
                    }
                }
            };

            match action {
                StartAction::Return(info) => return Ok(info),
                StartAction::Begin {
                    completion,
                    transition,
                } => {
                    tokio::spawn(run_start_transition(Arc::clone(&self.inner), completion));
                    return wait_for_start(transition).await;
                }
                StartAction::WaitForStart(transition) => {
                    return wait_for_start(transition).await;
                }
                StartAction::WaitForOther(transition) => {
                    let _ = wait_for_transition(transition).await?;
                }
            }
        }
    }

    pub async fn stop_server(&self) -> Result<(), RuntimeError> {
        loop {
            let action = {
                let mut lifecycle = self.inner.lifecycle.lock().await;

                match lifecycle.state {
                    RuntimeState::Stopped => StopAction::Return,
                    RuntimeState::Failed => {
                        lifecycle.state = RuntimeState::Stopped;
                        lifecycle.info = None;
                        lifecycle.shutdown = None;
                        lifecycle.server_task = None;
                        lifecycle.transition = None;
                        StopAction::Return
                    }
                    RuntimeState::Starting => StopAction::WaitForOther(
                        lifecycle
                            .transition
                            .as_ref()
                            .expect("STARTING runtime must retain its transition")
                            .clone(),
                    ),
                    RuntimeState::Stopping => StopAction::WaitForStop(
                        lifecycle
                            .transition
                            .as_ref()
                            .expect("STOPPING runtime must retain its transition")
                            .clone(),
                    ),
                    RuntimeState::Running => {
                        let (completion, transition) = watch::channel(TransitionOutcome::Pending);
                        lifecycle.state = RuntimeState::Stopping;
                        lifecycle.transition = Some(transition.clone());
                        StopAction::Begin {
                            shutdown: lifecycle
                                .shutdown
                                .take()
                                .expect("RUNNING runtime must retain a shutdown sender"),
                            server_task: lifecycle
                                .server_task
                                .take()
                                .expect("RUNNING runtime must retain its server task"),
                            completion,
                            transition,
                        }
                    }
                }
            };

            match action {
                StopAction::Return => return Ok(()),
                StopAction::Begin {
                    shutdown,
                    server_task,
                    completion,
                    transition,
                } => {
                    tokio::spawn(run_stop_transition(
                        Arc::clone(&self.inner),
                        shutdown,
                        server_task,
                        completion,
                    ));
                    return wait_for_stop(transition).await;
                }
                StopAction::WaitForStop(transition) => {
                    return wait_for_stop(transition).await;
                }
                StopAction::WaitForOther(transition) => {
                    let _ = wait_for_transition(transition).await?;
                }
            }
        }
    }

    pub async fn server_status(&self) -> RuntimeState {
        self.inner.lifecycle.lock().await.state
    }
}

enum StartAction {
    Return(ServerInfo),
    Begin {
        completion: watch::Sender<TransitionOutcome>,
        transition: watch::Receiver<TransitionOutcome>,
    },
    WaitForStart(watch::Receiver<TransitionOutcome>),
    WaitForOther(watch::Receiver<TransitionOutcome>),
}

enum StopAction {
    Return,
    Begin {
        shutdown: oneshot::Sender<()>,
        server_task: JoinHandle<io::Result<()>>,
        completion: watch::Sender<TransitionOutcome>,
        transition: watch::Receiver<TransitionOutcome>,
    },
    WaitForStop(watch::Receiver<TransitionOutcome>),
    WaitForOther(watch::Receiver<TransitionOutcome>),
}

async fn run_start_transition(
    inner: Arc<RuntimeInner>,
    completion: watch::Sender<TransitionOutcome>,
) {
    let outcome = match prepare_server(&inner).await {
        Ok(started) => {
            let info = started.info.clone();
            let mut lifecycle = inner.lifecycle.lock().await;
            lifecycle.info = Some(started.info);
            lifecycle.shutdown = Some(started.shutdown);
            lifecycle.server_task = Some(started.server_task);
            lifecycle.transition = None;
            lifecycle.state = RuntimeState::Running;
            TransitionOutcome::Started(Ok(info))
        }
        Err(error) => {
            let mut lifecycle = inner.lifecycle.lock().await;
            lifecycle.info = None;
            lifecycle.shutdown = None;
            lifecycle.server_task = None;
            lifecycle.transition = None;
            lifecycle.state = RuntimeState::Failed;
            TransitionOutcome::Started(Err(error))
        }
    };
    let _ = completion.send(outcome);
}

struct StartedServer {
    info: ServerInfo,
    shutdown: oneshot::Sender<()>,
    server_task: JoinHandle<io::Result<()>>,
}

async fn prepare_server(inner: &RuntimeInner) -> Result<StartedServer, RuntimeError> {
    let router_context = RouterContext {
        token: inner
            .config
            .token_enabled
            .then(|| Uuid::new_v4().to_string()),
    };
    let router = (inner.router_factory)(router_context.clone()).await;
    let configured_port = inner.config.port.unwrap_or(0);
    let bind_address = SocketAddr::from(([127, 0, 0, 1], configured_port));
    let listener = TcpListener::bind(bind_address).await.map_err(|source| {
        if configured_port == 0 {
            RuntimeError::BindFailed(Arc::new(source))
        } else {
            RuntimeError::PortUnavailable {
                port: configured_port,
                source: Arc::new(source),
            }
        }
    })?;
    let local_address = listener
        .local_addr()
        .map_err(|source| RuntimeError::BindFailed(Arc::new(source)))?;
    let info = ServerInfo {
        status: "ready".to_owned(),
        host: LOOPBACK_HOST.to_owned(),
        port: local_address.port(),
        protocol_version: PROTOCOL_VERSION.to_owned(),
        pid: std::process::id(),
        token: router_context.token,
    };
    let (shutdown, shutdown_requested) = oneshot::channel();
    let server_task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_requested.await;
            })
            .await
    });

    Ok(StartedServer {
        info,
        shutdown,
        server_task,
    })
}

async fn run_stop_transition(
    inner: Arc<RuntimeInner>,
    shutdown: oneshot::Sender<()>,
    server_task: JoinHandle<io::Result<()>>,
    completion: watch::Sender<TransitionOutcome>,
) {
    let _ = shutdown.send(());
    let result = match server_task.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(source)) => Err(RuntimeError::ServerFailed(Arc::new(source))),
        Err(error) => Err(RuntimeError::ServerTaskFailed(error.to_string())),
    };

    let mut lifecycle = inner.lifecycle.lock().await;
    lifecycle.info = None;
    lifecycle.transition = None;
    lifecycle.state = if result.is_ok() {
        RuntimeState::Stopped
    } else {
        RuntimeState::Failed
    };
    drop(lifecycle);
    let _ = completion.send(TransitionOutcome::Stopped(result));
}

async fn wait_for_start(
    transition: watch::Receiver<TransitionOutcome>,
) -> Result<ServerInfo, RuntimeError> {
    match wait_for_transition(transition).await? {
        TransitionOutcome::Started(result) => result,
        TransitionOutcome::Stopped(_) | TransitionOutcome::Pending => {
            Err(RuntimeError::ServerTaskFailed(
                "start transition returned an invalid result".to_owned(),
            ))
        }
    }
}

async fn wait_for_stop(transition: watch::Receiver<TransitionOutcome>) -> Result<(), RuntimeError> {
    match wait_for_transition(transition).await? {
        TransitionOutcome::Stopped(result) => result,
        TransitionOutcome::Started(_) | TransitionOutcome::Pending => Err(
            RuntimeError::ServerTaskFailed("stop transition returned an invalid result".to_owned()),
        ),
    }
}

async fn wait_for_transition(
    mut transition: watch::Receiver<TransitionOutcome>,
) -> Result<TransitionOutcome, RuntimeError> {
    loop {
        let outcome = transition.borrow().clone();
        if !matches!(outcome, TransitionOutcome::Pending) {
            return Ok(outcome);
        }

        transition.changed().await.map_err(|_| {
            RuntimeError::ServerTaskFailed(
                "lifecycle transition ended without publishing a result".to_owned(),
            )
        })?;
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
