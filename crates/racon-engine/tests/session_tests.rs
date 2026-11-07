use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use racon_engine::{AnalysisSession, PropertySource, PropertyStatus, SessionRepository};

struct TemporaryMercuryFile {
    path: PathBuf,
}

impl TemporaryMercuryFile {
    fn new(contents: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "racon-engine-session-{}-{unique}.mercury",
            std::process::id()
        ));
        fs::write(&path, contents).expect("temporary MercuryScript should be written");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryMercuryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn load_creates_independent_sessions_with_structural_properties() {
    let model = TemporaryMercuryFile::new(
        r#"
        SPN Model Cycle {
            place p0(tokens = 1);
            place p1;
            timedTransition t0(inputs = [p0], outputs = [p1], delay = 0.0);
            timedTransition t1(inputs = [p1], outputs = [p0], delay = 0.0);
        }

        main {
        }
        "#,
    );

    let first = AnalysisSession::load(model.path()).expect("first session should load");
    let second = AnalysisSession::load(model.path()).expect("second session should load");

    assert!(!first.session_id().is_empty());
    assert!(!second.session_id().is_empty());
    assert_ne!(first.session_id(), second.session_id());

    let response = first.load_response();
    assert_eq!(response.session_id, first.session_id());
    assert_eq!(response.source.source_type, "mercury_script");
    assert_eq!(response.source.path, model.path().to_string_lossy());
    assert_eq!(response.net.places, 2);
    assert_eq!(response.net.transitions, 2);
    assert_eq!(response.net.arcs, 4);

    let property_codes: Vec<_> = response
        .properties
        .iter()
        .map(|property| property.code.as_str())
        .collect();
    assert_eq!(
        property_codes,
        [
            "ORD", "HOM", "NBM", "PUR", "CSV", "SCF", "CON", "SC", "Ft0", "tF0", "Fp0", "pF0",
            "MG", "SM", "FC", "EFC", "ES", "DTP", "SMC", "SMD", "SMA", "CPI", "CTI", "SB", "B",
            "REV", "DSt", "BSt", "DTr", "DCF", "L", "LV", "L&S", "LL", "WL", "CL",
        ]
    );

    for code in ["ORD", "PUR"] {
        let property = response
            .properties
            .iter()
            .find(|property| property.code == code)
            .unwrap_or_else(|| panic!("missing {code} property"));
        assert_eq!(property.status, PropertyStatus::True);
        assert_eq!(property.source, PropertySource::Structural);
        assert_eq!(property.value, Some(true));
    }

    let csv = response
        .properties
        .iter()
        .find(|property| property.code == "CSV")
        .expect("missing CSV property");
    assert_eq!(csv.status, PropertyStatus::NotComputed);
    assert_eq!(csv.source, PropertySource::Algebraic);
    assert_eq!(csv.value, None);

    let strongly_connected = response
        .properties
        .iter()
        .find(|property| property.code == "SC")
        .expect("missing SC property");
    assert_eq!(strongly_connected.status, PropertyStatus::NotImplemented);
    assert_eq!(strongly_connected.source, PropertySource::Structural);
    assert_eq!(strongly_connected.value, None);

    let first_net = first.net();
    assert!(Arc::ptr_eq(&first_net, &first.net()));
    assert!(!Arc::ptr_eq(&first_net, &second.net()));
    assert!(!first.invariants_cached());
    assert!(!second.invariants_cached());

    let repository = SessionRepository::default();
    let first = repository.insert(first);
    assert!(Arc::ptr_eq(
        &first,
        &repository
            .get(first.session_id())
            .expect("inserted session should be retrievable")
    ));
    assert!(repository.get(second.session_id()).is_none());
}
