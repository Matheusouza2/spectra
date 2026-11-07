use crate::topology::StructuralProperties;
use num_bigint::BigUint;
use racon_core::graph::{NodeId, StateSpace};
use racon_core::net::{PetriNet, PlaceId, TransitionId};
use racon_invariants::{types::InvariantAnalysisResult, verify_structural_boundedness_certificate};
use std::collections::{HashSet, VecDeque};

/// Behavioral properties derived from structural + invariant + state-space data.
#[derive(Debug, Clone)]
pub struct BehavioralProperties {
    pub is_bounded: Option<bool>,
    pub is_structurally_bounded: bool,
    pub is_reversible: Option<bool>,
    pub is_deadlock_free: Option<bool>,
    pub liveness_classification: LivenessLevel,
    pub dead_transitions: Vec<TransitionId>,
    pub is_persistent: Option<bool>,
    pub is_gfc: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivenessLevel {
    L0,
    L1,
    L2,
    L3,
    L4,
    Unknown,
}

/// Expert system that applies Petri net theorems to derive behavioral
/// properties without full state-space exploration where possible.
pub struct ExpertSystem;

impl ExpertSystem {
    pub fn is_persistent(net: &PetriNet, ss: &dyn StateSpace) -> bool {
        (0..ss.node_count()).all(|i| {
            let m = ss.marking(NodeId(i as u32));
            let enabled: Vec<_> = net
                .transitions()
                .iter()
                .map(|t| t.id)
                .filter(|&t| net.is_enabled(t, m))
                .collect();
            enabled.iter().enumerate().all(|(a, &first)| {
                enabled.iter().enumerate().all(|(b, &second)| {
                    a == b
                        || net
                            .try_fire(first, m)
                            .map(|next| net.is_enabled(second, &next))
                            .unwrap_or(false)
                })
            })
        })
    }

    /// Dynamic conflict freedom: in no reachable marking does firing one enabled
    /// transition disable another. Needs an exact state space.
    pub fn is_dynamic_conflict_free(net: &PetriNet, ss: &dyn StateSpace) -> bool {
        (0..ss.node_count()).all(|index| {
            let marking = ss.marking(NodeId(index as u32));
            let fireable = net.fireable_transitions(marking);
            fireable.iter().enumerate().all(|(position, &first)| {
                fireable
                    .iter()
                    .skip(position + 1)
                    .all(|&second| !net.is_dynamic_conflict(first, second, marking))
            })
        })
    }

    pub fn is_gfc(net: &PetriNet, ss: &dyn StateSpace) -> bool {
        (0..ss.node_count()).all(|i| {
            let m = ss.marking(NodeId(i as u32));
            net.transitions().iter().enumerate().all(|(a, x)| {
                net.transitions().iter().skip(a + 1).all(|y| {
                    let shared = net
                        .preset(x.id)
                        .iter()
                        .any(|(p, _)| net.preset(y.id).iter().any(|(q, _)| p == q));
                    !shared || net.is_enabled(x.id, m) == net.is_enabled(y.id, m)
                })
            })
        })
    }

    pub fn deadlock_trap_property(net: &PetriNet) -> Option<bool> {
        let n = net.place_count();
        if n > 20 {
            return None;
        }
        for mask in 1u64..(1u64 << n) {
            let trap = |subset: u64| {
                net.transitions().iter().all(|t| {
                    let consumes = net
                        .preset(t.id)
                        .iter()
                        .any(|(p, _)| subset & (1u64 << p.0) != 0);
                    !consumes
                        || net
                            .postset(t.id)
                            .iter()
                            .any(|(p, _)| subset & (1u64 << p.0) != 0)
                })
            };
            let union = (1u64..(1u64 << n))
                .filter(|s| s & !mask == 0 && trap(*s))
                .fold(0, |a, s| a | s);
            let sufficient = net.places().iter().any(|p| {
                union & (1u64 << p.id.0) != 0
                    && net
                        .initial_marking()
                        .get(p.id.0 as usize)
                        .map_or(false, |v| match v {
                            racon_core::marking::TokenValue::Omega => true,
                            racon_core::marking::TokenValue::Finite(x) => *x > 0,
                            racon_core::marking::TokenValue::Exact(x) => *x > BigUint::from(0u8),
                        })
            });
            if !sufficient {
                return Some(false);
            }
        }
        Some(true)
    }
    /// Classify a transition according to the five Murata liveness levels.
    ///
    /// L2 means that, for every finite k, some firing sequence from the
    /// initial state contains at least k firings of the transition. L3 means
    /// that one of its firing
    /// edges belongs to a reachable cycle, hence it can occur infinitely
    /// often. L4 is the stronger definition from Starke: every reachable
    /// marking can reach a marking enabling the transition.
    pub fn liveness_level(
        net: &PetriNet,
        transition: TransitionId,
        ss: &dyn StateSpace,
    ) -> LivenessLevel {
        if !ss.is_exact() || ss.node_count() == 0 {
            return LivenessLevel::Unknown;
        }

        let enabled_somewhere =
            (0..ss.node_count()).any(|i| net.is_enabled(transition, ss.marking(NodeId(i as u32))));
        if !enabled_somewhere {
            return LivenessLevel::L0;
        }
        if Self::is_transition_live(net, transition, ss) {
            return LivenessLevel::L4;
        }
        if Self::can_fire_infinitely(transition, ss) {
            return LivenessLevel::L3;
        }
        // On a finite exact reachability graph, arbitrary repetition is
        // witnessed by a reachable cycle and is therefore already L3.
        // A finite threshold alone must never be promoted to L2.
        LivenessLevel::L1
    }

    /// Whether an edge labelled by `transition` is part of a reachable cycle.
    fn can_fire_infinitely(transition: TransitionId, ss: &dyn StateSpace) -> bool {
        for i in 0..ss.node_count() {
            let source = NodeId(i as u32);
            for (target, edge_transition) in ss.outgoing_edges(source) {
                if edge_transition != transition {
                    continue;
                }
                if Self::can_reach(ss, target, source) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether a firing sequence from the initial marking fires `transition`
    /// at least `k` times. The count is capped at `k`, so the search is finite.
    pub fn can_fire_at_least(
        _net: &PetriNet,
        transition: TransitionId,
        ss: &dyn StateSpace,
        k: usize,
    ) -> bool {
        if !ss.is_exact() {
            return false;
        }
        if k == 0 {
            return true;
        }
        let mut queue = VecDeque::from([(ss.initial_node(), 0usize)]);
        let mut visited = HashSet::new();
        while let Some((node, count)) = queue.pop_front() {
            if count >= k {
                return true;
            }
            if !visited.insert((node, count)) {
                continue;
            }
            for (target, edge_transition) in ss.outgoing_edges(node) {
                let fired = edge_transition == transition;
                queue.push_back((target, (count + usize::from(fired)).min(k)));
            }
        }
        false
    }

    fn can_reach(ss: &dyn StateSpace, from: NodeId, target: NodeId) -> bool {
        let mut queue = VecDeque::from([from]);
        let mut visited = HashSet::new();
        while let Some(node) = queue.pop_front() {
            if node == target {
                return true;
            }
            if visited.insert(node) {
                queue.extend(ss.successors(node).iter().copied());
            }
        }
        false
    }

    /// Return all reachable-state nodes with no outgoing firing edge.
    pub fn deadlock_nodes(ss: &dyn StateSpace) -> Vec<NodeId> {
        (0..ss.node_count())
            .map(|i| NodeId(i as u32))
            .filter(|&node| ss.successors(node).is_empty())
            .collect()
    }

    /// DTr: whether at least one transition is dead from the initial marking.
    ///
    /// A transition is dead only when no reachable marking enables it. An
    /// inexact state space cannot establish either answer definitively.
    pub fn has_dead_transition(net: &PetriNet, ss: &dyn StateSpace) -> Option<bool> {
        if !ss.is_exact() {
            return None;
        }
        Some(!Self::dead_transitions(net, ss).is_empty())
    }

    /// DSt: whether a dead state is reachable from the initial marking.
    ///
    /// A dead state is a reachable marking with no enabled transition. An
    /// inexact state space cannot establish either answer definitively.
    pub fn has_reachable_dead_state(ss: &dyn StateSpace) -> Option<bool> {
        if !ss.is_exact() {
            return None;
        }
        Some(!Self::deadlock_nodes(ss).is_empty())
    }

    /// A transition is live iff every state can reach a state where it is
    /// enabled. Reverse traversal computes this condition without confusing
    /// one observed firing with liveness.
    pub fn is_transition_live(
        net: &PetriNet,
        transition: TransitionId,
        ss: &dyn StateSpace,
    ) -> bool {
        if !ss.is_exact() || ss.node_count() == 0 {
            return false;
        }

        let enabled: Vec<NodeId> = (0..ss.node_count())
            .map(|i| NodeId(i as u32))
            .filter(|&node| net.is_enabled(transition, ss.marking(node)))
            .collect();
        if enabled.is_empty() {
            return false;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::from(enabled);
        while let Some(node) = queue.pop_front() {
            if !visited.insert(node) {
                continue;
            }
            queue.extend(ss.predecessors(node).iter().copied());
        }
        visited.len() == ss.node_count()
    }

    /// Return transitions that are never enabled in the supplied reachable
    /// state space.
    pub fn dead_transitions(net: &PetriNet, ss: &dyn StateSpace) -> Vec<TransitionId> {
        net.transitions()
            .iter()
            .map(|transition| transition.id)
            .filter(|&id| {
                !(0..ss.node_count()).any(|i| net.is_enabled(id, ss.marking(NodeId(i as u32))))
            })
            .collect()
    }

    /// Whether every transition in the net satisfies the liveness definition.
    pub fn is_live(net: &PetriNet, ss: &dyn StateSpace) -> bool {
        net.transitions()
            .iter()
            .all(|transition| Self::is_transition_live(net, transition.id, ss))
    }

    /// Whether a place is live according to Starke's Definition 14.5.
    /// A place with no post-transition is outside that definition and is not
    /// reported dead. For every other place, every reachable state must be
    /// able to reach a state with enough tokens for at least one post-transition;
    /// the definition uses the maximum outgoing multiplicity as the threshold.
    pub fn is_place_live(net: &PetriNet, place: PlaceId, ss: &dyn StateSpace) -> bool {
        if place.0 as usize >= net.place_count() || ss.node_count() == 0 {
            return false;
        }

        let outgoing_weights: Vec<u32> = net
            .transitions()
            .iter()
            .flat_map(|transition| {
                net.preset(transition.id)
                    .iter()
                    .filter_map(move |&(candidate, weight)| (candidate == place).then_some(weight))
            })
            .collect();
        let Some(threshold) = outgoing_weights.into_iter().max() else {
            return true;
        };

        let enabling_nodes: Vec<NodeId> = (0..ss.node_count())
            .map(|index| NodeId(index as u32))
            .filter(|&node| match ss.marking(node).get(place.0 as usize) {
                Some(racon_core::marking::TokenValue::Omega) => true,
                Some(racon_core::marking::TokenValue::Finite(value)) => *value >= threshold as u64,
                Some(racon_core::marking::TokenValue::Exact(value)) => {
                    *value >= BigUint::from(threshold)
                }
                None => false,
            })
            .collect();

        let mut can_reach_enabling = HashSet::new();
        let mut queue = VecDeque::from(enabling_nodes);
        while let Some(node) = queue.pop_front() {
            if !can_reach_enabling.insert(node) {
                continue;
            }
            queue.extend(ss.predecessors(node).iter().copied());
        }

        can_reach_enabling.len() == ss.node_count()
    }

    /// Return places with post-transitions that are dead in the supplied
    /// state space according to Starke's Definition 14.5.
    pub fn dead_places(net: &PetriNet, ss: &dyn StateSpace) -> Vec<PlaceId> {
        net.places()
            .iter()
            .map(|place| place.id)
            .filter(|&place| {
                let has_postset = net.transitions().iter().any(|transition| {
                    net.preset(transition.id)
                        .iter()
                        .any(|&(candidate, _)| candidate == place)
                });
                has_postset && !Self::is_place_live(net, place, ss)
            })
            .collect()
    }

    /// Derive behavioral properties by combining structural, invariant,
    /// and (optionally) state-space data.
    pub fn derive(
        net: &PetriNet,
        _structural: &StructuralProperties,
        invariants: &InvariantAnalysisResult,
        state_space: Option<&dyn StateSpace>,
    ) -> BehavioralProperties {
        let murata_bounded = invariants
            .structural_bounded_by_inequality
            .as_ref()
            .is_some_and(|certificate| {
                verify_structural_boundedness_certificate(net.incidence_matrix(), certificate)
            });
        let structurally_bounded =
            invariants.is_cpi || invariants.is_conservative || murata_bounded;
        let mut behavior = BehavioralProperties {
            is_bounded: None,
            is_structurally_bounded: structurally_bounded,
            is_reversible: None,
            is_deadlock_free: None,
            liveness_classification: LivenessLevel::Unknown,
            dead_transitions: Vec::new(),
            is_persistent: None,
            is_gfc: None,
        };

        // Rule 1: CPI => structurally bounded => bounded for any finite initial marking
        if structurally_bounded {
            behavior.is_structurally_bounded = true;
            behavior.is_bounded = Some(true);
        }

        // Rule 2: Conservative => bounded
        // Rule 3: State-space analysis
        if let Some(ss) = state_space.filter(|ss| ss.is_exact()) {
            let deadlocks = Self::deadlock_nodes(ss);
            behavior.is_deadlock_free = Some(deadlocks.is_empty());
            behavior.is_reversible = Some(Self::check_reversibility(ss));
            behavior.dead_transitions = Self::dead_transitions(net, ss);
            behavior.is_persistent = Some(Self::is_persistent(net, ss));
            behavior.is_gfc = Some(Self::is_gfc(net, ss));

            let weakest_level = net
                .transitions()
                .iter()
                .map(|transition| Self::liveness_level(net, transition.id, ss))
                .min_by_key(|level| match level {
                    LivenessLevel::L0 => 0,
                    LivenessLevel::L1 => 1,
                    LivenessLevel::L2 => 2,
                    LivenessLevel::L3 => 3,
                    LivenessLevel::L4 => 4,
                    LivenessLevel::Unknown => -1,
                })
                .unwrap_or(LivenessLevel::Unknown);
            behavior.liveness_classification = weakest_level;
        }

        behavior
    }

    fn check_reversibility(ss: &dyn StateSpace) -> bool {
        // Reversibility = every node can reach the initial node.
        // BFS backwards from initial node covers all nodes that can reach it.
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(ss.initial_node());
        visited.insert(ss.initial_node());

        while let Some(v) = queue.pop_front() {
            for &pred in ss.predecessors(v) {
                if visited.insert(pred) {
                    queue.push_back(pred);
                }
            }
        }

        visited.len() == ss.node_count()
    }
}
