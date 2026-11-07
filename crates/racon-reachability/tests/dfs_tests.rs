//! DFS must build exactly the graph BFS builds (same markings, same labelled
//! edges); only node numbering and the meaning of `depth` differ.

use std::collections::HashSet;

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::{
    ArcEndpoint, PetriNetBuilder, PlaceId, TokenValue, TransitionKind, TransitionId,
};
use racon_reachability::{ReachabilityEngine, ReachabilityEngineImpl, ReachabilityError, ReachabilityGraph};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, b: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % b
    }
}

fn edge_set(g: &ReachabilityGraph) -> HashSet<(Vec<TokenValue>, u32, Vec<TokenValue>)> {
    g.edges
        .iter()
        .map(|e| {
            (
                g.nodes[e.source.0 as usize].marking.tokens.clone(),
                e.transition.0,
                g.nodes[e.target.0 as usize].marking.tokens.clone(),
            )
        })
        .collect()
}

fn markings(g: &ReachabilityGraph) -> HashSet<Vec<TokenValue>> {
    g.nodes.iter().map(|n| n.marking.tokens.clone()).collect()
}

#[test]
fn dfs_and_bfs_build_the_same_graph_including_extended_nets() {
    let mut rng = Lcg(42);
    let mut compared = 0;
    for _ in 0..300 {
        let places = 2 + rng.next(3) as usize;
        let transitions = 2 + rng.next(3) as usize;
        let mut b = PetriNetBuilder::default();
        for p in 0..places {
            b.add_place(format!("p{p}"), None);
        }
        for t in 0..transitions {
            let tid = b.add_transition(format!("t{t}"), None);
            let mut touched = false;
            for p in 0..places {
                let pid = PlaceId(p as u32);
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Place(pid), ArcEndpoint::Transition(tid), 1 + rng.next(2) as u32);
                    touched = true;
                }
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(pid), 1 + rng.next(2) as u32);
                    touched = true;
                }
                if rng.next(5) == 0 {
                    b.add_inhibitor_arc(pid, tid, 1 + rng.next(3));
                    touched = true;
                }
            }
            if !touched {
                b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(0)), 1);
            }
            if rng.next(4) == 0 {
                b.set_transition_kind(tid, TransitionKind::Immediate);
            }
        }
        // make sure every place is incident to something
        for p in 0..places {
            b.add_inhibitor_arc(PlaceId(p as u32), TransitionId(0), 1_000_000);
        }
        let Ok(net) = b.build((0..places).map(|_| TokenValue::Finite(rng.next(3))).collect()) else {
            continue;
        };
        let context = AnalysisContext::new(AnalysisConfig::default());
        let mut engine = ReachabilityEngineImpl::default();
        engine.config.max_nodes = 300;
        let bfs = engine.build_complete(&net, &context);
        let dfs = engine.build_complete_dfs(&net, &context);
        match (bfs, dfs) {
            (Ok(bfs), Ok(dfs)) => {
                assert_eq!(markings(&bfs), markings(&dfs));
                assert_eq!(edge_set(&bfs), edge_set(&dfs));
                assert_eq!(bfs.edges.len(), dfs.edges.len());
                assert!(dfs.complete);
                compared += 1;
            }
            (Err(ReachabilityError::NodeLimitExceeded { .. }), Err(ReachabilityError::NodeLimitExceeded { .. })) => {}
            (a, b) => panic!("BFS/DFS disagree on termination: {:?} vs {:?}", a.is_ok(), b.is_ok()),
        }
    }
    assert!(compared >= 100, "only {compared} comparable nets");
}
