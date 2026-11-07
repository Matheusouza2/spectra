//! Tests for the `racon-extended-pt-v1` semantics: inhibitor arcs and
//! immediate-over-normal precedence (spec cases INH-*, IMM-*, COMP-01).

use racon_core::{
    ArcEndpoint, Marking, PetriNet, PetriNetBuilder, PlaceId, TokenValue, TransitionId,
    TransitionKind,
};

const P: ArcEndpoint = ArcEndpoint::Place(PlaceId(0));
const Q: ArcEndpoint = ArcEndpoint::Place(PlaceId(1));
fn t(i: u32) -> ArcEndpoint {
    ArcEndpoint::Transition(TransitionId(i))
}

/// Net with places p0 (tested place) and p1 (sink), one transition t0 that
/// consumes `pre` tokens from p0 (if >0) and an inhibitor `(p0, t0, k)`.
fn inhibitor_net(pre: u32, threshold: Option<u64>, tokens: u64) -> PetriNet {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    b.add_place("p1".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    if pre > 0 {
        b.add_arc(P, t(0), pre);
    }
    b.add_arc(t(0), Q, 1);
    if let Some(k) = threshold {
        b.add_inhibitor_arc(PlaceId(0), t0, k);
    }
    b.build(vec![TokenValue::Finite(tokens), TokenValue::Finite(0)])
        .unwrap()
}

fn m(tokens: &[u64]) -> Marking {
    Marking::from_finite(tokens)
}

#[test]
fn inh_01_threshold_one_enabled_when_empty() {
    let net = inhibitor_net(0, Some(1), 0);
    assert!(net.is_basic_enabled(TransitionId(0), &m(&[0, 0])));
    assert!(net.is_enabled(TransitionId(0), &m(&[0, 0])));
}

#[test]
fn inh_02_threshold_one_disabled_when_marked() {
    let net = inhibitor_net(0, Some(1), 1);
    assert!(!net.is_enabled(TransitionId(0), &m(&[1, 0])));
}

#[test]
fn inh_03_04_threshold_three_is_strict() {
    let net = inhibitor_net(0, Some(3), 0);
    assert!(net.is_enabled(TransitionId(0), &m(&[2, 0])));
    assert!(!net.is_enabled(TransitionId(0), &m(&[3, 0])));
    assert!(!net.is_enabled(TransitionId(0), &m(&[4, 0])));
}

#[test]
fn inh_05_pre_below_threshold_only_exact_marking_enables() {
    let net = inhibitor_net(2, Some(3), 2);
    for tokens in 0..6u64 {
        assert_eq!(
            net.is_enabled(TransitionId(0), &m(&[tokens, 0])),
            tokens == 2,
            "marking {tokens}"
        );
    }
}

#[test]
fn inh_06_pre_equal_threshold_is_never_enabled() {
    let net = inhibitor_net(3, Some(3), 0);
    for tokens in 0..8u64 {
        assert!(!net.is_enabled(TransitionId(0), &m(&[tokens, 0])));
    }
}

#[test]
fn inh_07_inhibitor_never_changes_incidence_or_tokens() {
    let with = inhibitor_net(1, Some(5), 1);
    let without = inhibitor_net(1, None, 1);
    assert_eq!(
        with.incidence_matrix().data(),
        without.incidence_matrix().data()
    );
    let before = m(&[1, 0]);
    assert_eq!(
        with.fire(TransitionId(0), &before),
        without.fire(TransitionId(0), &before)
    );
    // Inhibitor-only transition: firing changes only the output place.
    let only = inhibitor_net(0, Some(1), 0);
    assert_eq!(only.fire(TransitionId(0), &m(&[0, 0])), m(&[0, 1]));
}

#[test]
fn inhibitor_arc_with_zero_threshold_is_rejected() {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    b.add_arc(t(0), P, 1);
    b.add_inhibitor_arc(PlaceId(0), t0, 0);
    assert!(b.build(vec![TokenValue::Finite(0)]).is_err());
}

#[test]
fn duplicate_inhibitors_keep_the_smallest_threshold() {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    let t0 = b.add_transition("t0".into(), None);
    b.add_arc(t(0), P, 1);
    b.add_inhibitor_arc(PlaceId(0), t0, 4);
    b.add_inhibitor_arc(PlaceId(0), t0, 2);
    let net = b.build(vec![TokenValue::Finite(0)]).unwrap();
    assert_eq!(net.inhibitors(t0), &[(PlaceId(0), 2)]);
    assert_eq!(net.inhibitor_arcs().len(), 1);
}

#[test]
fn omega_never_passes_an_inhibitor_test() {
    let net = inhibitor_net(0, Some(1000), 0);
    let omega = Marking::new(vec![TokenValue::Omega, TokenValue::Finite(0)]);
    assert!(!net.is_basic_enabled(TransitionId(0), &omega));
}

/// p0 -> t_imm -> p1 ; p0 -> t_norm -> p1 ; optional inhibitor on t_imm.
fn precedence_net(inhibit_imm_on_p2: bool, p2_tokens: u64) -> PetriNet {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    b.add_place("p1".into(), None);
    let p2 = b.add_place("p2".into(), None);
    let ti = b.add_transition("imm".into(), None);
    let tn = b.add_transition("norm".into(), None);
    b.add_arc(P, t(0), 1);
    b.add_arc(t(0), Q, 1);
    b.add_arc(P, t(1), 1);
    b.add_arc(t(1), Q, 1);
    b.add_arc(ArcEndpoint::Place(p2), t(1), 0 + 1);
    b.add_arc(t(1), ArcEndpoint::Place(p2), 1);
    b.set_transition_kind(ti, TransitionKind::Immediate);
    if inhibit_imm_on_p2 {
        b.add_inhibitor_arc(p2, ti, 1);
    }
    let _ = tn;
    b.build(vec![
        TokenValue::Finite(1),
        TokenValue::Finite(0),
        TokenValue::Finite(p2_tokens),
    ])
    .unwrap()
}

#[test]
fn imm_01_immediate_preempts_normal() {
    let net = precedence_net(false, 1);
    let marking = net.initial_marking().clone();
    assert!(net.is_basic_enabled(TransitionId(1), &marking));
    assert_eq!(net.fireable_transitions(&marking), vec![TransitionId(0)]);
    assert!(!net.is_enabled(TransitionId(1), &marking));
    assert_eq!(net.preempted_transitions(&marking), vec![TransitionId(1)]);
}

#[test]
fn imm_02_normal_fireable_when_no_immediate_enabled() {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    b.add_place("p1".into(), None);
    b.add_transition("imm".into(), None);
    b.add_transition("norm".into(), None);
    b.add_place("gate".into(), None);
    b.add_arc(ArcEndpoint::Place(PlaceId(2)), t(0), 1); // imm needs gate, empty
    b.add_arc(t(0), Q, 1);
    b.add_arc(P, t(1), 1);
    b.add_arc(t(1), Q, 1);
    b.set_transition_kind(TransitionId(0), TransitionKind::Immediate);
    let net = b
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(0),
        ])
        .unwrap();
    assert_eq!(
        net.fireable_transitions(net.initial_marking()),
        vec![TransitionId(1)]
    );
}

#[test]
fn imm_03_two_immediates_are_both_successors() {
    let mut b = PetriNetBuilder::default();
    b.add_place("p0".into(), None);
    b.add_place("p1".into(), None);
    b.add_transition("a".into(), None);
    b.add_transition("b".into(), None);
    b.add_arc(P, t(0), 1);
    b.add_arc(t(0), Q, 1);
    b.add_arc(P, t(1), 1);
    b.add_arc(t(1), Q, 2);
    b.set_transition_kind(TransitionId(0), TransitionKind::Immediate);
    b.set_transition_kind(TransitionId(1), TransitionKind::Immediate);
    let net = b
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .unwrap();
    assert_eq!(
        net.fireable_transitions(net.initial_marking()),
        vec![TransitionId(0), TransitionId(1)]
    );
}

#[test]
fn imm_04_immediate_blocked_by_inhibitor_lets_normal_fire() {
    let net = precedence_net(true, 1);
    let marking = net.initial_marking().clone();
    assert!(!net.is_basic_enabled(TransitionId(0), &marking));
    assert_eq!(net.fireable_transitions(&marking), vec![TransitionId(1)]);
}

#[test]
fn comp_01_legacy_net_semantics_unchanged() {
    let net = inhibitor_net(1, None, 1);
    assert!(!net.is_extended());
    assert!(net.is_enabled(TransitionId(0), &m(&[1, 0])));
    assert!(!net.is_enabled(TransitionId(0), &m(&[0, 0])));
    assert_eq!(
        net.fireable_transitions(&m(&[1, 0])),
        vec![TransitionId(0)]
    );
}

#[test]
fn fire_always_equals_marking_plus_incidence_column() {
    let net = precedence_net(true, 0);
    let marking = net.initial_marking().clone();
    for t in net.fireable_transitions(&marking) {
        let next = net.fire(t, &marking);
        let column = net.incidence_matrix().column(t.0 as usize);
        for (idx, token) in next.tokens.iter().enumerate() {
            let before = match &marking.tokens[idx] {
                TokenValue::Finite(n) => *n as i64,
                _ => unreachable!(),
            };
            let after = match token {
                TokenValue::Finite(n) => *n as i64,
                _ => unreachable!(),
            };
            let delta: i64 = column[idx].clone().try_into().unwrap();
            assert_eq!(after, before + delta);
        }
    }
}

#[test]
fn dynamic_conflict_detects_inhibitor_induced_conflict() {
    // a: p0 -> p1 ; b: needs p0 empty-of-p1 (inhibited by p1). Firing a disables b.
    let mut bld = PetriNetBuilder::default();
    bld.add_place("p0".into(), None);
    bld.add_place("p1".into(), None);
    bld.add_place("p2".into(), None);
    let a = bld.add_transition("a".into(), None);
    let bt = bld.add_transition("b".into(), None);
    bld.add_arc(P, t(0), 1);
    bld.add_arc(t(0), Q, 1);
    bld.add_arc(ArcEndpoint::Place(PlaceId(2)), t(1), 1);
    bld.add_arc(t(1), ArcEndpoint::Place(PlaceId(2)), 1);
    bld.add_inhibitor_arc(PlaceId(1), bt, 1);
    let net = bld
        .build(vec![
            TokenValue::Finite(1),
            TokenValue::Finite(0),
            TokenValue::Finite(1),
        ])
        .unwrap();
    // No shared input place, yet a and b are in dynamic conflict.
    assert!(net.is_dynamic_conflict(a, bt, net.initial_marking()));
}
