use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder};
use racon_core::StateSpace;
use racon_reachability::{ReachabilityConfig, ReachabilityEngine, ReachabilityEngineImpl};

fn cycle_net() -> racon_core::PetriNet {
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    let t1 = b.add_transition("t1".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .unwrap()
}

#[test]
fn edge_limit_is_enforced_during_expansion() {
    let net = cycle_net();
    let engine = ReachabilityEngineImpl {
        config: ReachabilityConfig {
            max_nodes: 100,
            max_edges: 1,
            levels_per_expansion: 5,
        },
    };
    let context = AnalysisContext::new(AnalysisConfig::default());
    let error = engine
        .build_complete(&net, &context)
        .expect_err("edge limit must stop graph");
    assert!(error.to_string().contains("Edge limit exceeded"));
}

#[test]
fn complete_graph_at_node_limit_is_not_reported_as_incomplete() {
    let net = cycle_net();
    let engine = ReachabilityEngineImpl {
        config: ReachabilityConfig {
            max_nodes: 2,
            max_edges: 10,
            levels_per_expansion: 5,
        },
    };
    let context = AnalysisContext::new(AnalysisConfig::default());
    let graph = engine
        .build_complete(&net, &context)
        .expect("a graph completed exactly at max_nodes is complete");
    assert_eq!(graph.node_count(), 2);
}
