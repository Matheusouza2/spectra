use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::graph::StateSpace;
use racon_engine::AnalysisSession;
use racon_reachability::{ReachabilityEngine, ReachabilityEngineImpl};
use racon_reduction::{
    ReducedExplorationConfig, ReducedReachabilityGraph, ReductionKind, SymmetryOptions,
};
use racon_structural::ExpertSystem;

fn load(name: &str) -> AnalysisSession {
    let path = format!("{}/../../tests/models/{name}.mercury", env!("CARGO_MANIFEST_DIR"));
    AnalysisSession::load(&path).expect("corpus model loads")
}

fn full_graph(session: &AnalysisSession) -> racon_reachability::ReachabilityGraph {
    let context = AnalysisContext::new(AnalysisConfig::default());
    ReachabilityEngineImpl::default()
        .build_complete(session.net().as_ref(), &context)
        .expect("full reachability graph")
}

fn reduced_graph(
    session: &AnalysisSession,
    reductions: &[ReductionKind],
) -> ReducedReachabilityGraph {
    let context = AnalysisContext::new(AnalysisConfig::default());
    ReducedReachabilityGraph::build(
        session.net().as_ref(),
        reductions,
        &SymmetryOptions::default(),
        ReducedExplorationConfig::default(),
        &context,
    )
    .expect("reduced reachability graph")
}

#[test]
fn symmetry_reduction_preserves_dead_states_and_liveness() {
    let session = load("symmetric");
    let net = session.net();
    let full = full_graph(&session);
    let reduced = reduced_graph(&session, &[ReductionKind::Symmetry]);

    assert!(full.is_exact());
    assert!(reduced.is_exact());
    assert!(
        reduced.node_count() <= full.node_count(),
        "reduced exploration must not discover more orbit representatives than the full graph"
    );

    assert_eq!(
        ExpertSystem::has_reachable_dead_state(&full),
        ExpertSystem::has_reachable_dead_state(&reduced),
        "symmetry-reduced graph must agree on reachable dead states"
    );
    assert_eq!(
        ExpertSystem::is_live(&net, &full),
        ExpertSystem::is_live(&net, &reduced),
        "symmetry-reduced graph must agree on liveness"
    );
}

#[test]
fn stubborn_reduction_preserves_dead_states_and_liveness() {
    let session = load("live");
    let net = session.net();
    let full = full_graph(&session);
    let reduced = reduced_graph(&session, &[ReductionKind::Stubborn]);

    assert!(reduced.is_exact());
    assert!(reduced.node_count() <= full.node_count());

    assert_eq!(
        ExpertSystem::has_reachable_dead_state(&full),
        ExpertSystem::has_reachable_dead_state(&reduced)
    );
    assert_eq!(ExpertSystem::is_live(&net, &full), ExpertSystem::is_live(&net, &reduced));
    assert_eq!(
        ExpertSystem::dead_transitions(&net, &full),
        ExpertSystem::dead_transitions(&net, &reduced)
    );
}

#[test]
fn combined_symmetry_and_stubborn_reduction_is_exact_and_agreeing() {
    let session = load("symmetric");
    let net = session.net();
    let full = full_graph(&session);
    let reduced = reduced_graph(
        &session,
        &[ReductionKind::Symmetry, ReductionKind::Stubborn],
    );

    assert_eq!(reduced.variant().as_str(), "symmetry-stubborn");
    assert_eq!(
        ExpertSystem::has_reachable_dead_state(&full),
        ExpertSystem::has_reachable_dead_state(&reduced)
    );
    assert_eq!(ExpertSystem::is_live(&net, &full), ExpertSystem::is_live(&net, &reduced));
}
