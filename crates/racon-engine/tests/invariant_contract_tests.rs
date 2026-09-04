use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_engine::{
    AnalysisSession, InvariantInteger, InvariantKind, InvariantQuery, InvariantQueryError,
    PropertyStatus,
};

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
            "racon-engine-invariants-{}-{unique}.mercury",
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

fn cycle_model() -> TemporaryMercuryFile {
    TemporaryMercuryFile::new(
        r#"
        SPN Model NamedCycle {
            place Reservoir(tokens = 1);
            place Processing;
            timedTransition Start(inputs = [Reservoir], outputs = [Processing], delay = 0.0);
            timedTransition Finish(inputs = [Processing], outputs = [Reservoir], delay = 0.0);
        }

        main {
        }
        "#,
    )
}

fn context() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

#[test]
fn first_query_returns_named_dtos_and_second_query_reuses_cached_analysis() {
    let model = cycle_model();
    let session = AnalysisSession::load(model.path()).expect("session should load");
    let query = InvariantQuery {
        kind: InvariantKind::All,
        minimal: Some(true),
        ..InvariantQuery::default()
    };

    let first = session
        .query_invariants(&query, &context())
        .expect("first invariant query should succeed");
    let first_cached = session
        .invariant_cache()
        .cached()
        .expect("first query should populate the cache");

    assert_eq!(first.len(), 2);

    let place = first
        .iter()
        .find(|invariant| invariant.invariant_type == InvariantKind::Place)
        .expect("PLACE invariant should be returned");
    assert!(place.minimal);
    assert_eq!(
        place.vector,
        vec![InvariantInteger::Number(1), InvariantInteger::Number(1)]
    );
    assert_eq!(place.support, ["Reservoir", "Processing"]);
    assert_eq!(place.terms.len(), 2);
    assert_eq!(place.terms[0].node, "Reservoir");
    assert_eq!(place.terms[0].coefficient, InvariantInteger::Number(1));

    let transition = first
        .iter()
        .find(|invariant| invariant.invariant_type == InvariantKind::Transition)
        .expect("TRANSITION invariant should be returned");
    assert_eq!(transition.support, ["Start", "Finish"]);
    assert_eq!(transition.terms[1].node, "Finish");

    let second = session
        .query_invariants(&query, &context())
        .expect("second invariant query should succeed");
    let second_cached = session
        .invariant_cache()
        .cached()
        .expect("second query should retain the cache");

    assert_eq!(second, first);
    assert!(Arc::ptr_eq(&first_cached, &second_cached));
    assert_eq!(
        session
            .properties()
            .into_iter()
            .find(|property| property.code == "CSV")
            .expect("CSV property should exist")
            .status,
        PropertyStatus::True
    );
}

#[test]
fn place_transition_and_minimal_filters_are_applied_to_cached_semiflows() {
    let model = cycle_model();
    let session = AnalysisSession::load(model.path()).expect("session should load");

    let places = session
        .query_invariants(
            &InvariantQuery {
                kind: InvariantKind::Place,
                minimal: Some(true),
                ..InvariantQuery::default()
            },
            &context(),
        )
        .expect("PLACE query should succeed");
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].invariant_type, InvariantKind::Place);

    let transitions = session
        .query_invariants(
            &InvariantQuery {
                kind: InvariantKind::Transition,
                minimal: Some(true),
                ..InvariantQuery::default()
            },
            &context(),
        )
        .expect("TRANSITION query should succeed");
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].invariant_type, InvariantKind::Transition);

    let non_minimal = session
        .query_invariants(
            &InvariantQuery {
                kind: InvariantKind::All,
                minimal: Some(false),
                ..InvariantQuery::default()
            },
            &context(),
        )
        .expect("minimal=false query should be classified by the cached data");
    assert!(non_minimal.is_empty());
}

#[test]
fn sub_and_sur_filters_are_explicitly_not_implemented() {
    let model = cycle_model();
    let session = AnalysisSession::load(model.path()).expect("session should load");

    for query in [
        InvariantQuery {
            sub: true,
            ..InvariantQuery::default()
        },
        InvariantQuery {
            sur: true,
            ..InvariantQuery::default()
        },
    ] {
        let error = session
            .query_invariants(&query, &context())
            .expect_err("unsupported filter should fail explicitly");
        assert!(matches!(error, InvariantQueryError::NotImplemented { .. }));
        assert_eq!(error.code(), "NOT_IMPLEMENTED");
    }

    assert!(!session.invariants_cached());
}

#[test]
fn invariant_integer_preserves_values_larger_than_i64_as_decimal_text() {
    let value = InvariantInteger::from_decimal("9223372036854775808")
        .expect("valid integer text should be accepted");

    assert_eq!(
        value,
        InvariantInteger::Decimal("9223372036854775808".to_owned())
    );
    assert_eq!(value.as_decimal(), "9223372036854775808");
}

#[test]
fn invariant_integer_preserves_the_decimal_form_of_regular_json_numbers() {
    let value = InvariantInteger::from_decimal("42").expect("i64 value should be accepted");

    assert_eq!(value, InvariantInteger::Number(42));
    assert_eq!(value.as_decimal(), "42");
}
