//! Conformance tests for the ERV unfolding prefix (R12).
//!
//! Oracle: exhaustive exploration of the net.
//! * 1-safe net: the markings of the cuts of the prefix are exactly the
//!   reachable markings; transitions labelling no event are exactly the dead
//!   ones; a deadlock is found iff one is reachable; both adequate orders give
//!   complete prefixes and ERV's is never larger than McMillan's.
//! * non-1-safe net: unfolding fails instead of returning a wrong prefix.

use std::collections::{BTreeSet, VecDeque};

use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue};
use racon_reduction::unfolding::{unfold, AdequateOrder, UnfoldingConfig};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

type Arcs = Vec<(Vec<usize>, Vec<usize>)>;

fn build(arcs: &Arcs, m0: &[u64]) -> Option<PetriNet> {
    let mut b = PetriNetBuilder::default();
    for p in 0..m0.len() {
        b.add_place(format!("p{p}"), None);
    }
    for (t, (pre, post)) in arcs.iter().enumerate() {
        let tid = b.add_transition(format!("t{t}"), None);
        for &p in pre {
            b.add_arc(ArcEndpoint::Place(PlaceId(p as u32)), ArcEndpoint::Transition(tid), 1);
        }
        for &p in post {
            b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(p as u32)), 1);
        }
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect()).ok()
}

/// Reachable markings, or `None` over the cap.
fn reach(arcs: &Arcs, m0: &[u64], cap: usize) -> Option<BTreeSet<Vec<u32>>> {
    let start: Vec<u32> = m0.iter().map(|&x| x as u32).collect();
    let mut seen = BTreeSet::from([start.clone()]);
    let mut queue = VecDeque::from([start]);
    while let Some(m) = queue.pop_front() {
        for (pre, post) in arcs {
            if pre.iter().all(|&p| m[p] >= 1) {
                let mut n = m.clone();
                for &p in pre {
                    n[p] -= 1;
                }
                for &p in post {
                    n[p] += 1;
                }
                if seen.insert(n.clone()) {
                    if seen.len() > cap {
                        return None;
                    }
                    queue.push_back(n);
                }
            }
        }
    }
    Some(seen)
}

fn random_net(rng: &mut Lcg) -> (Arcs, Vec<u64>) {
    let places = 3 + rng.next(5) as usize;
    let transitions = 2 + rng.next(6) as usize;
    let mut arcs: Arcs = Vec::new();
    for _ in 0..transitions {
        let k = 1 + rng.next(2) as usize;
        let pick = |rng: &mut Lcg| {
            let mut v = BTreeSet::new();
            while v.len() < k {
                v.insert(rng.next(places as u64) as usize);
            }
            v.into_iter().collect::<Vec<_>>()
        };
        let pre = pick(rng);
        let post = pick(rng);
        arcs.push((pre, post));
    }
    let mut m0 = vec![0u64; places];
    for _ in 0..(1 + rng.next(3)) {
        m0[rng.next(places as u64) as usize] = 1;
    }
    (arcs, m0)
}

#[test]
fn prefix_is_complete_and_erv_is_not_larger_than_mcmillan() {
    let mut rng = Lcg(0xE5BA_2A01);
    let (mut safe, mut unsafe_, mut smaller) = (0, 0, 0);
    let (mut erv_events, mut mcm_events, mut cutoffs) = (0usize, 0usize, 0usize);
    for case in 0..4000 {
        let (arcs, m0) = random_net(&mut rng);
        let Some(net) = build(&arcs, &m0) else { continue };
        let Some(reachable) = reach(&arcs, &m0, 20_000) else { continue };
        let is_safe = reachable.iter().all(|m| m.iter().all(|&x| x <= 1));
        let erv = unfold(&net, &UnfoldingConfig::default());
        let mcm = unfold(&net, &UnfoldingConfig { order: AdequateOrder::McMillan, ..Default::default() });
        if !is_safe {
            assert!(erv.is_err() && mcm.is_err(), "case {case}: unsafe net unfolded");
            unsafe_ += 1;
            continue;
        }
        safe += 1;
        let erv = erv.unwrap_or_else(|e| panic!("case {case}: {e}"));
        let mcm = mcm.unwrap_or_else(|e| panic!("case {case}: {e}"));
        for (name, unf) in [("ERV", &erv), ("McMillan", &mcm)] {
            assert_eq!(unf.reachable_markings(1_000_000).unwrap(), reachable, "case {case} {name}: markings");
            let dead_expected: BTreeSet<u32> = (0..arcs.len() as u32)
                .filter(|&t| !reachable.iter().any(|m| arcs[t as usize].0.iter().all(|&p| m[p] >= 1)))
                .collect();
            let dead: BTreeSet<u32> = unf.dead_transitions().iter().map(|t| t.0).collect();
            assert_eq!(dead, dead_expected, "case {case} {name}: dead transitions");
            let deadlock_expected = reachable.iter().any(|m| !arcs.iter().any(|(pre, _)| pre.iter().all(|&p| m[p] >= 1)));
            assert_eq!(unf.find_deadlock(1_000_000).unwrap().is_some(), deadlock_expected, "case {case} {name}: deadlock");
        }
        assert!(erv.event_count() <= mcm.event_count(), "case {case}: ERV {} > McMillan {}", erv.event_count(), mcm.event_count());
        // Total order: at most one non-cut-off event per reachable marking.
        assert!(erv.event_count() - erv.cutoff_count() <= reachable.len(), "case {case}");
        smaller += usize::from(erv.event_count() < mcm.event_count());
        erv_events += erv.event_count();
        mcm_events += mcm.event_count();
        cutoffs += erv.cutoff_count();
    }
    println!("safe={safe} unsafe={unsafe_} erv_smaller={smaller} erv_events={erv_events} mcmillan_events={mcm_events} cutoffs={cutoffs}");
    assert!(safe > 800 && unsafe_ > 200 && cutoffs > 500);
}

/// Family of Figure 5: k diamonds in a row; the unfolding of McMillan's
/// algorithm is exponential, the prefix with the ERV order is linear.
fn diamonds(k: usize) -> PetriNet {
    let places = k + 1;
    let mut arcs: Arcs = Vec::new();
    for i in 0..k {
        arcs.push((vec![i], vec![i + 1]));
        arcs.push((vec![i], vec![i + 1]));
    }
    let mut m0 = vec![0u64; places];
    m0[0] = 1;
    // close the net so every transition has a postset and the net is cyclic
    arcs.push((vec![k], vec![0]));
    build(&arcs, &m0).unwrap()
}

#[test]
fn erv_prefix_of_the_diamond_family_is_linear_while_mcmillan_is_exponential() {
    let mut previous_mcmillan = 0;
    for k in 2..=9 {
        let net = diamonds(k);
        let erv = unfold(&net, &UnfoldingConfig::default()).unwrap();
        let mcm = unfold(&net, &UnfoldingConfig { order: AdequateOrder::McMillan, ..Default::default() }).unwrap();
        assert!(erv.event_count() <= 4 * k + 2, "k={k}: ERV has {} events", erv.event_count());
        assert!(erv.event_count() < mcm.event_count(), "k={k}");
        if k > 2 {
            assert!(mcm.event_count() > previous_mcmillan + previous_mcmillan / 2, "k={k}: McMillan not growing exponentially");
        }
        previous_mcmillan = mcm.event_count();
        let markings = erv.reachable_markings(1_000_000).unwrap();
        assert_eq!(markings.len(), k + 1);
    }
}

#[test]
fn unsafe_and_unsupported_nets_are_refused() {
    // t: p0 -> p0 + p1 repeatedly marks p1 twice.
    let net = build(&vec![(vec![0], vec![0, 1])], &[1, 0]).unwrap();
    assert!(unfold(&net, &UnfoldingConfig::default()).is_err());
    // two tokens initially
    let net = build(&vec![(vec![0], vec![1])], &[2, 0]).unwrap();
    assert!(unfold(&net, &UnfoldingConfig::default()).is_err());
}
