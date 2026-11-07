pub mod engine;
pub mod error;
pub mod graph;
pub mod verification;

pub use engine::{ExpansionResult, ReachabilityConfig, ReachabilityEngine, ReachabilityEngineImpl};
pub use error::ReachabilityError;
pub use graph::ReachabilityGraph;

#[cfg(test)]
mod tests {
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};
    use racon_core::StateSpace;

    use crate::{ReachabilityEngine, ReachabilityEngineImpl};

    fn simple_cycle_net() -> racon_core::net::PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        let t1 = builder.add_transition("t1".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
        builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap()
    }

    #[test]
    fn test_simple_reachability() {
        let net = simple_cycle_net();
        let engine = ReachabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine.build_complete(&net, &context).unwrap();
        assert!(graph.is_exact());
        assert!(graph.nodes.len() >= 2);
        assert!(graph.verify_consistency().is_ok());
        assert!(graph.verify_firing_semantics().is_ok());
    }

    #[test]
    fn test_marking_deduplication() {
        let net = simple_cycle_net();
        let engine = ReachabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine.build_complete(&net, &context).unwrap();
        let unique_markings: std::collections::HashSet<_> =
            graph.nodes.iter().map(|n| n.marking.clone()).collect();
        assert_eq!(unique_markings.len(), graph.nodes.len());
    }

    #[test]
    fn test_bfs_path() {
        let net = simple_cycle_net();
        let engine = ReachabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine.build_complete(&net, &context).unwrap();
        if graph.nodes.len() > 1 {
            let path = graph.bfs_path(0, graph.nodes.len() - 1);
            assert!(path.is_some());
        }
    }
}
