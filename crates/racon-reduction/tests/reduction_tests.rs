use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder};
use racon_reduction::StructuralReducer;

/// Chain: p0 → t0 → p1. p1 has no outgoing arcs → sink.
fn net_with_sink_place() -> PetriNet {
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.build(vec![TokenValue::Finite(0), TokenValue::Finite(0)])
        .unwrap()
}

/// Chain: p0 (initial=0) → t0 → p1 (initial=1). p0 has no incoming arcs and initial 0 → source.
fn net_with_source_place() -> PetriNet {
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.build(vec![TokenValue::Finite(0), TokenValue::Finite(1)])
        .unwrap()
}

/// Self-loop: p0 ←→ t0 (preset == postset for t0).
fn net_with_self_loop() -> PetriNet {
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    b.build(vec![TokenValue::Finite(0)]).unwrap()
}

/// p2 → t0 → p0 → t1 and p2 → t0 → p1 → t1.
/// p0 and p1 are duplicates (same preset=[(t0,1)], postset=[(t1,1)]) but NOT sinks.
fn net_with_duplicate_place() -> PetriNet {
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let p2 = b.add_place("p2".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    let t1 = b.add_transition("t1".into(), None);
    b.add_arc(ArcEndpoint::Place(p2), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    b.build(vec![
        TokenValue::Finite(0),
        TokenValue::Finite(0),
        TokenValue::Finite(1),
    ])
    .unwrap()
}

#[test]
fn test_eliminate_sink_place() {
    let net = net_with_sink_place();
    let result = StructuralReducer::reduce(&net);
    assert_eq!(result.reduced_net.places().len(), 1);
    assert_eq!(result.reduced_net.transitions().len(), 1);
    assert_eq!(result.report.total_places_removed, 1);
}

#[test]
fn test_eliminate_source_place() {
    let net = net_with_source_place();
    let result = StructuralReducer::reduce(&net);
    assert_eq!(result.reduced_net.places().len(), 1);
    assert_eq!(result.reduced_net.transitions().len(), 1);
    assert_eq!(result.report.total_places_removed, 1);
}

#[test]
fn test_remove_self_loop_transition_preserves_valid() {
    // A single self-loop is the only transition. Removing it would leave 0 transitions
    // (invalid net). The reducer must preserve the original net.
    let net = net_with_self_loop();
    let result = StructuralReducer::reduce(&net);
    assert_eq!(result.reduced_net.places().len(), 1);
    assert_eq!(result.reduced_net.transitions().len(), 1);
    assert_eq!(result.report.total_transitions_removed, 0);
}

#[test]
fn test_self_loop_with_other_transition() {
    // Net: p1 ←→ t0 (self-loop), p0 → t1 → p1
    // t0 is a self-loop and should be removed. t1 remains.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None); // self-loop
    let t1 = b.add_transition("t1".into(), None);
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p1), 1);
    let net = b
        .build(vec![TokenValue::Finite(0), TokenValue::Finite(0)])
        .unwrap();

    let result = StructuralReducer::reduce(&net);
    assert!(result.report.total_transitions_removed >= 1);
    assert_eq!(result.reduced_net.transitions().len(), 1); // only t1 remains
}

#[test]
fn test_remove_duplicate_places() {
    let net = net_with_duplicate_place();
    let result = StructuralReducer::reduce(&net);
    assert_eq!(result.reduced_net.places().len(), 2); // p2 + one of p0/p1
    assert_eq!(result.report.total_places_removed, 1);
}

#[test]
fn test_reduce_no_op() {
    // Cycle: p0 ↔ t0 → p1 ↔ t1 → p0. Nothing is reducible.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    let t1 = b.add_transition("t1".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    let net = b
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .unwrap();

    let result = StructuralReducer::reduce(&net);
    assert_eq!(result.reduced_net.places().len(), 2);
    assert_eq!(result.reduced_net.transitions().len(), 2);
    assert_eq!(result.report.iterations, 0);
}

#[test]
fn test_reduce_multiple_iterations() {
    // Chain: p0 → t0 → p1 → t1 → p2. p2 is a sink → removed in one iteration.
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("p0".into(), None);
    let p1 = b.add_place("p1".into(), None);
    let p2 = b.add_place("p2".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    let t1 = b.add_transition("t1".into(), None);
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p2), 1);
    let net = b
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(0),
        ])
        .unwrap();

    let result = StructuralReducer::reduce(&net);
    assert!(result.report.total_places_removed > 0);
    assert!(result.report.iterations >= 1);
}
