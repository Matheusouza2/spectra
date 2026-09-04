use std::env;
use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;

use racon_api::{router, ApiState};
use racon_engine::SessionRepository;
use racon_runtime::{Runtime, RuntimeConfig, RuntimeError, ServerInfo};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StartServerArgs {
    json: bool,
    port: Option<u16>,
}

#[derive(Debug)]
enum CliError {
    InvalidArgument(String),
    Runtime(RuntimeError),
    Signal(io::Error),
    Output(io::Error),
}

impl CliError {
    const fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "INVALID_ARGUMENT",
            Self::Runtime(error) => error.code(),
            Self::Signal(_) | Self::Output(_) => "INTERNAL_ERROR",
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message) => formatter.write_str(message),
            Self::Runtime(error) => error.fmt(formatter),
            Self::Signal(error) => write!(formatter, "failed to wait for shutdown signal: {error}"),
            Self::Output(error) => write!(formatter, "failed to write CLI output: {error}"),
        }
    }
}

impl From<RuntimeError> for CliError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let json_requested = arguments.iter().any(|argument| argument == "--json");

    match run(arguments).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json_requested {
                let payload = json!({
                    "error": {
                        "code": error.code(),
                        "message": error.to_string(),
                    }
                });
                if let Err(output_error) = write_json_line(&payload) {
                    eprintln!("failed to write structured CLI error: {output_error}");
                }
            }
            eprintln!("failed to start server: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(arguments: Vec<String>) -> Result<(), CliError> {
    let arguments = parse_arguments(arguments)?;
    let runtime = api_runtime(RuntimeConfig {
        port: arguments.port,
        token_enabled: true,
    });
    let server = runtime.start_server().await?;

    if arguments.json {
        write_json_line(&server_info_json(&server)).map_err(CliError::Output)?;
    } else {
        eprintln!(
            "RACoN server ready at http://{}:{} (protocol {})",
            server.host, server.port, server.protocol_version
        );
    }
    eprintln!("server started; press Ctrl-C to stop");

    if let Err(error) = tokio::signal::ctrl_c().await {
        let _ = runtime.stop_server().await;
        return Err(CliError::Signal(error));
    }

    eprintln!("shutdown requested");
    runtime.stop_server().await?;
    eprintln!("server stopped");
    Ok(())
}

fn api_runtime(config: RuntimeConfig) -> Runtime {
    let sessions = Arc::new(SessionRepository::default());
    Runtime::with_router(config, move |context| {
        let state = ApiState::new(Arc::clone(&sessions), context);
        async move { router(state) }
    })
}

fn parse_arguments(arguments: Vec<String>) -> Result<StartServerArgs, CliError> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        Some("start-server") => {}
        Some(command) => {
            return Err(CliError::InvalidArgument(format!(
                "unknown command '{command}'; expected 'start-server'"
            )));
        }
        None => {
            return Err(CliError::InvalidArgument(
                "missing command; expected 'start-server'".to_owned(),
            ));
        }
    }

    let mut parsed = StartServerArgs {
        json: false,
        port: None,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--json" => parsed.json = true,
            "--port" => {
                let value = arguments.next().ok_or_else(|| {
                    CliError::InvalidArgument("--port requires a value from 0 to 65535".to_owned())
                })?;
                let port = value.parse::<u16>().map_err(|_| {
                    CliError::InvalidArgument(format!(
                        "invalid port '{value}'; expected a value from 0 to 65535"
                    ))
                })?;
                parsed.port = Some(port);
            }
            unknown => {
                return Err(CliError::InvalidArgument(format!(
                    "unknown option '{unknown}'"
                )));
            }
        }
    }

    Ok(parsed)
}

fn server_info_json(server: &ServerInfo) -> Value {
    let mut fields = Map::new();
    fields.insert("status".to_owned(), Value::String(server.status.clone()));
    fields.insert("host".to_owned(), Value::String(server.host.clone()));
    fields.insert("port".to_owned(), Value::from(server.port));
    fields.insert(
        "protocol_version".to_owned(),
        Value::String(server.protocol_version.clone()),
    );
    fields.insert("pid".to_owned(), Value::from(server.pid));
    if let Some(token) = &server.token {
        fields.insert("token".to_owned(), Value::String(token.clone()));
    }
    Value::Object(fields)
}

fn write_json_line(value: &Value) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}
