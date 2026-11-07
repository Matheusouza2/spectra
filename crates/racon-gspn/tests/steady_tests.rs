use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder};
use racon_gspn::{solve_steady_state, GspnGraph, GspnSpec, QMatrix, TransitionType};
use std::collections::HashMap;
use std::sync::Arc;

/// Two-state CTMC: rates 1.0 and 2.0.
/// Steady-state: π0 = 2/3, π1 = 1/3
fn build_two_state_ctmc() -> (GspnGraph, Vec<f64>) {
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
    let spec = GspnSpec {
        transition_types: map,
    };
    let graph = GspnGraph::build(&net, &spec).unwrap();
    let expected = vec![2.0 / 3.0, 1.0 / 3.0];
    (graph, expected)
}

#[test]
fn test_steady_state_two_state() {
    let (graph, expected) = build_two_state_ctmc();
    let result = solve_steady_state(&graph.q_matrix, None).unwrap();
    assert!(result.converged);
    assert!(
        (result.probabilities[0] - expected[0]).abs() < 1e-6,
        "expected π0 = {}, got {}",
        expected[0],
        result.probabilities[0]
    );
    assert!(
        (result.probabilities[1] - expected[1]).abs() < 1e-6,
        "expected π1 = {}, got {}",
        expected[1],
        result.probabilities[1]
    );
}

#[test]
fn test_steady_state_sum_to_one() {
    let (graph, _) = build_two_state_ctmc();
    let result = solve_steady_state(&graph.q_matrix, None).unwrap();
    let sum: f64 = result.probabilities.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "probabilities sum to {}", sum);
}

#[test]
fn test_steady_state_custom_tolerance() {
    let (graph, expected) = build_two_state_ctmc();
    let result = solve_steady_state(&graph.q_matrix, Some(1e-8)).unwrap();
    assert!(result.converged);
    assert!(
        (result.probabilities[0] - expected[0]).abs() < 1e-6,
        "expected π0 = {}, got {}",
        expected[0],
        result.probabilities[0]
    );
}

#[test]
fn test_steady_state_empty_qmatrix() {
    let q = QMatrix {
        n: 0,
        transitions: vec![],
        diagonal: vec![],
    };
    let result = solve_steady_state(&q, None).unwrap();
    assert!(result.converged);
    assert!(result.probabilities.is_empty());
    assert_eq!(result.iterations, 0);
}

#[test]
fn test_steady_state_no_timed_transitions() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".into(), None);
    let t0 = builder.add_transition("T0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    // Add T0 as P/T transition only — NOT in GspnSpec
    let net = Arc::new(builder.build(vec![TokenValue::Finite(1)]).unwrap());
    let spec = GspnSpec {
        transition_types: HashMap::new(),
    };
    let result = GspnGraph::build(&net, &spec);
    assert!(result.is_err()); // No timed transitions in spec → GspnError::NoTimedTransitions
}
