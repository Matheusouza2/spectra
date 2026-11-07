//! Dead transitions through stubborn sets (Schmidt 1999, §11, Thm 6).
//!
//! A transition `t` is *dead* iff no reachable marking enables it. At every
//! marking the search fires only the enabled transitions of a weakly stubborn
//! superset of `{t}` (the attractor set is `{t}` itself). Then:
//!
//! * `t` is dead in the reduced graph **iff** it is dead in the full graph;
//! * if it is not dead, a **shortest** firing sequence from the initial marking
//!   to a marking enabling `t` is preserved (the search is breadth-first, so
//!   the returned witness is such a sequence).
//!
//! No ignoring proviso is needed: while `t` is disabled the closure contains
//! the producers of a scapegoat input place, so every path to a marking that
//! enables `t` fires a member of the set first. Exhausting the reduced graph
//! proves deadness; exceeding the node budget is `LimitExceeded` (UNKNOWN),
//! never a verdict. Only plain P/T nets are supported.

use std::collections::{HashMap, VecDeque};

use racon_core::execution::AnalysisContext;
use racon_core::marking::Marking;
use racon_core::net::{PetriNet, TransitionId};

use crate::error::ReductionError;
use crate::goal_search::{close_stubborn, NetIndex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadVerdict {
    /// No reachable marking enables the transition.
    Dead,
    /// Shortest firing sequence from the initial marking to a marking that
    /// enables the transition (the transition itself is not part of `path`).
    Live { path: Vec<TransitionId> },
}

#[derive(Debug, Clone, Copy)]
pub struct DeadSearchConfig {
    pub max_nodes: usize,
    /// When false every enabled transition is fired (baseline).
    pub use_stubborn: bool,
}

impl Default for DeadSearchConfig {
    fn default() -> Self {
        Self {
            max_nodes: 5_000_000,
            use_stubborn: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadSearchReport {
    pub verdict: DeadVerdict,
    pub visited: usize,
}

/// Decide whether `transition` is dead (Thm 6).
pub fn decide_dead_transition(
    net: &PetriNet,
    transition: TransitionId,
    config: DeadSearchConfig,
    context: &AnalysisContext,
) -> Result<DeadSearchReport, ReductionError> {
    if net.is_extended() {
        return Err(ReductionError::NotApplicable {
            name: "STUBBORN_DEAD_TRANSITIONS",
            reason: "stubborn sets are proved for plain P/T nets; nets with inhibitor arcs or immediate transitions are not supported",
        });
    }
    if transition.0 as usize >= net.transition_count() {
        return Err(ReductionError::NotApplicable {
            name: "STUBBORN_DEAD_TRANSITIONS",
            reason: "transition index out of range",
        });
    }
    let index = NetIndex::new(net);

    let initial = net.initial_marking().clone();
    let mut markings = vec![initial.clone()];
    let mut parents: Vec<Option<(usize, TransitionId)>> = vec![None];
    let mut seen: HashMap<Marking, usize> = HashMap::from([(initial, 0)]);
    let mut queue = VecDeque::from([0usize]);

    while let Some(node) = queue.pop_front() {
        context
            .check_cancellation()
            .map_err(|_| ReductionError::Cancelled)?;
        let marking = markings[node].clone();

        if net.is_enabled(transition, &marking) {
            let mut path = Vec::new();
            let mut cursor = node;
            while let Some((parent, fired)) = parents[cursor] {
                path.push(fired);
                cursor = parent;
            }
            path.reverse();
            return Ok(DeadSearchReport {
                verdict: DeadVerdict::Live { path },
                visited: markings.len(),
            });
        }

        let stubborn = if config.use_stubborn {
            let mut included = vec![false; net.transition_count()];
            included[transition.0 as usize] = true;
            let mut stack = vec![transition.0 as usize];
            close_stubborn(net, &index, &marking, &mut included, &mut stack);
            Some(included)
        } else {
            None
        };

        for candidate in net.transitions() {
            let id = candidate.id;
            if stubborn.as_ref().is_some_and(|set| !set[id.0 as usize]) {
                continue;
            }
            if !net.is_enabled(id, &marking) {
                continue;
            }
            let successor = net.fire(id, &marking);
            if seen.contains_key(&successor) {
                continue;
            }
            if markings.len() >= config.max_nodes {
                return Err(ReductionError::LimitExceeded {
                    limit: config.max_nodes,
                });
            }
            let created = markings.len();
            markings.push(successor.clone());
            parents.push(Some((node, id)));
            seen.insert(successor, created);
            queue.push_back(created);
        }
    }

    Ok(DeadSearchReport {
        verdict: DeadVerdict::Dead,
        visited: markings.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::AnalysisConfig;
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % bound
        }
    }

    fn random_net(rng: &mut Lcg) -> PetriNet {
        let places = 2 + rng.next(4) as usize;
        let transitions = 2 + rng.next(5) as usize;
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..places).map(|i| b.add_place(format!("p{i}"), None)).collect();
        for i in 0..transitions {
            let t = b.add_transition(format!("t{i}"), None);
            let mut has_input = false;
            for &p in &ps {
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1 + rng.next(2) as u32);
                    has_input = true;
                }
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1 + rng.next(2) as u32);
                }
            }
            if !has_input {
                b.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(t), 1);
            }
        }
        let tokens = (0..places).map(|_| TokenValue::Finite(rng.next(3))).collect();
        b.build(tokens).unwrap()
    }

    #[test]
    fn dead_verdicts_and_shortest_paths_match_the_full_search() {
        let context = AnalysisContext::new(AnalysisConfig::default());
        let mut rng = Lcg(0xDEAD);
        let iterations: usize = std::env::var("RACON_FUZZ_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(400);
        let (mut compared, mut dead, mut live, mut smaller) = (0usize, 0usize, 0usize, 0usize);
        for _ in 0..iterations {
            let net = random_net(&mut rng);
            for transition in net.transitions() {
                let budget = DeadSearchConfig { max_nodes: 2_000, use_stubborn: false };
                let Ok(full) = decide_dead_transition(&net, transition.id, budget, &context) else {
                    continue;
                };
                let reduced = decide_dead_transition(
                    &net,
                    transition.id,
                    DeadSearchConfig { use_stubborn: true, ..budget },
                    &context,
                )
                .expect("the reduced graph never exceeds the full one");
                compared += 1;
                match (&full.verdict, &reduced.verdict) {
                    (DeadVerdict::Dead, DeadVerdict::Dead) => dead += 1,
                    (DeadVerdict::Live { path: a }, DeadVerdict::Live { path: b }) => {
                        live += 1;
                        // Thm 6: a shortest enabling sequence is preserved.
                        assert_eq!(a.len(), b.len(), "{net:?} t{}", transition.id.0);
                        let mut m = net.initial_marking().clone();
                        for &t in b {
                            assert!(net.is_enabled(t, &m));
                            m = net.fire(t, &m);
                        }
                        assert!(net.is_enabled(transition.id, &m));
                    }
                    other => panic!("verdict mismatch {other:?}\n{net:?} t{}", transition.id.0),
                }
                if reduced.visited < full.visited {
                    smaller += 1;
                }
            }
        }
        assert!(compared > iterations, "too few bounded samples: {compared}");
        assert!(dead > 20 && live > 20, "dead={dead} live={live}");
        assert!(smaller > 0, "stubborn never reduced the search");
    }

    #[test]
    fn dead_transition_in_a_product_net_is_found_without_interleaving() {
        // A pipeline that can never feed t_dead, next to a busy independent loop.
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..4).map(|i| b.add_place(format!("p{i}"), None)).collect();
        let busy = b.add_transition("busy".into(), None);
        let dead = b.add_transition("dead".into(), None);
        b.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(busy), 1);
        b.add_arc(ArcEndpoint::Transition(busy), ArcEndpoint::Place(ps[1]), 1);
        b.add_arc(ArcEndpoint::Place(ps[2]), ArcEndpoint::Transition(dead), 1);
        b.add_arc(ArcEndpoint::Transition(dead), ArcEndpoint::Place(ps[3]), 1);
        let net = b
            .build(vec![TokenValue::Finite(5), TokenValue::Finite(0), TokenValue::Finite(0), TokenValue::Finite(0)])
            .unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let run = |use_stubborn| {
            decide_dead_transition(&net, dead, DeadSearchConfig { use_stubborn, ..Default::default() }, &context).unwrap()
        };
        let (reduced, full) = (run(true), run(false));
        assert_eq!(reduced.verdict, DeadVerdict::Dead);
        assert_eq!(full.verdict, DeadVerdict::Dead);
        assert_eq!(reduced.visited, 1);
        assert_eq!(full.visited, 6);
    }

    #[test]
    fn extended_nets_and_bad_indices_are_rejected() {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        b.add_inhibitor_arc(p, t, 2);
        let net = b.build(vec![TokenValue::Finite(0)]).unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let err = decide_dead_transition(&net, t, DeadSearchConfig::default(), &context).unwrap_err();
        assert_eq!(err.code(), "REDUCTION_NOT_APPLICABLE");
    }
}
