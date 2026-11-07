//! Basis of ALL invariants: exact, spanning, and larger than the semiflow set when needed.

use num_bigint::BigInt;
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder};
use racon_core::PetriNet;
use racon_invariants::{p_invariant_basis, t_invariant_basis, InvariantEngine, InvariantEngineImpl};

fn ints(values: &[i64]) -> Vec<BigInt> {
    values.iter().map(|v| BigInt::from(*v)).collect()
}

/// p --t0--> q and p --t1--> q: C = [[-1,-1],[1,1]].
fn parallel() -> PetriNet {
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let q = b.add_place("q".into(), None);
    for name in ["t0", "t1"] {
        let t = b.add_transition(name.into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(q), 1);
    }
    b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)]).unwrap()
}

fn assert_is_kernel(net: &PetriNet, basis: &[Vec<BigInt>], places: bool) {
    let m = net.incidence_matrix();
    for v in basis {
        if places {
            for t in 0..m.transition_count() {
                let dot: BigInt = (0..m.place_count()).map(|p| &v[p] * m.get(p, t).unwrap()).sum();
                assert_eq!(dot, BigInt::default(), "y^T C != 0 for {v:?}");
            }
        } else {
            for p in 0..m.place_count() {
                let dot: BigInt = (0..m.transition_count()).map(|t| m.get(p, t).unwrap() * &v[t]).sum();
                assert_eq!(dot, BigInt::default(), "C x != 0 for {v:?}");
            }
        }
    }
}

#[test]
fn t_basis_contains_a_vector_with_negative_entries_that_no_semiflow_has() {
    let net = parallel();
    let basis = t_invariant_basis(&net);
    assert_eq!((basis.rank, basis.dimension), (1, 1));
    assert_eq!(basis.vectors, vec![ints(&[1, -1])]);
    assert_is_kernel(&net, &basis.vectors, false);
    // Semiflows are non-negative, so none exists here, yet the kernel is not trivial.
    let semiflows = InvariantEngineImpl::default()
        .compute_t_semiflows(&net, &racon_core::execution::AnalysisContext::new(racon_core::execution::AnalysisConfig::default()))
        .unwrap();
    assert!(semiflows.is_empty());
}

#[test]
fn p_basis_is_the_token_sum_and_dimension_is_places_minus_rank() {
    let net = parallel();
    let basis = p_invariant_basis(&net);
    assert_eq!(basis.vectors, vec![ints(&[1, 1])]);
    assert_eq!(basis.dimension, 2 - basis.rank);
    assert_is_kernel(&net, &basis.vectors, true);
}

#[test]
fn kernel_is_trivial_when_the_matrix_has_full_column_rank() {
    // One transition p -> q: C^T has rank 1 over 2 places (a P-invariant), C has rank 1 over 1 transition.
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let q = b.add_place("q".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(q), 2);
    let net = b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)]).unwrap();
    assert!(t_invariant_basis(&net).vectors.is_empty(), "firing t always changes the marking");
    let pb = p_invariant_basis(&net);
    assert_eq!(pb.vectors, vec![ints(&[2, 1])], "2*p + q is conserved");
    assert_is_kernel(&net, &pb.vectors, true);
}
