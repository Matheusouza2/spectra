//! Comprehensive integration tests for racon-core.
//!
//! Tests cover: PetriNetBuilder, preset/postset, is_enabled/fire,
//! Marking creation and query, TokenValue arithmetic and comparison,
//! IncidenceMatrix dimensions and operations, NodeId ordering and Display,
//! AnalysisConfig defaults, CancellationToken, and MemoryBudget.

use num_bigint::BigInt;
use racon_core::{
    // Execution
    AnalysisConfig,
    AnalysisContext,
    AnalysisEngine,
    // Net types
    ArcEndpoint,
    CancellationToken,
    EdgeId,
    // Marking types
    Marking,
    MemoryBudget,
    MemoryError,
    // Matrix
    // Graph
    NodeId,
    PetriNet,
    PetriNetBuilder,
    PlaceId,
    // Error
    RaconError,
    TokenValue,
    TransitionId,
};

// =============================================================================
// Helper: build a simple producer–consumer net used by many tests
// =============================================================================
//
//   p0 (buffer)         p1 (ready)
//     ^  \              /  |
//     |   \            /   |
//     |    t0 (produce)    t1 (consume)
//     |                    |
//     +--------------------+
//
// Tokens: p0 = 1, p1 = 0
// t0: p1 → p0 (weight 1)
// t1: p0 → p1 (weight 1)

fn build_producer_consumer() -> PetriNet {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("buffer".into(), None);
    let p1 = builder.add_place("ready".into(), None);
    let t0 = builder.add_transition("produce".into(), None);
    let t1 = builder.add_transition("consume".into(), None);
    // t0: p1 → p0
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
    // t1: p0 → p1
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 1);
    builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p1), 1);

    builder
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .expect("producer-consumer net should build")
}

// =============================================================================
// PetriNetBuilder
// =============================================================================

#[test]
fn test_builder_creates_correct_net() {
    let net = build_producer_consumer();

    assert_eq!(net.place_count(), 2);
    assert_eq!(net.transition_count(), 2);
    assert_eq!(net.arcs().len(), 4);

    // Place names
    assert_eq!(net.places()[0].name, "buffer");
    assert_eq!(net.places()[1].name, "ready");

    // Transition names
    assert_eq!(net.transitions()[0].name, "produce");
    assert_eq!(net.transitions()[1].name, "consume");

    // Initial marking
    let im = net.initial_marking();
    assert_eq!(im.get(0), Some(&TokenValue::Finite(1)));
    assert_eq!(im.get(1), Some(&TokenValue::Finite(0)));
}

#[test]
fn test_builder_add_place_returns_sequential_ids() {
    let mut builder = PetriNetBuilder::default();
    let a = builder.add_place("a".into(), None);
    let b = builder.add_place("b".into(), None);
    let c = builder.add_place("c".into(), None);
    assert_eq!(a, PlaceId(0));
    assert_eq!(b, PlaceId(1));
    assert_eq!(c, PlaceId(2));
}

#[test]
fn test_builder_add_transition_returns_sequential_ids() {
    let mut builder = PetriNetBuilder::default();
    let ta = builder.add_transition("ta".into(), None);
    let tb = builder.add_transition("tb".into(), None);
    assert_eq!(ta, TransitionId(0));
    assert_eq!(tb, TransitionId(1));
}

#[test]
fn test_builder_rejects_no_places() {
    let mut builder = PetriNetBuilder::default();
    builder.add_transition("t".into(), None);
    builder.add_arc(
        ArcEndpoint::Place(PlaceId(0)),
        ArcEndpoint::Transition(TransitionId(0)),
        1,
    );
    let err = builder
        .build(vec![])
        .expect_err("should reject net with no places");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("No places")));
}

#[test]
fn test_builder_rejects_no_transitions() {
    let mut builder = PetriNetBuilder::default();
    builder.add_place("p".into(), None);
    let err = builder
        .build(vec![TokenValue::Finite(0)])
        .expect_err("should reject net with no transitions");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("No transitions")));
}

#[test]
fn test_builder_rejects_no_arcs() {
    let mut builder = PetriNetBuilder::default();
    builder.add_place("p".into(), None);
    builder.add_transition("t".into(), None);
    let err = builder
        .build(vec![TokenValue::Finite(0)])
        .expect_err("should reject net with no arcs");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("No arcs")));
}

#[test]
fn test_validation_rejects_isolated_place() {
    let mut builder = PetriNetBuilder::default();
    let _isolated = builder.add_place("isolated".into(), None);
    let connected = builder.add_place("connected".into(), None);
    let t = builder.add_transition("t".into(), None);
    builder.add_arc(ArcEndpoint::Place(connected), ArcEndpoint::Transition(t), 1);

    let net = builder
        .build(vec![TokenValue::Finite(0), TokenValue::Finite(0)])
        .expect("basic builder validation should succeed");
    let err = racon_core::validation::validate_petri_net(&net)
        .expect_err("an isolated place is not a Petri-net node under Starke's definition");
    assert!(
        matches!(err, RaconError::StructuralViolation(msg) if msg.contains("Place") && msg.contains("no incident arc"))
    );
}

#[test]
fn test_validation_rejects_isolated_transition() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let connected = builder.add_transition("connected".into(), None);
    let _isolated = builder.add_transition("isolated".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(connected), 1);

    let net = builder
        .build(vec![TokenValue::Finite(0)])
        .expect("basic builder validation should succeed");
    let err = racon_core::validation::validate_petri_net(&net)
        .expect_err("an isolated transition is not a Petri-net node under Starke's definition");
    assert!(
        matches!(err, RaconError::StructuralViolation(msg) if msg.contains("Transition") && msg.contains("no incident arc"))
    );
}

#[test]
fn test_builder_rejects_wrong_marking_size() {
    let mut builder = PetriNetBuilder::default();
    builder.add_place("p0".into(), None);
    builder.add_place("p1".into(), None);
    let t = builder.add_transition("t".into(), None);
    builder.add_arc(
        ArcEndpoint::Place(PlaceId(0)),
        ArcEndpoint::Transition(t),
        1,
    );
    builder.add_arc(
        ArcEndpoint::Transition(t),
        ArcEndpoint::Place(PlaceId(1)),
        1,
    );
    // Only 1 token for 2 places
    let err = builder
        .build(vec![TokenValue::Finite(0)])
        .expect_err("should reject mismatched marking size");
    assert!(
        matches!(err, RaconError::InvalidInput(msg) if msg.contains("size 1 != place count 2"))
    );
}

#[test]
fn test_builder_rejects_omega_in_initial() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t = builder.add_transition("t".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    let err = builder
        .build(vec![TokenValue::Omega])
        .expect_err("should reject Omega in initial marking");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("Omega")));
}

#[test]
fn test_builder_rejects_zero_weight_arc() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t = builder.add_transition("t".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 0);
    let err = builder
        .build(vec![TokenValue::Finite(0)])
        .expect_err("should reject zero-weight arc");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("Arc weight must be > 0")));
}

#[test]
fn test_builder_rejects_duplicate_arc_endpoints() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t = builder.add_transition("t".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 2);

    let err = builder
        .build(vec![TokenValue::Finite(1)])
        .expect_err("duplicate arc endpoints must not produce ambiguous semantics");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("Duplicate arc")));
}

#[test]
fn test_dynamic_conflict_depends_on_available_tokens() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t1), 1);
    let net = builder
        .build(vec![TokenValue::Finite(1)])
        .expect("choice net should build");

    assert!(net.is_dynamic_conflict(t0, t1, net.initial_marking()));
    let two_tokens = Marking::from_finite(&[2]);
    assert!(!net.is_dynamic_conflict(t0, t1, &two_tokens));
}

#[test]
fn test_dynamic_conflict_supports_sets_and_multisets() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    let t1 = builder.add_transition("t1".into(), None);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t1), 1);
    let net = builder.build(vec![TokenValue::Finite(2)]).unwrap();

    assert!(net.is_concurrent_multiset(&[(t0, 1), (t1, 1)], net.initial_marking()));
    assert!(!net.is_concurrent_multiset(&[(t0, 2), (t1, 1)], net.initial_marking()));
    assert!(net.is_dynamic_conflict_multiset(&[(t0, 2), (t1, 1)], net.initial_marking()));
}

#[test]
fn test_builder_rejects_invalid_arc_endpoints() {
    let mut builder = PetriNetBuilder::default();
    let p = builder.add_place("p".into(), None);
    let _t = builder.add_transition("t".into(), None);
    // Place -> Place is invalid
    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Place(PlaceId(999)), 1);
    let err = builder
        .build(vec![TokenValue::Finite(0)])
        .expect_err("should reject Place→Place arc");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("Arc must connect")));
}

// =============================================================================
// PetriNet::preset / postset
// =============================================================================

#[test]
fn test_preset_and_postset() {
    let net = build_producer_consumer();

    // t0 (produce): preset = [p1(1)], postset = [p0(1)]
    let t0_pre = net.preset(TransitionId(0));
    assert_eq!(t0_pre.len(), 1);
    assert_eq!(t0_pre[0], (PlaceId(1), 1)); // p1 weight 1

    let t0_post = net.postset(TransitionId(0));
    assert_eq!(t0_post.len(), 1);
    assert_eq!(t0_post[0], (PlaceId(0), 1)); // p0 weight 1

    // t1 (consume): preset = [p0(1)], postset = [p1(1)]
    let t1_pre = net.preset(TransitionId(1));
    assert_eq!(t1_pre.len(), 1);
    assert_eq!(t1_pre[0], (PlaceId(0), 1));

    let t1_post = net.postset(TransitionId(1));
    assert_eq!(t1_post.len(), 1);
    assert_eq!(t1_post[0], (PlaceId(1), 1));
}

#[test]
fn test_preset_postset_empty_for_nonexistent_transition() {
    let net = build_producer_consumer();
    // TransitionId(99) was never added → empty slices
    assert!(net.preset(TransitionId(99)).is_empty());
    assert!(net.postset(TransitionId(99)).is_empty());
}

// =============================================================================
// PetriNet::is_enabled() and fire()
// =============================================================================

#[test]
fn test_is_enabled_returns_true_for_enabled_transition() {
    let net = build_producer_consumer();
    let m = net.initial_marking();
    // t0 (produce) needs p1 ≥ 1 → p1 = 0 → NOT enabled
    assert!(!net.is_enabled(TransitionId(0), &m));
    // t1 (consume) needs p0 ≥ 1 → p0 = 1 → enabled
    assert!(net.is_enabled(TransitionId(1), &m));
}

#[test]
fn test_fire_produces_correct_successor() {
    let net = build_producer_consumer();
    let m = net.initial_marking();
    // Fire t1 (consume): p0 loses 1, p1 gains 1
    let m2 = net.fire(TransitionId(1), &m);
    assert_eq!(m2.get(0), Some(&TokenValue::Finite(0)));
    assert_eq!(m2.get(1), Some(&TokenValue::Finite(1)));
}

#[test]
fn test_try_fire_rejects_disabled_transition_and_invalid_marking() {
    let net = build_producer_consumer();
    assert!(net
        .try_fire(TransitionId(0), net.initial_marking())
        .is_err());
    assert!(net
        .try_fire(TransitionId(1), &Marking::from_finite(&[1]))
        .is_err());
    assert!(!net.is_enabled(TransitionId(99), net.initial_marking()));
    assert!(net
        .try_fire(TransitionId(99), net.initial_marking())
        .is_err());
}

#[test]
fn test_fire_enables_other_transition() {
    let net = build_producer_consumer();
    let m0 = net.initial_marking();
    let m1 = net.fire(TransitionId(1), &m0);
    // After t1 fires: p0=0, p1=1 → t0 (produce) enabled now
    assert!(net.is_enabled(TransitionId(0), &m1));
    // t1 no longer enabled (p0 = 0)
    assert!(!net.is_enabled(TransitionId(1), &m1));
}

#[test]
fn test_fire_round_trip() {
    let net = build_producer_consumer();
    // m0 → t1 → m1 → t0 → m2 (should equal m0)
    let m0 = net.initial_marking();
    let m1 = net.fire(TransitionId(1), &m0);
    let m2 = net.fire(TransitionId(0), &m1);
    assert_eq!(&*m0, &m2, "firing consume then produce returns to initial");
}

#[test]
fn test_fire_with_weighted_arcs() {
    // Build a net: p0 -(2)→ t0 -(3)→ p1
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let p1 = builder.add_place("p1".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 2);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 3);
    let net = builder
        .build(vec![TokenValue::Finite(5), TokenValue::Finite(0)])
        .expect("weighted net should build");

    let m = net.initial_marking();
    assert!(net.is_enabled(TransitionId(0), &m)); // p0=5 ≥ 2

    let m2 = net.fire(TransitionId(0), &m);
    assert_eq!(m2.get(0), Some(&TokenValue::Finite(3))); // 5 - 2
    assert_eq!(m2.get(1), Some(&TokenValue::Finite(3))); // 0 + 3
}

#[test]
fn test_is_enabled_false_when_insufficient_tokens() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("p0".into(), None);
    let t0 = builder.add_transition("t0".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 5);
    let net = builder
        .build(vec![TokenValue::Finite(3)])
        .expect("net should build");

    let m = net.initial_marking();
    assert!(!net.is_enabled(TransitionId(0), &m)); // need 5, have 3
}

// =============================================================================
// Marking creation, get(), len(), is_empty()
// =============================================================================

#[test]
fn test_marking_new_and_from_finite() {
    let m1 = Marking::new(vec![
        TokenValue::Finite(3),
        TokenValue::Finite(7),
        TokenValue::Omega,
    ]);
    assert_eq!(m1.len(), 3);

    let m2 = Marking::from_finite(&[3, 7]);
    assert_eq!(m2.len(), 2);
    assert_eq!(m2.get(0), Some(&TokenValue::Finite(3)));
    assert_eq!(m2.get(1), Some(&TokenValue::Finite(7)));
}

#[test]
fn test_marking_get() {
    let m = Marking::from_finite(&[10, 20, 30]);
    assert_eq!(m.get(0), Some(&TokenValue::Finite(10)));
    assert_eq!(m.get(1), Some(&TokenValue::Finite(20)));
    assert_eq!(m.get(2), Some(&TokenValue::Finite(30)));
    assert_eq!(m.get(3), None); // out of bounds
}

#[test]
fn test_marking_empty() {
    let m = Marking::new(vec![]);
    assert!(m.is_empty());
    assert_eq!(m.len(), 0);

    let m2 = Marking::from_finite(&[]);
    assert!(m2.is_empty());
}

#[test]
fn test_marking_has_omega() {
    let m1 = Marking::from_finite(&[1, 2, 3]);
    assert!(!m1.has_omega());

    let m2 = Marking::new(vec![TokenValue::Finite(1), TokenValue::Omega]);
    assert!(m2.has_omega());

    let m3 = Marking::new(vec![TokenValue::Omega]);
    assert!(m3.has_omega());
}

// =============================================================================
// TokenValue — comparison (geq, gt)
// =============================================================================

#[test]
fn test_token_value_geq() {
    // Omega >= everything
    assert!(TokenValue::Omega.geq(&TokenValue::Omega));
    assert!(TokenValue::Omega.geq(&TokenValue::Finite(u64::MAX)));
    assert!(TokenValue::Omega.geq(&TokenValue::Finite(0)));

    // Finite >= Finite
    assert!(TokenValue::Finite(5).geq(&TokenValue::Finite(3)));
    assert!(TokenValue::Finite(3).geq(&TokenValue::Finite(3)));
    assert!(!TokenValue::Finite(2).geq(&TokenValue::Finite(5)));

    // Finite(anything) >= Omega → false
    assert!(!TokenValue::Finite(100).geq(&TokenValue::Omega));
}

#[test]
fn test_token_value_gt() {
    assert!(!TokenValue::Omega.gt(&TokenValue::Omega));
    assert!(TokenValue::Omega.gt(&TokenValue::Finite(0)));
    assert!(TokenValue::Omega.gt(&TokenValue::Finite(u64::MAX)));
    assert!(!TokenValue::Finite(0).gt(&TokenValue::Omega));
    assert!(!TokenValue::Finite(5).gt(&TokenValue::Finite(5)));
    assert!(TokenValue::Finite(5).gt(&TokenValue::Finite(3)));
}

// =============================================================================
// TokenValue — saturating arithmetic
// =============================================================================

#[test]
fn test_token_value_add_is_exact_beyond_u64() {
    // Finite + Finite
    assert_eq!(
        TokenValue::Finite(3).saturating_add(TokenValue::Finite(4)),
        TokenValue::Finite(7)
    );
    // Finite + Omega = Omega
    assert_eq!(
        TokenValue::Finite(5).saturating_add(TokenValue::Omega),
        TokenValue::Omega
    );
    // Omega + Finite = Omega
    assert_eq!(
        TokenValue::Omega.saturating_add(TokenValue::Finite(5)),
        TokenValue::Omega
    );
    // Saturation
    assert_eq!(
        TokenValue::Finite(u64::MAX)
            .saturating_add(TokenValue::Finite(1))
            .to_string(),
        "18446744073709551616"
    );
}

#[test]
fn test_token_value_saturating_sub() {
    // Finite - Finite
    assert_eq!(
        TokenValue::Finite(10).saturating_sub(TokenValue::Finite(3)),
        TokenValue::Finite(7)
    );
    // Saturating at 0
    assert_eq!(
        TokenValue::Finite(2).saturating_sub(TokenValue::Finite(5)),
        TokenValue::Finite(0)
    );
    // Omega - anything = Omega
    assert_eq!(
        TokenValue::Omega.saturating_sub(TokenValue::Finite(100)),
        TokenValue::Omega
    );
    // Finite - Omega = Finite(0)
    assert_eq!(
        TokenValue::Finite(50).saturating_sub(TokenValue::Omega),
        TokenValue::Finite(0)
    );
}

// =============================================================================
// TokenValue — operator overloads
// =============================================================================

#[test]
fn test_token_value_add_operator() {
    let a = TokenValue::Finite(10);
    let b = TokenValue::Finite(20);
    assert_eq!(a + b, TokenValue::Finite(30));

    let c = TokenValue::Finite(5);
    let d = TokenValue::Omega;
    assert_eq!(c + d, TokenValue::Omega);
}

#[test]
fn test_token_value_sub_operator() {
    let a = TokenValue::Finite(10);
    let b = TokenValue::Finite(3);
    assert_eq!(a - b, TokenValue::Finite(7));

    let c = TokenValue::Finite(3);
    let d = TokenValue::Finite(10);
    assert_eq!(c - d, TokenValue::Finite(0)); // saturating
}

// =============================================================================
// TokenValue — Display
// =============================================================================

#[test]
fn test_token_value_display_finite() {
    assert_eq!(format!("{}", TokenValue::Finite(0)), "0");
    assert_eq!(format!("{}", TokenValue::Finite(42)), "42");
    assert_eq!(
        format!("{}", TokenValue::Finite(u64::MAX)),
        u64::MAX.to_string()
    );
}

#[test]
fn test_token_value_display_omega() {
    assert_eq!(format!("{}", TokenValue::Omega), "ω");
}

// =============================================================================
// Marking — PartialOrd
// =============================================================================

#[test]
fn test_marking_partial_ord() {
    use std::cmp::Ordering;

    let m_a = Marking::from_finite(&[2, 3]);
    let m_b = Marking::from_finite(&[2, 3]);
    let m_c = Marking::from_finite(&[1, 2]);
    let m_d = Marking::from_finite(&[5, 1]);

    // Equal
    assert_eq!(m_a.partial_cmp(&m_b), Some(Ordering::Equal));

    // Greater (all components >= and at least one >)
    assert_eq!(m_a.partial_cmp(&m_c), Some(Ordering::Greater));

    // Less
    assert_eq!(m_c.partial_cmp(&m_a), Some(Ordering::Less));

    // Incomparable: [2,3] vs [5,1] → 2 < 5 but 3 > 1
    assert_eq!(m_a.partial_cmp(&m_d), None);

    // Different lengths → None
    let m_short = Marking::from_finite(&[1]);
    assert_eq!(m_a.partial_cmp(&m_short), None);
}

#[test]
fn test_marking_partial_ord_with_omega() {
    use std::cmp::Ordering;

    let m_omega = Marking::new(vec![TokenValue::Omega, TokenValue::Finite(0)]);
    let m_finite = Marking::from_finite(&[100, 0]);

    // Omega >= everything
    assert_eq!(m_omega.partial_cmp(&m_finite), Some(Ordering::Greater));
    assert_eq!(m_finite.partial_cmp(&m_omega), Some(Ordering::Less));
}

// =============================================================================
// IncidenceMatrix
// =============================================================================

#[test]
fn test_incidence_matrix_dimensions() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();
    assert_eq!(im.place_count(), 2);
    assert_eq!(im.transition_count(), 2);
    assert_eq!(im.data().len(), 2);
    assert_eq!(im.data()[0].len(), 2);
}

#[test]
fn test_incidence_matrix_data_values() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();

    // Producer–consumer net:
    // t0 (produce): p1 → p0 → post[0][0] = +1, pre[0][0] = 0 → C[0][0] = +1
    //                            pre[1][0] = 1, post[1][0] = 0 → C[1][0] = -1
    // t1 (consume): p0 → p1 → pre[0][1] = 1, post[0][1] = 0 → C[0][1] = -1
    //                            post[1][1] = 1, pre[1][1] = 0 → C[1][1] = +1
    assert_eq!(im.get(0, 0), Some(BigInt::from(1))); // p0 net +1 from t0
    assert_eq!(im.get(1, 0), Some(BigInt::from(-1))); // p1 net -1 from t0
    assert_eq!(im.get(0, 1), Some(BigInt::from(-1))); // p0 net -1 from t1
    assert_eq!(im.get(1, 1), Some(BigInt::from(1))); // p1 net +1 from t1
}

#[test]
fn test_incidence_matrix_get_out_of_bounds() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();
    assert_eq!(im.get(99, 0), None);
    assert_eq!(im.get(0, 99), None);
}

#[test]
fn test_incidence_matrix_row_and_column() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();

    let row0 = im.row(0).expect("row 0 should exist");
    assert_eq!(row0, &[BigInt::from(1), BigInt::from(-1)]);

    let col1 = im.column(1); // t1: effect on each place
    assert_eq!(col1, vec![BigInt::from(-1), BigInt::from(1)]);

    assert!(im.row(99).is_none());
}

#[test]
fn test_incidence_matrix_left_multiply() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();

    // y = [1, 1] → y^T * C = [1*1 + 1*(-1), 1*(-1) + 1*1] = [0, 0]
    let result = im.left_multiply(&[BigInt::from(1), BigInt::from(1)]);
    assert_eq!(result, vec![BigInt::from(0), BigInt::from(0)]);
}

#[test]
fn test_incidence_matrix_right_multiply() {
    let net = build_producer_consumer();
    let im = net.incidence_matrix();

    // x = [1, 1] → C * x = [1*1 + (-1)*1, (-1)*1 + 1*1] = [0, 0]
    let result = im.right_multiply(&[BigInt::from(1), BigInt::from(1)]);
    assert_eq!(result, vec![BigInt::from(0), BigInt::from(0)]);
}

#[test]
fn test_incidence_matrix_preserves_bigint_algebra_without_i64_narrowing() {
    let net = build_producer_consumer();
    let matrix = net.incidence_matrix();
    let huge: BigInt = (BigInt::from(i64::MAX) + 1) * 3;

    assert_eq!(
        matrix.left_multiply(&[huge.clone(), huge.clone()]),
        vec![0.into(), 0.into()]
    );
    assert_eq!(
        matrix.right_multiply(&[huge.clone(), BigInt::from(0)]),
        vec![huge.clone(), -huge]
    );
}

// =============================================================================
// NodeId — ordering and Display
// =============================================================================

#[test]
fn test_node_id_ordering() {
    let n0 = NodeId(0);
    let n1 = NodeId(1);
    let n2 = NodeId(2);

    assert!(n0 < n1);
    assert!(n1 < n2);
    assert!(n0 < n2);
    assert_eq!(n1, NodeId(1));

    // Sortable
    let mut ids = vec![NodeId(3), NodeId(1), NodeId(2)];
    ids.sort();
    assert_eq!(ids, vec![NodeId(1), NodeId(2), NodeId(3)]);
}

#[test]
fn test_node_id_max() {
    assert_eq!(NodeId::MAX, NodeId(u32::MAX));
}

#[test]
fn test_node_id_display() {
    assert_eq!(format!("{}", NodeId(0)), "n0");
    assert_eq!(format!("{}", NodeId(42)), "n42");
    assert_eq!(format!("{}", NodeId(u32::MAX)), format!("n{}", u32::MAX));
}

#[test]
fn test_edge_id_display() {
    assert_eq!(format!("{}", EdgeId(0)), "e0");
    assert_eq!(format!("{}", EdgeId(7)), "e7");
}

// =============================================================================
// AnalysisConfig — default values
// =============================================================================

#[test]
fn test_analysis_config_default() {
    let config = AnalysisConfig::default();
    assert_eq!(config.max_memory_mb, 4096);
    assert_eq!(config.cancellation_timeout.as_secs(), 30);
    assert_eq!(config.progress_interval.as_millis(), 100);
    assert_eq!(config.graph_expansion_levels, 5);
    // thread_count depends on host; just verify it's > 0
    assert!(config.thread_count > 0);
}

// =============================================================================
// CancellationToken
// =============================================================================

#[test]
fn test_cancellation_token_new_not_cancelled() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn test_cancellation_token_cancel() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn test_cancellation_token_clone_shares_state() {
    let token = CancellationToken::new();
    let clone = token.clone();
    token.cancel();
    // Both original and clone observe the cancellation
    assert!(clone.is_cancelled());
    assert!(token.is_cancelled());
}

#[test]
fn test_cancellation_token_default_equals_new() {
    let a = CancellationToken::default();
    let b = CancellationToken::new();
    assert!(!a.is_cancelled());
    assert!(!b.is_cancelled());
}

// =============================================================================
// MemoryBudget
// =============================================================================

#[test]
fn test_memory_budget_allocate_and_deallocate() {
    let budget = MemoryBudget::new(1024 * 1024); // 1 MB
    assert!(budget.allocate(100).is_ok());
    assert_eq!(budget.current_mb(), 0); // 100 bytes < 1 MB
    budget.deallocate(100);
    // After full cycle, current should be 0
}

#[test]
fn test_memory_budget_overflow_error() {
    let budget = MemoryBudget::new(500); // 500 bytes limit

    // Allocate 400 — should succeed
    assert!(budget.allocate(400).is_ok());

    // Allocate 200 more — would exceed 500 → should fail
    let err = budget.allocate(200).expect_err("should exceed budget");
    assert!(matches!(
        err,
        MemoryError::LimitExceeded {
            required: 200,
            limit: 500
        }
    ));
}

#[test]
fn test_memory_budget_exact_limit() {
    let budget = MemoryBudget::new(100);
    assert!(budget.allocate(100).is_ok());
    // Exactly at limit — next allocation fails
    let err = budget.allocate(1).expect_err("should exceed budget");
    assert!(matches!(err, MemoryError::LimitExceeded { .. }));
}

#[test]
fn test_memory_budget_deallocate_reclaims_space() {
    let budget = MemoryBudget::new(200);
    assert!(budget.allocate(150).is_ok());
    budget.deallocate(100); // free 100
    assert!(budget.allocate(100).is_ok()); // 50 used + 100 = 150 ≤ 200
}

#[test]
fn test_memory_budget_zero_limit() {
    let budget = MemoryBudget::new(0);
    let err = budget
        .allocate(1)
        .expect_err("zero limit should reject all");
    assert!(matches!(err, MemoryError::LimitExceeded { .. }));
}

#[test]
fn test_memory_budget_limit_mb() {
    let budget = MemoryBudget::new(5 * 1024 * 1024); // 5 MB
    assert_eq!(budget.limit_mb(), 5);
}

#[test]
fn test_memory_budget_current_mb() {
    let budget = MemoryBudget::new(10 * 1024 * 1024); // 10 MB
    assert_eq!(budget.current_mb(), 0);
    // Allocate 3 MB
    budget.allocate(3 * 1024 * 1024).unwrap();
    assert_eq!(budget.current_mb(), 3);
    budget.deallocate(1 * 1024 * 1024);
    assert_eq!(budget.current_mb(), 2);
}

// =============================================================================
// MemoryError — Display and Error trait
// =============================================================================

#[test]
fn test_memory_error_display() {
    let err = MemoryError::LimitExceeded {
        required: 500,
        limit: 100,
    };
    let msg = format!("{}", err);
    assert!(
        msg.contains("500"),
        "message should contain required bytes: {}",
        msg
    );
    assert!(
        msg.contains("100"),
        "message should contain limit bytes: {}",
        msg
    );
}

#[test]
fn test_memory_error_is_error() {
    use std::error::Error;
    let err = MemoryError::LimitExceeded {
        required: 999,
        limit: 100,
    };
    // Must implement std::error::Error
    assert!(err.source().is_none());
}

// =============================================================================
// AnalysisContext
// =============================================================================

#[test]
fn test_analysis_context_creation() {
    let config = AnalysisConfig::default();
    let ctx = AnalysisContext::new(config.clone());

    // Config accessible
    assert_eq!(ctx.config.max_memory_mb, config.max_memory_mb);

    // Fresh cancellation token
    assert!(!ctx.cancellation.is_cancelled());

    // Memory budget matches config
    assert_eq!(ctx.memory_budget.limit_mb(), config.max_memory_mb);
}

#[test]
fn test_analysis_context_check_cancellation() {
    let config = AnalysisConfig::default();
    let ctx = AnalysisContext::new(config);

    // Not cancelled yet
    assert!(ctx.check_cancellation().is_ok());

    // Cancel and check
    ctx.cancellation.cancel();
    assert!(ctx.check_cancellation().is_err());
}

// =============================================================================
// AnalysisEngine trait — compile-time check
// =============================================================================

/// Dummy engine to verify the AnalysisEngine trait is object-safe enough for
/// our needs and compiles correctly.
struct DummyEngine;

impl AnalysisEngine for DummyEngine {
    type Input = String;
    type Output = usize;
    type Error = std::io::Error;

    fn analyze(
        &self,
        input: Self::Input,
        _context: &AnalysisContext,
    ) -> Result<Self::Output, Self::Error> {
        Ok(input.len())
    }
}

#[test]
fn test_dummy_engine_via_trait() {
    let engine = DummyEngine;
    let config = AnalysisConfig::default();
    let ctx = AnalysisContext::new(config);
    let result = engine.analyze("hello".into(), &ctx);
    assert_eq!(result.unwrap(), 5);
}

// =============================================================================
// PetriNet: places(), transitions(), arcs() return correct counts
// =============================================================================

#[test]
fn test_net_place_ids_are_correct() {
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("alpha".into(), Some("A".into()));
    let p1 = builder.add_place("beta".into(), None);
    let _p2 = builder.add_place("gamma".into(), Some("G".into()));
    let t0 = builder.add_transition("tx".into(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
    builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(_p2), 1);
    let net = builder
        .build(vec![
            TokenValue::Finite(0),
            TokenValue::Finite(0),
            TokenValue::Finite(0),
        ])
        .expect("net should build");

    assert_eq!(net.place_count(), 3);
    assert_eq!(net.transition_count(), 1);
    assert_eq!(net.places()[0].label, Some("A".into()));
    assert_eq!(net.places()[1].label, None);
    assert_eq!(net.places()[2].label, Some("G".into()));
}

#[test]
fn test_net_arcs_iteration() {
    let net = build_producer_consumer();
    for arc in net.arcs() {
        assert!(arc.weight > 0, "all arcs must have positive weight");
        match (&arc.source, &arc.target) {
            (ArcEndpoint::Place(_), ArcEndpoint::Transition(_))
            | (ArcEndpoint::Transition(_), ArcEndpoint::Place(_)) => {}
            _ => panic!("arc must connect Place↔Transition"),
        }
    }
}

// =============================================================================
// Place / Transition / Arc type properties
// =============================================================================

#[test]
fn test_place_id_copy_trait() {
    let a = PlaceId(42);
    let b = a; // Copy
    assert_eq!(a, b);
}

#[test]
fn test_transition_id_copy_trait() {
    let a = TransitionId(7);
    let b = a; // Copy
    assert_eq!(a, b);
}

#[test]
fn test_arc_endpoint_copy_trait() {
    let ep = ArcEndpoint::Place(PlaceId(1));
    let ep2 = ep; // Copy
    assert_eq!(ep, ep2);
}

// =============================================================================
// Validation module (integration-style)
// =============================================================================

#[test]
fn test_validate_petri_net_ok() {
    let net = build_producer_consumer();
    assert!(racon_core::validation::validate_petri_net(&net).is_ok());
}

#[test]
fn test_validate_marking_ok() {
    let net = build_producer_consumer();
    let m = Marking::from_finite(&[5, 3]);
    assert!(racon_core::validation::validate_marking(&m, &net).is_ok());
}

#[test]
fn test_validate_marking_wrong_size() {
    let net = build_producer_consumer();
    let m = Marking::from_finite(&[1]); // too short
    let err =
        racon_core::validation::validate_marking(&m, &net).expect_err("should reject wrong size");
    assert!(matches!(err, RaconError::InvalidInput(_)));
}

#[test]
fn test_validate_marking_rejects_omega() {
    let net = build_producer_consumer();
    let m = Marking::new(vec![TokenValue::Finite(1), TokenValue::Omega]);
    let err = racon_core::validation::validate_marking(&m, &net).expect_err("should reject omega");
    assert!(matches!(err, RaconError::InvalidInput(msg) if msg.contains("Omega")));
}

// =============================================================================
// StateSpace trait — compile-time check
// =============================================================================

#[test]
fn test_state_space_trait_object_safe() {
    // Just verify the trait compiles when referenced
    fn _takes_state_space(_s: &dyn racon_core::StateSpace) -> usize {
        _s.node_count()
    }
}
