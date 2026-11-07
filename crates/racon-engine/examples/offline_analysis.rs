//! Offline (no-HTTP) baseline binary for the §47 size report.
//!
//! Runs the same parser/structural/invariant engine as the HTTP service but
//! links none of the axum/tokio-HTTP stack, so its size isolates the cost of
//! the local HTTP/JSON transport added by `racon-cli`.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use racon_engine::AnalysisSession;

fn main() {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("racon-offline-{unique}.mercury"));
    fs::write(
        &path,
        "SPN Model Offline {\n    place A(tokens = 1);\n    place B;\n    timedTransition T(inputs = [A], outputs = [B], delay = 0.0);\n    timedTransition U(inputs = [B], outputs = [A], delay = 0.0);\n}\n",
    )
    .unwrap();

    let session = AnalysisSession::load(&path).expect("model loads");
    let response = session.load_response();
    println!(
        "places={} transitions={} properties={}",
        response.net.places,
        response.net.transitions,
        response.properties.len()
    );

    let _ = fs::remove_file(&path);
}
