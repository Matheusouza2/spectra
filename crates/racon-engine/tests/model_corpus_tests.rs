//! Math corpus (§53): known models with known expected results.

use std::sync::Arc;

use racon_engine::{AnalysisRequest, AnalysisSession, SessionRepository};

fn model_path(name: &str) -> String {
    format!(
        "{}/../../tests/models/{name}.mercury",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn load(name: &str) -> Arc<AnalysisSession> {
    let repo = SessionRepository::default();
    let session = AnalysisSession::load(model_path(name))
        .unwrap_or_else(|error| panic!("failed to load {name}: {error:?}"));
    repo.insert(session)
}

fn explore(name: &str) -> Arc<AnalysisSession> {
    let repo = SessionRepository::default();
    let session = repo.insert(
        AnalysisSession::load(model_path(name))
            .unwrap_or_else(|error| panic!("failed to load {name}: {error:?}")),
    );
    let analysis_id =
        repo.create_state_space_analysis(Arc::clone(&session), AnalysisRequest::default());
    assert!(
        repo.run_state_space_analysis(&analysis_id),
        "state-space job for {name} should run"
    );
    repo.finish_state_space_analysis(&analysis_id);
    session
}

fn status(session: &AnalysisSession, code: &str) -> String {
    session
        .properties()
        .into_iter()
        .find(|property| property.code == code)
        .map(|property| property.status.as_str().to_owned())
        .unwrap_or_else(|| format!("MISSING({code})"))
}

#[test]
fn corpus_structural_classification() {
    let cases = [
        ("ordinary", "ORD", "TRUE"),
        ("nonordinary", "ORD", "FALSE"),
        ("homogeneous", "HOM", "TRUE"),
        ("nonhomogeneous", "HOM", "FALSE"),
        ("pure", "PUR", "TRUE"),
        ("nonpure", "PUR", "FALSE"),
        ("marked_graph", "MG", "TRUE"),
        ("state_machine", "SM", "TRUE"),
        ("free_choice", "FC", "TRUE"),
        ("extended_free_choice", "EFC", "TRUE"),
        ("extended_free_choice", "FC", "FALSE"),
    ];

    for (model, code, expected) in cases {
        let session = load(model);
        assert_eq!(
            status(&session, code),
            expected,
            "model {model} expected {code}={expected}"
        );
    }
}

#[test]
fn corpus_boundedness() {
    assert_eq!(status(&explore("bounded"), "B"), "TRUE");
    assert_eq!(status(&explore("unbounded"), "B"), "FALSE");
}

#[test]
fn corpus_deadlock_and_liveness() {
    assert_eq!(status(&explore("deadlock"), "DSt"), "TRUE");
    assert_eq!(status(&explore("no_deadlock"), "DSt"), "FALSE");
    assert_eq!(status(&explore("live"), "L"), "TRUE");
    assert_eq!(status(&explore("non_live"), "L"), "FALSE");
}

#[test]
fn corpus_reversibility() {
    assert_eq!(status(&explore("reversible"), "REV"), "TRUE");
    assert_eq!(status(&explore("non_reversible"), "REV"), "FALSE");
}

#[test]
fn corpus_symmetry() {
    use racon_reduction::{compute_symmetries, SymmetryOptions};

    let session = load("symmetric");
    let report = compute_symmetries(session.net().as_ref(), &SymmetryOptions::default());
    assert!(
        report.generator_count >= 1,
        "symmetric model should expose at least one automorphism generator"
    );
}

#[test]
fn corpus_high_concurrency_is_bounded_and_live() {
    let session = explore("high_concurrency");
    assert_eq!(status(&session, "B"), "TRUE");
    assert_eq!(status(&session, "L"), "TRUE");
}
