//! JNI contract (spec §5): the bridge exposes lifecycle/bootstrap only.
//!
//! Analysis always travels over HTTP/JSON, so the JNI surface must not grow
//! mathematical entry points.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn exports_only_the_three_lifecycle_entry_points() {
    let source = fs::read_to_string(repo_root().join("crates/racon-jni/src/lib.rs"))
        .expect("racon-jni lib.rs must exist");

    for symbol in [
        "Java_racon_RaconRuntime_startServer",
        "Java_racon_RaconRuntime_stopServer",
        "Java_racon_RaconRuntime_serverStatus",
    ] {
        assert!(source.contains(symbol), "missing JNI export {symbol}");
    }

    for forbidden in [
        "isOrdinary",
        "isBounded",
        "isLive",
        "getDeadlocks",
        "compute_p_semiflows",
        "ReachabilityEngine",
        "CoverabilityEngine",
        "InvariantEngine",
        "PetriNet",
    ] {
        assert!(
            !source.contains(forbidden),
            "JNI bootstrap must not expose analysis symbol `{forbidden}`"
        );
    }
}

#[test]
fn java_runtime_declares_only_lifecycle_natives() {
    let source = fs::read_to_string(repo_root().join("examples/java/racon/RaconRuntime.java"))
        .expect("RaconRuntime.java must exist");

    let native_lines: Vec<&str> = source.lines().filter(|line| line.contains("native")).collect();
    assert_eq!(native_lines.len(), 3, "expected exactly three native methods");
    for method in ["startServer", "stopServer", "serverStatus"] {
        assert!(
            native_lines.iter().any(|line| line.contains(method)),
            "missing native {method}"
        );
    }
    for forbidden in ["isBounded", "isLive", "invariant", "stateSpace", "deadlock"] {
        assert!(
            !source.contains(forbidden),
            "RaconRuntime must stay lifecycle-only, found `{forbidden}`"
        );
    }
}
