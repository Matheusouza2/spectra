use std::path::PathBuf;

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_engine::{AnalysisSession, PropertyStatus};

fn sb_status(path: &str) -> (PropertyStatus, Option<bool>, Option<String>) {
    let session = AnalysisSession::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models").join(path),
    )
    .expect("model loads");
    let context = AnalysisContext::new(AnalysisConfig::default());
    session
        .invariant_cache()
        .get_or_compute(&session.net(), &context)
        .expect("invariants");
    let sb = session
        .properties()
        .into_iter()
        .find(|p| p.code == "SB")
        .expect("SB present");
    (sb.status, sb.value, sb.rule)
}

/// INA decides SB = N for this net (sink place WATER_OUTPUT grows with every
/// WATER_OUT firing); RACoN used to answer UNKNOWN.
#[test]
fn sink_place_net_is_decided_not_structurally_bounded() {
    let (status, value, rule) = sb_status("water_plant_unbounded.mercury");
    assert_eq!(status, PropertyStatus::False);
    assert_eq!(value, Some(false));
    assert_eq!(rule.as_deref(), Some("STRUCTURAL_UNBOUNDED_CERTIFICATE"));
}

#[test]
fn conservative_net_stays_structurally_bounded() {
    let (status, _, _) = sb_status("bounded.mercury");
    assert_eq!(status, PropertyStatus::True);
}
