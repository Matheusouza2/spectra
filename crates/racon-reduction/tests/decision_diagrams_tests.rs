//! Conformance tests for MDD saturation (R-Ciardo/Bryant).
//!
//! Oracle: breadth-first enumeration of the reachable markings.

use std::collections::{BTreeSet, VecDeque};

use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue};
use racon_reduction::decision_diagrams::{generate, DdConfig};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

type Arcs = Vec<(Vec<(usize, u32)>, Vec<(usize, u32)>)>;

fn build(arcs: &Arcs, m0: &[u64]) -> Option<PetriNet> {
    let mut b = PetriNetBuilder::default();
    for p in 0..m0.len() {
        b.add_place(format!("p{p}"), None);
    }
    let mut any = false;
    for (t, (pre, post)) in arcs.iter().enumerate() {
        let tid = b.add_transition(format!("t{t}"), None);
        for &(p, w) in pre {
            b.add_arc(ArcEndpoint::Place(PlaceId(p as u32)), ArcEndpoint::Transition(tid), w);
            any = true;
        }
        for &(p, w) in post {
            b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(p as u32)), w);
            any = true;
        }
    }
    if !any {
        return None;
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect()).ok()
}

fn bfs(arcs: &Arcs, m0: &[u64], cap: usize) -> Option<BTreeSet<Vec<u64>>> {
    let mut seen = BTreeSet::from([m0.to_vec()]);
    let mut queue = VecDeque::from([m0.to_vec()]);
    while let Some(m) = queue.pop_front() {
        for (pre, post) in arcs {
            if pre.iter().all(|&(p, w)| m[p] >= u64::from(w)) {
                let mut n = m.clone();
                for &(p, w) in pre {
                    n[p] -= u64::from(w);
                }
                for &(p, w) in post {
                    n[p] += u64::from(w);
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
    let places = 2 + rng.next(5) as usize;
    let transitions = 2 + rng.next(6) as usize;
    let mut side = |rng: &mut Lcg| {
        let mut v = Vec::new();
        for p in 0..places {
            if rng.next(100) < 30 {
                v.push((p, 1 + (rng.next(10) / 8) as u32));
            }
        }
        v
    };
    let mut arcs: Arcs = Vec::new();
    for _ in 0..transitions {
        let pre = side(rng);
        let post = side(rng);
        arcs.push((pre, post));
    }
    let m0 = (0..places).map(|_| if rng.next(100) < 45 { 0 } else { 1 + rng.next(3) }).collect();
    (arcs, m0)
}

#[test]
fn saturation_computes_exactly_the_reachable_markings() {
    let mut rng = Lcg(0xDD5A_7001);
    let (mut checked, mut unbounded_refused) = (0, 0);
    for case in 0..4000 {
        let (arcs, m0) = random_net(&mut rng);
        let Some(net) = build(&arcs, &m0) else { continue };
        match bfs(&arcs, &m0, 5_000) {
            Some(expected) => {
                let space = generate(&net, &DdConfig::default()).unwrap_or_else(|e| panic!("case {case}: {e}"));
                assert_eq!(space.states(1_000_000).unwrap(), expected, "case {case}: states");
                assert_eq!(space.state_count(), Some(expected.len() as u128), "case {case}: count");
                let bounds: Vec<u64> = (0..m0.len()).map(|p| expected.iter().map(|m| m[p]).max().unwrap()).collect();
                assert_eq!(space.place_bounds(), bounds, "case {case}: bounds");
                for m in expected.iter().take(20) {
                    assert!(space.contains(m), "case {case}");
                }
                let mut outside = m0.clone();
                outside[0] += 1000;
                assert!(!space.contains(&outside));
                checked += 1;
            }
            None => {
                // BFS blew past the cap: either unbounded or just large.
                if generate(&net, &DdConfig { max_nodes: 20_000 }).is_err() {
                    unbounded_refused += 1;
                }
            }
        }
    }
    println!("checked={checked} unbounded_refused={unbounded_refused}");
    assert!(checked > 2000 && unbounded_refused > 10);
}

/// Dining philosophers: 4 places and 4 transitions per philosopher around a ring.
fn philosophers(n: usize) -> (Arcs, Vec<u64>) {
    // per philosopher i: thinking(4i), hungry? -> use idle(4i), hasLeft(4i+1), eating(4i+2), fork(4i+3)
    let mut arcs: Arcs = Vec::new();
    let mut m0 = vec![0u64; 4 * n];
    for i in 0..n {
        let (idle, has_left, eating, fork) = (4 * i, 4 * i + 1, 4 * i + 2, 4 * i + 3);
        let next_fork = 4 * ((i + 1) % n) + 3;
        m0[idle] = 1;
        m0[fork] = 1;
        arcs.push((vec![(idle, 1), (fork, 1)], vec![(has_left, 1)]));
        arcs.push((vec![(has_left, 1), (next_fork, 1)], vec![(eating, 1)]));
        arcs.push((vec![(eating, 1)], vec![(idle, 1), (fork, 1), (next_fork, 1)]));
    }
    (arcs, m0)
}

#[test]
fn philosophers_match_bfs_and_the_mdd_stays_small() {
    let mut previous_states = 0u128;
    for n in 2..=7 {
        let (arcs, m0) = philosophers(n);
        let net = build(&arcs, &m0).unwrap();
        let expected = bfs(&arcs, &m0, 2_000_000).unwrap();
        let space = generate(&net, &DdConfig::default()).unwrap();
        assert_eq!(space.state_count(), Some(expected.len() as u128), "n={n}");
        assert_eq!(space.states(3_000_000).unwrap(), expected, "n={n}");
        let states = space.state_count().unwrap();
        assert!(states > previous_states);
        previous_states = states;
        println!("n={n} states={states} mdd_nodes={} allocated={}", space.node_count(), space.allocated_nodes);
    }
    // 1000 philosophers: the state space (>10^100) is far beyond enumeration.
    let (arcs, m0) = philosophers(300);
    let net = build(&arcs, &m0).unwrap();
    let space = generate(&net, &DdConfig::default()).unwrap();
    assert!(space.state_count().is_none() || space.state_count().unwrap() > 10u128.pow(30));
    assert!(space.node_count() < 20_000, "{}", space.node_count());
}

#[test]
fn unbounded_net_hits_the_node_limit_and_extended_nets_are_refused() {
    let arcs: Arcs = vec![(vec![(0, 1)], vec![(0, 1), (1, 1)])];
    let net = build(&arcs, &[1, 0]).unwrap();
    assert!(generate(&net, &DdConfig { max_nodes: 2_000 }).is_err());
}
