//! State-equation test: certificates are verified exactly, solutions are real.

use num_bigint::BigInt;
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder};
use racon_invariants::{state_equation_test, StateEquationVerdict};

/// p0(2) --t0--> p1 --t1--> p2, plus a source transition t2 feeding p2 (no input).
fn chain() -> racon_core::PetriNet {
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
    b.build(vec![TokenValue::Finite(2), TokenValue::Finite(0), TokenValue::Finite(0)]).unwrap()
}

fn marking(values: &[i64]) -> Vec<BigInt> {
    values.iter().map(|v| BigInt::from(*v)).collect()
}

#[test]
fn conservation_violation_has_a_verified_farkas_certificate() {
    // The chain conserves 2 tokens; a target with 3 can never be reached.
    match state_equation_test(&chain(), &marking(&[1, 1, 1])) {
        StateEquationVerdict::Infeasible { farkas } => assert!(!farkas.is_empty()),
        other => panic!("expected Infeasible, got {other:?}"),
    }
}

#[test]
fn reachable_marking_yields_an_exact_parikh_vector() {
    // M0 = [2,0,0]  ->  [1,0,1]: fire t0 then t1 once each.
    match state_equation_test(&chain(), &marking(&[1, 0, 1])) {
        StateEquationVerdict::Solution { parikh } => assert_eq!(parikh, marking(&[1, 1])),
        other => panic!("expected Solution, got {other:?}"),
    }
}

#[test]
fn state_equation_is_necessary_not_sufficient() {
    // Never claims Infeasible for a reachable marking.
    for target in [[2, 0, 0], [1, 1, 0], [0, 2, 0], [0, 0, 2], [0, 1, 1]] {
        assert!(
            !matches!(state_equation_test(&chain(), &marking(&target)), StateEquationVerdict::Infeasible { .. }),
            "{target:?} is reachable"
        );
    }
}

#[test]
fn wrong_length_target_is_inconclusive() {
    assert_eq!(state_equation_test(&chain(), &marking(&[1, 1])), StateEquationVerdict::Inconclusive);
}
