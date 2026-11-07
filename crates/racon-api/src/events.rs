use racon_engine::AnalysisSnapshot;
use serde_json::{json, Value};

pub const HEARTBEAT_INTERVAL_MS: u64 = 15_000;

pub fn event_name(snapshot: &AnalysisSnapshot) -> &'static str {
    match snapshot.status() {
        "COMPLETED" => "completed",
        "CANCELLED" => "cancelled",
        "FAILED" => "failed",
        _ => "progress",
    }
}

pub fn serialize(snapshot: &AnalysisSnapshot, name: &str) -> Value {
    json!({
        "type": name,
        "analysis_id": snapshot.analysis_id(),
        "data": {
            "status": snapshot.status(),
            "phase": snapshot.phase(),
            "discovered": snapshot.states_discovered(),
            "processed": snapshot.states_processed(),
            "edges": snapshot.edges_discovered(),
            "frontier": snapshot.frontier_size(),
            "omega": snapshot.omega_nodes(),
            "elapsed_ms": snapshot.elapsed_ms(),
            "memory": Value::Null,
            "error": snapshot.error(),
        }
    })
}
