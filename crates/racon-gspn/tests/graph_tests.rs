use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder};
use racon_gspn::{GspnGraph, GspnSpec, TransitionType};
use std::collections::HashMap;
use std::sync::Arc;

/// Build a simple two-state CTMC:
///   P0(1), P1(0)
///   T0: P0 -> P1, timed rate=1.0
///   T1: P1 -> P0, timed rate=2.0
fn build_two_state_ctmc() -> (Arc<PetriNet>, GspnSpec) {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".into(), None);
    let p1 = builder.add_place("P1".into(), None);
    let t0 = builder.add_transition("T0".into(), None);
    let t1 = builder.add_transition("T1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    let net = Arc::new(
        builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap(),
    );
    let mut map = HashMap::new();
    map.insert(t0, TransitionType::Timed { rate: 1.0 });
    map.insert(t1, TransitionType::Timed { rate: 2.0 });
    (
        net,
        GspnSpec {
            transition_types: map,
        },
    )
}

/// Build a GSPN with an immediate transition:
///   P0(1), P1(0), P2(0)
///   T0(timed, rate=1.0): P0 -> P1
///   IM0(imm, weight=1): P1 -> P2
fn build_immediate_gspn() -> (Arc<PetriNet>, GspnSpec) {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".into(), None);
    let p1 = builder.add_place("P1".into(), None);
    let p2 = builder.add_place("P2".into(), None);
    let t0 = builder.add_transition("T0".into(), None);
    let im0 = builder.add_transition("IM0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(im0), 1);
    builder.add_arc(ArcEndpoint::Transition(im0), ArcEndpoint::Place(p2), 1);
    let net = Arc::new(
        builder
            .build(vec![
                TokenValue::Finite(1),
                TokenValue::Finite(0),
                TokenValue::Finite(0),
            ])
            .unwrap(),
    );
    let mut map = HashMap::new();
    map.insert(t0, TransitionType::Timed { rate: 1.0 });
    map.insert(
        im0,
        TransitionType::Immediate {
            weight: 1.0,
            priority: 0,
        },
    );
    (
        net,
        GspnSpec {
            transition_types: map,
        },
    )
}

#[test]
fn test_gspn_graph_two_state() {
    let (net, spec) = build_two_state_ctmc();
    let graph = GspnGraph::build(&net, &spec).unwrap();
    assert_eq!(graph.n_tangible(), 2);
    assert_eq!(graph.q_matrix.n, 2);
}

#[test]
fn test_gspn_graph_no_timed_transitions() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".into(), None);
    let t0 = builder.add_transition("T0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    let net = Arc::new(builder.build(vec![TokenValue::Finite(1)]).unwrap());
    let spec = GspnSpec {
        transition_types: HashMap::new(),
    };
    let result = GspnGraph::build(&net, &spec);
    assert!(result.is_err());
}

#[test]
fn test_gspn_graph_immediate_elimination() {
    let (net, spec) = build_immediate_gspn();
    let graph = GspnGraph::build(&net, &spec).unwrap();
    // Tangible states: P0(1,0,0) and P2(0,0,1)
    // P1(0,1,0) is vanishing (IM0 is enabled) → eliminated
    assert_eq!(graph.n_tangible(), 2);
}

#[test]
fn test_gspn_graph_rate_through_vanishing() {
    let (net, spec) = build_immediate_gspn();
    let graph = GspnGraph::build(&net, &spec).unwrap();
    // Tangible states:
    //   0: P0(1,0,0) → T0 enabled, rate=1.0
    //   1: P2(0,0,1) → deadlock (no outgoing transitions)
    // The transition from state 0 to state 1 should have rate = 1.0
    //   (through vanishing marking P1, IM0 fires with prob=1.0)
    let q_01 = graph.get_rate(0, 1);
    assert!(
        (q_01 - 1.0).abs() < 1e-10,
        "expected rate 1.0, got {}",
        q_01
    );
}

#[test]
fn test_gspn_graph_timed_transition_rates() {
    let (net, spec) = build_two_state_ctmc();
    let graph = GspnGraph::build(&net, &spec).unwrap();
    // State 0 → State 1: rate = 1.0 (T0)
    assert!(
        (graph.get_rate(0, 1) - 1.0).abs() < 1e-10,
        "expected rate 1.0, got {}",
        graph.get_rate(0, 1)
    );
    // State 1 → State 0: rate = 2.0 (T1)
    assert!(
        (graph.get_rate(1, 0) - 2.0).abs() < 1e-10,
        "expected rate 2.0, got {}",
        graph.get_rate(1, 0)
    );
}

#[test]
fn test_gspn_graph_diagonal() {
    let (net, spec) = build_two_state_ctmc();
    let graph = GspnGraph::build(&net, &spec).unwrap();
    // State 0: outgoing rate = 1.0, so diagonal = -1.0
    assert!(
        (graph.q_matrix.diagonal[0] + 1.0).abs() < 1e-10,
        "expected diagonal[0] = -1.0, got {}",
        graph.q_matrix.diagonal[0]
    );
    // State 1: outgoing rate = 2.0, so diagonal = -2.0
    assert!(
        (graph.q_matrix.diagonal[1] + 2.0).abs() < 1e-10,
        "expected diagonal[1] = -2.0, got {}",
        graph.q_matrix.diagonal[1]
    );
}
