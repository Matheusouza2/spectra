//! Integration tests for racon-invariants.

use num_bigint::BigInt;
use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNetBuilder};
use racon_invariants::engine::{InvariantEngine, InvariantEngineImpl};
use racon_invariants::{verify_p_invariants, verify_t_invariants};

/// Build a small net with known P-invariants:
///   p0 ←→ t0 ←→ p1 ←→ t1 ←→ p2   (line)
///   p0 + p1 + p2 = constant (total tokens conserved)
/// This is a simple chain where the sum of tokens is invariant.
fn build_conservative_chain() -> racon_core::PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let p2 = builder.add_place("p2".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);

    // t0: p0 → p1
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    // t1: p1 → p2
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p2), 1);
    // t0: p2 → p0 (reverse cycle)
    builder.add_arc(ArcEndpoint::Place(p2), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);

    builder
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(1),
        ])
        .expect("Should build chain net")
}

fn make_context() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

// =============================================================================
// P-Semiflow Tests
// =============================================================================

#[test]
fn test_p_semiflows_conservative_chain() {
    let net = build_conservative_chain();
    let engine = InvariantEngineImpl::default();
    let ctx = make_context();

    let p_semiflows = engine
        .compute_p_semiflows(&net, &ctx)
        .expect("Should compute P-semiflows");

    assert!(
        p_semiflows.iter().all(|sf| sf.constant_value.is_some()),
        "P-invariants must carry y*m0^T"
    );

    // A conservative chain with a cycle should have at least one P-semiflow
    assert!(!p_semiflows.is_empty(), "Chain should have P-semiflows");

    // Each semiflow should be non-negative
    for sf in &p_semiflows {
        assert!(sf.is_semiflow, "Should be marked as semiflow");
        assert!(
            sf.coefficients.iter().all(|c| c >= &BigInt::from(0)),
            "Coefficients must be >= 0"
        );
        assert!(!sf.support.is_empty(), "Support should not be empty");
    }
}

#[test]
fn test_p_semiflows_pass_verification() {
    let net = build_conservative_chain();
    let engine = InvariantEngineImpl::default();
    let ctx = make_context();

    let p_semiflows = engine
        .compute_p_semiflows(&net, &ctx)
        .expect("Should compute P-semiflows");
    let c = net.incidence_matrix();

    let result = verify_p_invariants(c, &p_semiflows);
    assert!(
        result.is_ok(),
        "P-semiflows should satisfy y^T * C = 0: {:?}",
        result
    );
    racon_invariants::verify_p_semiflow_completeness(c, &p_semiflows, 3).unwrap();
    let certificate = racon_invariants::certify_p_semiflow_completeness(c, &p_semiflows, 3)
        .expect("bounded completeness certificate");
    assert_eq!(certificate.coefficient_bound, 3);
    assert_eq!(certificate.dimension, net.place_count());
    assert!(certificate.verified);
}

#[test]
fn test_p_semiflows_no_invariant() {
    // Net: p0 → t0 (no cycle), C = [-1], no non-trivial left null space
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p".into(), None);
    let t0 = builder.add_transition("t".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    let net = builder
        .build(vec![TokenValue::Finite(0)])
        .expect("Should build net");

    let engine = InvariantEngineImpl::default();
    let ctx = make_context();

    let p_semiflows = engine
        .compute_p_semiflows(&net, &ctx)
        .expect("Should compute P-semiflows");
    assert!(p_semiflows.is_empty(), "No cycle means no P-semiflows");
}

// =============================================================================
// T-Semiflow Tests
// =============================================================================

#[test]
fn test_t_semiflows_conservative_chain() {
    let net = build_conservative_chain();
    let engine = InvariantEngineImpl::default();
    let ctx = make_context();

    let t_semiflows = engine
        .compute_t_semiflows(&net, &ctx)
        .expect("Should compute T-semiflows");

    // The chain cycle t0→t1 should have at least one T-semiflow
    assert!(!t_semiflows.is_empty(), "Chain should have T-semiflows");

    for sf in &t_semiflows {
        assert!(sf.is_semiflow, "Should be marked as semiflow");
        assert!(
            sf.coefficients.iter().all(|c| c >= &BigInt::from(0)),
            "Coefficients must be >= 0"
        );
        assert!(!sf.support.is_empty(), "Support should not be empty");
    }
}

#[test]
fn test_t_semiflows_pass_verification() {
    let net = build_conservative_chain();
    let engine = InvariantEngineImpl::default();
    let ctx = make_context();

    let t_semiflows = engine
        .compute_t_semiflows(&net, &ctx)
        .expect("Should compute T-semiflows");
    let c = net.incidence_matrix();

    let result = verify_t_invariants(c, &t_semiflows);
    assert!(
        result.is_ok(),
        "T-semiflows should satisfy C * x = 0: {:?}",
        result
    );
    assert!(t_semiflows.iter().any(|sf| sf.is_realizable == Some(true)));
    assert!(t_semiflows
        .iter()
        .any(|sf| sf.realization_witness.is_some()));
    racon_invariants::verify_t_semiflow_completeness(c, &t_semiflows, 3).unwrap();
    let certificate = racon_invariants::certify_t_semiflow_completeness(c, &t_semiflows, 3)
        .expect("bounded completeness certificate");
    assert_eq!(certificate.coefficient_bound, 3);
    assert_eq!(certificate.dimension, net.transition_count());
    assert!(certificate.verified);
}

#[test]
fn test_algebraic_t_invariant_can_be_unrealizable_from_initial_marking() {
    let mut builder = PetriNetBuilder::default();
    let place = builder.add_place("p".into(), None);
    let transition = builder.add_transition("loop".into(), None);
    builder.add_arc(
        ArcEndpoint::Place(place),
        ArcEndpoint::Transition(transition),
        1,
    );
    builder.add_arc(
        ArcEndpoint::Transition(transition),
        ArcEndpoint::Place(place),
        1,
    );
    let net = builder.build(vec![TokenValue::Finite(0)]).unwrap();
    let engine = InvariantEngineImpl::default();
    let semiflows = engine.compute_t_semiflows(&net, &make_context()).unwrap();
    assert_eq!(semiflows.len(), 1);
    assert_eq!(semiflows[0].is_realizable, Some(false));
    assert!(semiflows[0].realization_witness.is_none());
}

#[test]
fn test_murata_structural_boundedness_certificate_is_verified() {
    use racon_invariants::structural_boundedness_certificate;
    let net = build_conservative_chain();
    let certificate =
        structural_boundedness_certificate(&net).expect("positive Ay <= 0 certificate");
    assert!(certificate.iter().all(|value| *value > BigInt::from(0)));
    assert!(net
        .incidence_matrix()
        .left_multiply(&certificate)
        .iter()
        .all(|value| *value <= BigInt::from(0)));
    assert!(racon_invariants::verify_structural_boundedness_certificate(
        net.incidence_matrix(),
        &certificate
    ));
    assert!(
        !racon_invariants::verify_structural_boundedness_certificate(
            net.incidence_matrix(),
            &[BigInt::from(0), BigInt::from(1), BigInt::from(1)]
        )
    );
}

#[test]
fn test_invariant_verification_preserves_bigint_coefficients() {
    let net = build_conservative_chain();
    let huge = BigInt::from(i64::MAX) + BigInt::from(1);
    let p = racon_invariants::types::PInvariant {
        coefficients: vec![huge.clone(), huge.clone(), huge.clone()],
        is_semiflow: true,
        is_minimal: false,
        support: vec![
            racon_core::PlaceId(0),
            racon_core::PlaceId(1),
            racon_core::PlaceId(2),
        ],
        constant_value: None,
    };
    let t = racon_invariants::types::TInvariant {
        coefficients: vec![huge.clone(), huge.clone()],
        is_semiflow: true,
        is_minimal: false,
        support: vec![racon_core::TransitionId(0), racon_core::TransitionId(1)],
        is_realizable: None,
        realization_witness: None,
    };

    verify_p_invariants(net.incidence_matrix(), &[p]).expect("BigInt y^T C must verify");
    verify_t_invariants(net.incidence_matrix(), &[t]).expect("BigInt Cx must verify");
}

// =============================================================================
// Verification Tests
// =============================================================================

#[test]
fn test_verify_p_invariant_rejects_bad_length() {
    let net = build_conservative_chain();
    let c = net.incidence_matrix();

    let bad = racon_invariants::types::PInvariant {
        coefficients: vec![BigInt::from(1)], // too short (net has 3 places)
        is_semiflow: true,
        is_minimal: true,
        support: vec![racon_core::PlaceId(0)],
        constant_value: None,
    };

    let result = verify_p_invariants(c, &[bad]);
    assert!(result.is_err(), "Should reject wrong-length invariant");
}

#[test]
fn test_verify_t_invariant_rejects_bad_length() {
    let net = build_conservative_chain();
    let c = net.incidence_matrix();

    let bad = racon_invariants::types::TInvariant {
        coefficients: vec![BigInt::from(1)], // too short (net has 2 transitions)
        is_semiflow: true,
        is_minimal: true,
        support: vec![racon_core::TransitionId(0)],
        is_realizable: None,
        realization_witness: None,
    };

    let result = verify_t_invariants(c, &[bad]);
    assert!(result.is_err(), "Should reject wrong-length invariant");
}

#[test]
fn test_murata_certificate_sink_net_and_non_sb_net() {
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};
    use racon_invariants::{structural_boundedness_certificate, verify_structural_boundedness_certificate};

    // Starke Satz 11.9 remark: one transition with one input place has no
    // P-invariant but is structurally bounded.
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    let sink = b.build(vec![TokenValue::Finite(1)]).unwrap();
    let cert = structural_boundedness_certificate(&sink).expect("sink net is SB");
    assert!(verify_structural_boundedness_certificate(sink.incidence_matrix(), &cert));

    // t consumes 1 from p and produces 2: not structurally bounded.
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 2);
    let growing = b.build(vec![TokenValue::Finite(1)]).unwrap();
    assert!(structural_boundedness_certificate(&growing).is_none());
}
