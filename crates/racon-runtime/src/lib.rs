//! Runtime lifecycle for RACoN services.

mod lifecycle;

pub use lifecycle::{
    server_status, start_server, stop_server, Runtime, RuntimeConfig, RuntimeError, RuntimeState,
    ServerInfo,
};
