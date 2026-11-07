//! Integration tests for racon-structural.

use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder, PlaceId};
use racon_core::Marking;
use racon_structural::expert::{ExpertSystem, LivenessLevel};
use racon_structural::topology::StructuralAnalyzer;

// =============================================================================
// Helpers
// =============================================================================

/// Build a simple ordinary net: p0 → t0 → p1 → t1 → p0 (cycle, all weights = 1)
fn build_ordinary_cycle() -> racon_core::PetriNet {
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
        .expect("Should build cycle net")
}

/// Build a net with a self-loop (impure): p0 → t0 → p0
fn build_self_loop() -> racon_core::PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let t0 = builder.add_transition("t0".into(), None);

    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);

    builder
        .build(vec![TokenValue::Finite(1)])
        .expect("Should build self-loop net")
}

/// Build a net with a non-unit arc weight (non-ordinary)
fn build_weighted_net() -> racon_core::PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0".into(), None);

    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 2); // weight 2
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);

    builder
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .expect("Should build weighted net")
}

/// Build two disconnected components:
///   Component 1: p0 → t0 → p1
///   Component 2: p2 → t1 → p3
fn build_disconnected() -> racon_core::PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let p2 = builder.add_place("p2".into(), None);
    let p3 = builder.add_place("p3".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);

    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    builder.add_arc(ArcEndpoint::Place(p2), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p3), 1);

    builder
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(1),
            TokenValue::Finite(0),
        ])
        .expect("Should build disconnected net")
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn test_ordinary_net() {
    let props = StructuralAnalyzer::analyze(&build_ordinary_cycle());
    assert!(props.is_ordinary, "Ordinary net should be ordinary");
    assert!(props.is_pure, "Cycle should be pure (no self-loops)");
    assert!(props.is_connected, "Cycle should be connected");
    assert!(
        props.is_state_machine,
        "Each place has ≤1 input and ≤1 output"
    );
    assert!(
        props.is_marked_graph,
        "Each transition has ≤1 input and ≤1 output"
    );
}

#[test]
fn test_non_ordinary_net() {
    let props = StructuralAnalyzer::analyze(&build_weighted_net());
    assert!(!props.is_ordinary, "Weighted net should not be ordinary");
}

#[test]
fn test_self_loop_is_impure() {
    let props = StructuralAnalyzer::analyze(&build_self_loop());
    assert!(!props.is_pure, "Self-loop should be impure");
    assert!(props.is_ordinary, "Self-loop with weight 1 is ordinary");
    assert!(props.is_connected, "Self-loop is connected");
}

#[test]
fn test_disconnected_net() {
    let props = StructuralAnalyzer::analyze(&build_disconnected());
    assert!(
        !props.is_connected,
        "Disconnected net should not be connected"
    );
}

#[test]
fn test_source_sink_detection() {
    // Ordinary cycle has no source/sink places
    let props = StructuralAnalyzer::analyze(&build_ordinary_cycle());
    assert!(
        !props.has_transitions_without_preset,
        "All transitions have preset"
    );
    assert!(
        !props.has_transitions_without_postset,
        "All transitions have postset"
    );
    assert!(!props.has_places_without_preset, "All places have preset");
    assert!(!props.has_places_without_postset, "All places have postset");
}

#[test]
fn test_source_sink_properties_match_empty_preset_and_postset_definitions() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0-source".into(), None);
    let t1 = builder.add_transition("t1-sink".into(), None);

    // t0 has no preset and produces p0.
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    // t1 consumes p1 and has no postset.
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);

    let net = builder
        .build(vec![TokenValue::Finite(0), TokenValue::Finite(1)])
        .expect("source/sink net should build");
    let props = StructuralAnalyzer::analyze(&net);

    assert!(props.has_transitions_without_preset);
    assert!(props.has_transitions_without_postset);
    assert!(props.has_places_without_preset);
    assert!(props.has_places_without_postset);
}

#[test]
fn test_static_conflict_free() {
    let props = StructuralAnalyzer::analyze(&build_ordinary_cycle());
    assert!(
        props.is_static_conflict_free,
        "Each place in cycle has ≤1 output"
    );
}

#[test]
fn test_minimal_chain_is_trivial() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    let net = builder
        .build(vec![TokenValue::Finite(0), TokenValue::Finite(0)])
        .expect("Should build minimal chain net");
    let props = StructuralAnalyzer::analyze(&net);
    assert!(props.is_connected, "Chain should be connected");
    assert!(props.is_ordinary, "Chain should be ordinary");
    assert!(props.is_pure, "Chain should be pure");
    assert!(
        props.is_state_machine,
        "Each place has ≤1 input and ≤1 output"
    );
}

#[test]
fn test_empty_net_rejected() {
    let err = PetriNetBuilder::default()
        .build(vec![])
        .expect_err("Should reject net with no places");
    assert!(
        err.to_string().contains("No places"),
        "Expected 'No places' error, got: {err}"
    );
}

#[test]
fn test_free_choice() {
    // A simple free-choice net: p0 → t0 or t1
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 1);

    let net = builder
        .build(vec![TokenValue::Finite(1)])
        .expect("Should build free-choice net");
    let props = StructuralAnalyzer::analyze(&net);
    assert!(props.is_free_choice, "Simple choice should be free choice");
    // p0 has 2 output transitions, so not static conflict free
    assert!(
        !props.is_static_conflict_free,
        "p0 has 2 outputs → not static conflict free"
    );
    assert!(
        props.is_extended_free_choice,
        "The two transitions have equal presets"
    );
}

#[test]
fn test_marked_graph_and_state_machine_use_opposite_node_sides() {
    // Two transitions each have exactly one input and output, but p0 has two
    // incoming transitions. This is a state machine, not a marked graph.
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let p2 = builder.add_place("p2".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    builder.add_arc(ArcEndpoint::Place(p2), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    let net = builder
        .build(vec![
            TokenValue::Finite(0),
            TokenValue::Finite(1),
            TokenValue::Finite(1),
        ])
        .expect("state-machine test net should build");

    let props = StructuralAnalyzer::analyze(&net);
    assert!(props.is_state_machine);
    assert!(!props.is_marked_graph);
}

#[test]
fn test_marked_graph_requires_exactly_one_input_and_output_per_place() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    let net = builder
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .expect("chain net should build");

    assert!(!StructuralAnalyzer::analyze(&net).is_marked_graph);
}

#[test]
fn test_free_choice_requires_unit_shared_preset() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 2);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 2);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    let net = builder
        .build(vec![TokenValue::Finite(2), TokenValue::Finite(1)])
        .expect("weighted choice net should build");

    let props = StructuralAnalyzer::analyze(&net);
    assert!(!props.is_free_choice);
    assert!(props.is_extended_free_choice);
}

#[test]
fn test_weighted_free_choice_allows_non_unit_shared_arc() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t0), 2);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t1), 3);
    let net = builder.build(vec![TokenValue::Finite(3)]).unwrap();

    assert!(StructuralAnalyzer::analyze(&net).is_free_choice);
}

struct MockStateSpace {
    markings: Vec<Marking>,
    successors: Vec<Vec<NodeId>>,
    predecessors: Vec<Vec<NodeId>>,
}

impl StateSpace for MockStateSpace {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        &self.successors[node.0 as usize]
    }
    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        &self.predecessors[node.0 as usize]
    }
    fn marking(&self, node: NodeId) -> &Marking {
        &self.markings[node.0 as usize]
    }
    fn node_count(&self) -> usize {
        self.markings.len()
    }
    fn edge_count(&self) -> usize {
        self.successors.iter().map(Vec::len).sum()
    }
    fn initial_node(&self) -> NodeId {
        NodeId(0)
    }
    fn is_finite(&self) -> bool {
        true
    }
}

struct InexactStateSpace<'a>(&'a MockStateSpace);

impl StateSpace for InexactStateSpace<'_> {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        self.0.successors(node)
    }
    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        self.0.predecessors(node)
    }
    fn marking(&self, node: NodeId) -> &Marking {
        self.0.marking(node)
    }
    fn node_count(&self) -> usize {
        self.0.node_count()
    }
    fn edge_count(&self) -> usize {
        self.0.edge_count()
    }
    fn initial_node(&self) -> NodeId {
        self.0.initial_node()
    }
    fn is_finite(&self) -> bool {
        self.0.is_finite()
    }
    fn is_exact(&self) -> bool {
        false
    }
}

fn build_live_cycle_with_dead_transition() -> racon_core::PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let p2 = builder.add_place("p2".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    let dead = builder.add_transition("dead".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    builder.add_arc(ArcEndpoint::Place(p2), ArcEndpoint::Transition(dead), 1);
    builder.add_arc(ArcEndpoint::Transition(dead), ArcEndpoint::Place(p2), 1);
    builder
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(0),
        ])
        .expect("live cycle with dead transition should build")
}

#[test]
fn test_liveness_is_reachability_from_every_future_state() {
    let net = build_live_cycle_with_dead_transition();
    let space = MockStateSpace {
        markings: vec![
            Marking::from_finite(&[1, 0, 0]),
            Marking::from_finite(&[0, 1, 0]),
        ],
        successors: vec![vec![NodeId(1)], vec![NodeId(0)]],
        predecessors: vec![vec![NodeId(1)], vec![NodeId(0)]],
    };

    assert!(ExpertSystem::is_transition_live(
        &net,
        racon_core::TransitionId(0),
        &space
    ));
    assert_eq!(
        ExpertSystem::dead_transitions(&net, &space),
        vec![racon_core::TransitionId(2)]
    );
    assert!(!ExpertSystem::is_live(&net, &space));
}

#[test]
fn test_dtr_requires_a_transition_dead_over_all_reachable_states() {
    let net = build_ordinary_cycle();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[1, 0]), Marking::from_finite(&[0, 1])],
        successors: vec![vec![NodeId(1)], vec![NodeId(0)]],
        predecessors: vec![vec![NodeId(1)], vec![NodeId(0)]],
    };

    assert_eq!(ExpertSystem::has_dead_transition(&net, &space), Some(false));
}

#[test]
fn test_dst_requires_a_reachable_state_with_no_enabled_transition() {
    let net = build_ordinary_cycle();
    let space = MockStateSpace {
        markings: vec![net.initial_marking().clone()],
        successors: vec![vec![]],
        predecessors: vec![vec![]],
    };

    assert_eq!(ExpertSystem::has_reachable_dead_state(&space), Some(true));
}

#[test]
fn test_deadlock_nodes_are_states_without_successors() {
    let net = build_ordinary_cycle();
    let space = MockStateSpace {
        markings: vec![net.initial_marking().clone()],
        successors: vec![vec![]],
        predecessors: vec![vec![]],
    };
    assert_eq!(ExpertSystem::deadlock_nodes(&space), vec![NodeId(0)]);
}

#[test]
fn test_place_liveness_follows_starke_definition() {
    let net = build_ordinary_cycle();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[1, 0]), Marking::from_finite(&[0, 1])],
        successors: vec![vec![NodeId(1)], vec![NodeId(0)]],
        predecessors: vec![vec![NodeId(1)], vec![NodeId(0)]],
    };

    assert!(ExpertSystem::is_place_live(&net, PlaceId(0), &space));
    assert!(ExpertSystem::dead_places(&net, &space).is_empty());
}

#[test]
fn test_place_liveness_detects_place_dead_in_all_future_states() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let sink = builder.add_place("sink".into(), None);
    let t = builder.add_transition("consume".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(sink), 1);
    let net = builder
        .build(vec![TokenValue::Finite(0), TokenValue::Finite(0)])
        .unwrap();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[0, 0])],
        successors: vec![vec![]],
        predecessors: vec![vec![]],
    };

    assert!(!ExpertSystem::is_place_live(&net, p, &space));
    assert_eq!(ExpertSystem::dead_places(&net, &space), vec![p]);
}

#[test]
fn test_place_without_postset_is_not_reported_dead() {
    let net = build_ordinary_cycle();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[1, 0])],
        successors: vec![vec![]],
        predecessors: vec![vec![]],
    };

    assert!(ExpertSystem::is_place_live(&net, PlaceId(0), &space));
}

fn classify_net(
    net: &racon_core::PetriNet,
    markings: Vec<Marking>,
    successors: Vec<Vec<NodeId>>,
) -> LivenessLevel {
    let mut predecessors = vec![Vec::new(); markings.len()];
    for (source, targets) in successors.iter().enumerate() {
        for &target in targets {
            predecessors[target.0 as usize].push(NodeId(source as u32));
        }
    }
    let space = MockStateSpace {
        markings,
        successors,
        predecessors,
    };
    ExpertSystem::liveness_level(net, racon_core::TransitionId(0), &space)
}

#[test]
fn test_starke_liveness_levels_l0_to_l4() {
    // L0: transition is never enabled.
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    let l0 = b.build(vec![TokenValue::Finite(0)]).unwrap();
    assert_eq!(
        classify_net(&l0, vec![Marking::from_finite(&[0])], vec![vec![]]),
        LivenessLevel::L0
    );

    // L1: enabled once, but not twice and not infinitely often.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p1), 1);
    let l1 = b
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .unwrap();
    assert_eq!(
        classify_net(
            &l1,
            vec![Marking::from_finite(&[1, 0]), Marking::from_finite(&[0, 1])],
            vec![vec![NodeId(1)], vec![]]
        ),
        LivenessLevel::L1
    );

    // Two firings alone are still L1, not L2: Murata L2 requires arbitrary k.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p1), 1);
    let l2 = b
        .build(vec![TokenValue::Finite(2), TokenValue::Finite(0)])
        .unwrap();
    assert_eq!(
        classify_net(
            &l2,
            vec![
                Marking::from_finite(&[2, 0]),
                Marking::from_finite(&[1, 1]),
                Marking::from_finite(&[0, 2])
            ],
            vec![vec![NodeId(1)], vec![NodeId(2)], vec![]]
        ),
        LivenessLevel::L1
    );

    // L3: a reachable self-cycle fires t infinitely, but another branch kills it.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t = b.add_transition("t".into(), None);
    let dead = b.add_transition("dead".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p0), 1);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(dead), 1);
    b.add_arc(ArcEndpoint::Transition(dead), ArcEndpoint::Place(p1), 1);
    let l3 = b
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .unwrap();
    assert_eq!(
        classify_net(
            &l3,
            vec![Marking::from_finite(&[1, 0]), Marking::from_finite(&[0, 1])],
            vec![vec![NodeId(0), NodeId(1)], vec![]]
        ),
        LivenessLevel::L3
    );

    // L4: every reachable marking can reach a marking enabling t.
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
    let l4 = b.build(vec![TokenValue::Finite(1)]).unwrap();
    assert_eq!(
        classify_net(&l4, vec![Marking::from_finite(&[1])], vec![vec![NodeId(0)]]),
        LivenessLevel::L4
    );
}

#[test]
fn test_l2_requires_arbitrarily_many_firings() {
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
    let net = b.build(vec![TokenValue::Finite(1)]).unwrap();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[1])],
        successors: vec![vec![NodeId(0)]],
        predecessors: vec![vec![NodeId(0)]],
    };

    assert!(ExpertSystem::can_fire_at_least(
        &net,
        racon_core::TransitionId(0),
        &space,
        2
    ));
    assert!(ExpertSystem::can_fire_at_least(
        &net,
        racon_core::TransitionId(0),
        &space,
        3
    ));
    assert!(ExpertSystem::can_fire_at_least(
        &net,
        racon_core::TransitionId(0),
        &space,
        256
    ));
    assert_eq!(
        ExpertSystem::liveness_level(&net, racon_core::TransitionId(0), &space),
        LivenessLevel::L4
    );
}

#[test]
fn behavioral_properties_reject_inexact_state_spaces() {
    let net = build_self_loop();
    let space = MockStateSpace {
        markings: vec![Marking::from_finite(&[1])],
        successors: vec![vec![NodeId(0)]],
        predecessors: vec![vec![NodeId(0)]],
    };
    let inexact = InexactStateSpace(&space);

    assert_eq!(
        ExpertSystem::liveness_level(&net, racon_core::TransitionId(0), &inexact),
        LivenessLevel::Unknown
    );
    assert!(!ExpertSystem::is_transition_live(
        &net,
        racon_core::TransitionId(0),
        &inexact
    ));
}
