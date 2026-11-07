pub mod boundedness;
pub mod engine;
pub mod error;
pub mod graph;
pub mod mincov;

pub use boundedness::{decide_boundedness, BoundednessVerdict};
pub use engine::{
    CoverabilityConfig, CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy,
};
pub use error::CoverabilityError;
pub use graph::CoverabilityGraph;
pub use mincov::{MinCov, MinCovConfig, MinimalCoverabilitySet, OmegaValue};

#[cfg(test)]
mod tests {
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};
    use racon_core::StateSpace;

    use crate::{
        CoverabilityConfig, CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy,
    };

    /// Unbounded net: t0 produces 2 tokens to p0 but only consumes 1.
    /// Net growth: each firing adds +1 token to p0, making it unbounded.
    fn unbounded_counter_net() -> racon_core::net::PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 2);
        builder.build(vec![TokenValue::Finite(1)]).unwrap()
    }

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
    fn test_unbounded_detection() {
        let net = unbounded_counter_net();
        let engine = CoverabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine
            .build_complete(&net, CoverabilityStrategy::KarpMiller, &context)
            .unwrap();
        assert_eq!(graph.is_bounded(), Some(false));
        assert!(!graph.is_exact());
        assert!(graph.active_edges_are_consistent());
        let unbounded = graph.detect_unbounded_places();
        assert!(!unbounded.is_empty());
    }

    #[test]
    fn test_bounded_net_no_omega() {
        let net = simple_cycle_net();
        let engine = CoverabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine
            .build_complete(&net, CoverabilityStrategy::KarpMiller, &context)
            .unwrap();
        assert_eq!(
            graph.is_bounded(),
            Some(true),
            "Karp-Miller is the only supported coverability strategy"
        );
        assert_eq!(graph.stats.omega_nodes, 0);
        assert!(graph.active_edges_are_consistent());
    }

    #[test]
    fn test_karp_miller_unbounded() {
        let net = unbounded_counter_net();
        let engine = CoverabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine
            .build_complete(&net, CoverabilityStrategy::KarpMiller, &context)
            .unwrap();
        assert_eq!(graph.is_bounded(), Some(false));
    }

    #[test]
    fn finkel_prunes_strictly_dominated_coverability_nodes() {
        let net = unbounded_counter_net();
        let engine = CoverabilityEngineImpl::default();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let karp = engine
            .build_complete(&net, CoverabilityStrategy::KarpMiller, &context)
            .unwrap();
        let finkel = engine
            .build_complete(
                &net,
                CoverabilityStrategy::FinkelMinimalCoverability,
                &context,
            )
            .unwrap();
        assert_eq!(finkel.is_bounded(), Some(false));
        assert!(finkel.stats.pruned_nodes > 0);
        let active = finkel.nodes.iter().filter(|node| node.is_active).count();
        assert!(active < karp.nodes.len());
        assert!(finkel.active_edges_are_consistent());
    }

    #[test]
    fn node_limit_allows_existing_successor_at_boundary() {
        let engine = CoverabilityEngineImpl {
            config: CoverabilityConfig {
                max_nodes: 3,
                max_edges: usize::MAX,
            },
        };
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = engine
            .build_complete(
                &simple_cycle_net(),
                CoverabilityStrategy::KarpMiller,
                &context,
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 3);
    }
}
