//! Conformance tests for the Thierry-Mieg structural rules (R8).
//!
//! Oracle: exhaustive exploration of the original and of the reduced net.
//! * Safety: the set of reachable markings projected on the support is equal.
//! * Deadlock: a reachable deadlock exists in one iff it exists in the other.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue};
use racon_reduction::thierry_mieg::{reduce, ReductionGoal, ThierryMiegConfig};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

fn build(pre: &[Vec<u32>], post: &[Vec<u32>], m0: &[u64]) -> Option<PetriNet> {
    let mut b = PetriNetBuilder::default();
    for p in 0..m0.len() {
        b.add_place(format!("p{p}"), None);
    }
    let mut arcs = 0;
    for t in 0..pre.len() {
        let tid = b.add_transition(format!("t{t}"), None);
        for p in 0..m0.len() {
            if pre[t][p] > 0 {
                b.add_arc(ArcEndpoint::Place(PlaceId(p as u32)), ArcEndpoint::Transition(tid), pre[t][p]);
                arcs += 1;
            }
            if post[t][p] > 0 {
                b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(p as u32)), post[t][p]);
                arcs += 1;
            }
        }
    }
    if arcs == 0 {
        return None;
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect()).ok()
}

/// (markings reachable, has deadlock) or `None` when the cap is exceeded.
fn explore(net: &PetriNet, cap: usize) -> Option<(BTreeSet<Vec<u64>>, bool)> {
    let m0: Vec<u64> = net
        .initial_marking()
        .tokens
        .iter()
        .map(|t| match t {
            TokenValue::Finite(n) => *n,
            _ => unreachable!(),
        })
        .collect();
    explore_raw(&m0, net.transitions().len(), |t| {
        (
            net.preset(net.transitions()[t].id).iter().map(|&(p, w)| (p.0 as usize, u64::from(w))).collect(),
            net.postset(net.transitions()[t].id).iter().map(|&(p, w)| (p.0 as usize, u64::from(w))).collect(),
        )
    }, cap)
}

fn explore_raw(
    m0: &[u64],
    transitions: usize,
    arcs: impl Fn(usize) -> (Vec<(usize, u64)>, Vec<(usize, u64)>),
    cap: usize,
) -> Option<(BTreeSet<Vec<u64>>, bool)> {
    let arcs: Vec<_> = (0..transitions).map(&arcs).collect();
    let mut seen = BTreeSet::from([m0.to_vec()]);
    let mut queue = VecDeque::from([m0.to_vec()]);
    let mut deadlock = false;
    while let Some(m) = queue.pop_front() {
        let mut any = false;
        for (pre, post) in &arcs {
            if pre.iter().all(|&(p, w)| m[p] >= w) {
                any = true;
                let mut n = m.clone();
                for &(p, w) in pre {
                    n[p] -= w;
                }
                for &(p, w) in post {
                    n[p] += w;
                }
                if seen.insert(n.clone()) {
                    if seen.len() > cap {
                        return None;
                    }
                    queue.push_back(n);
                }
            }
        }
        deadlock |= !any;
    }
    Some((seen, deadlock))
}

fn random_net(rng: &mut Lcg) -> (Vec<Vec<u32>>, Vec<Vec<u32>>, Vec<u64>) {
    let places = 2 + rng.next(5) as usize;
    let transitions = 2 + rng.next(6) as usize;
    let mut pre = vec![vec![0u32; places]; transitions];
    let mut post = vec![vec![0u32; places]; transitions];
    for t in 0..transitions {
        for p in 0..places {
            if rng.next(100) < 28 {
                pre[t][p] = 1 + (rng.next(10) / 8) as u32;
            }
            if rng.next(100) < 28 {
                post[t][p] = 1 + (rng.next(10) / 8) as u32;
            }
        }
    }
    let m0 = (0..places).map(|_| if rng.next(100) < 45 { 0 } else { 1 + rng.next(3) }).collect();
    (pre, post, m0)
}

#[test]
fn rules_preserve_safety_projection_and_deadlocks_on_random_nets() {
    let mut rng = Lcg(0xABCD_1234);
    let cfg = ThierryMiegConfig::default();
    let mut coverage: BTreeMap<u8, usize> = BTreeMap::new();
    let mut checked = 0;
    for case in 0..3000 {
        let (pre, post, m0) = random_net(&mut rng);
        let Some(net) = build(&pre, &post, &m0) else { continue };
        let Some((orig_states, orig_deadlock)) = explore(&net, 4_000) else { continue };
        let places = m0.len();
        let support: Vec<PlaceId> =
            (0..places).filter(|_| rng.next(100) < 35).map(|p| PlaceId(p as u32)).collect();

        for goal in [ReductionGoal::Deadlock, ReductionGoal::Safety { support: support.clone() }] {
            let result = reduce(&net, &goal, &cfg).expect("plain net");
            for (rule, n) in &result.applications {
                *coverage.entry(*rule).or_insert(0) += n;
            }
            let names = &result.remaining_places;
            let (states, deadlock) = match &result.reduced_net {
                Some(reduced) => explore(reduced, 100_000).expect("reduction must not blow up"),
                None => {
                    let dead = result.remaining_transitions.is_empty();
                    (BTreeSet::from([result.initial_marking.clone()]), dead)
                }
            };
            match &goal {
                ReductionGoal::Deadlock => {
                    let expected = orig_deadlock;
                    let got = if result.deadlock_free { false } else { deadlock };
                    assert_eq!(got, expected, "case {case}: deadlock verdict changed ({:?})", result.applications);
                }
                ReductionGoal::Safety { support } => {
                    let idx: Vec<usize> = support.iter().map(|p| p.0 as usize).collect();
                    let project_orig: BTreeSet<Vec<u64>> =
                        orig_states.iter().map(|m| idx.iter().map(|&p| m[p]).collect()).collect();
                    let positions: Vec<usize> = idx
                        .iter()
                        .map(|p| {
                            names
                                .iter()
                                .position(|n| *n == format!("p{p}"))
                                .expect("support places are never removed")
                        })
                        .collect();
                    let project_red: BTreeSet<Vec<u64>> =
                        states.iter().map(|m| positions.iter().map(|&q| m[q]).collect()).collect();
                    assert_eq!(
                        project_red, project_orig,
                        "case {case}: projection on {idx:?} changed ({:?}, {:?})",
                        result.applications, result.agglomerations
                    );
                }
            }
            checked += 1;
        }
    }
    println!("checked={checked} coverage={coverage:?}");
    assert!(checked > 1500);
    for rule in [1u8, 2, 4, 5, 6, 7, 8, 9, 10, 11, 14, 15, 16, 17] {
        assert!(coverage.get(&rule).copied().unwrap_or(0) > 0, "rule {rule} never fired: {coverage:?}");
    }
}

#[test]
fn pre_agglomeration_fuses_a_two_input_feeder_with_its_consumer() {
    // h = t0 consumes p0 and p1 and feeds p2; f = t1 moves p2 to p3 (support).
    // Two inputs keep rule 17 from firing first; p2 is unmarked and agglomerable.
    let net = build(
        &[vec![1, 1, 0, 0], vec![0, 0, 1, 0]],
        &[vec![0, 0, 1, 0], vec![0, 0, 0, 1]],
        &[1, 2, 0, 0],
    )
    .unwrap();
    let r = reduce(&net, &ReductionGoal::Safety { support: vec![PlaceId(3)] }, &ThierryMiegConfig::default()).unwrap();
    assert_eq!(r.applications.get(&14), Some(&1), "{:?}", r.applications);
    assert!(r.agglomerations.iter().any(|a| a.place == "p2" && a.new_transitions == vec!["t0.t1".to_owned()]));
    let (states, _) = explore(r.reduced_net.as_ref().expect("net remains"), 100).unwrap();
    assert!(states.iter().any(|m| m.last() == Some(&1)), "{states:?}");
}

#[test]
fn source_transition_proves_deadlock_freedom() {
    let net = build(&[vec![0, 0], vec![1, 0]], &[vec![1, 0], vec![0, 1]], &[0, 0]).unwrap();
    let r = reduce(&net, &ReductionGoal::Deadlock, &ThierryMiegConfig::default()).unwrap();
    assert!(r.deadlock_free);
    assert!(r.reduced_net.is_none());
    assert_eq!(r.remaining_transitions, vec!["t0".to_owned()]);
}

#[test]
fn extended_nets_are_refused() {
    // Smoke check on the error path with an out-of-range support place.
    let net = build(&[vec![1]], &[vec![1]], &[1]).unwrap();
    let err = reduce(&net, &ReductionGoal::Safety { support: vec![PlaceId(9)] }, &ThierryMiegConfig::default());
    assert!(err.is_err());
}
