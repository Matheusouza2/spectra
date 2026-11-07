//! Conformance tests for MinCov (Finkel-Haddad-Khmelnitsky 2020, Algorithm 1).
//!
//! Oracles (spec §8.1.4-§8.1.6): exhaustive reachability on bounded nets and
//! the classical Karp-Miller construction, compared after normalisation to the
//! antichain of maximal ω-markings.

use std::collections::{BTreeSet, HashSet, VecDeque};

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::{
    ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue, TransitionId, TransitionKind,
};
use racon_coverability::{
    CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy, MinCov, OmegaValue,
};

type Om = Vec<OmegaValue>;

fn ctx() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

fn geq(a: &Om, b: &Om) -> bool {
    a.iter().zip(b).all(|(x, y)| match (x, y) {
        (OmegaValue::Omega, _) => true,
        (OmegaValue::Finite(_), OmegaValue::Omega) => false,
        (OmegaValue::Finite(p), OmegaValue::Finite(q)) => p >= q,
    })
}

fn maximal(items: impl IntoIterator<Item = Om>) -> BTreeSet<Om> {
    let all: Vec<Om> = items.into_iter().collect::<HashSet<_>>().into_iter().collect();
    all.iter()
        .filter(|m| !all.iter().any(|o| *o != **m && geq(o, m)))
        .cloned()
        .collect()
}

fn net_from(pre: &[Vec<u32>], post: &[Vec<u32>], m0: &[u64]) -> PetriNet {
    let places = m0.len();
    let mut b = PetriNetBuilder::default();
    for p in 0..places {
        b.add_place(format!("p{p}"), None);
    }
    for t in 0..pre.len() {
        let tid = b.add_transition(format!("t{t}"), None);
        for p in 0..places {
            if pre[t][p] > 0 {
                b.add_arc(
                    ArcEndpoint::Place(PlaceId(p as u32)),
                    ArcEndpoint::Transition(tid),
                    pre[t][p],
                );
            }
            if post[t][p] > 0 {
                b.add_arc(
                    ArcEndpoint::Transition(tid),
                    ArcEndpoint::Place(PlaceId(p as u32)),
                    post[t][p],
                );
            }
        }
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect())
        .expect("valid net")
}

fn mincov(net: &PetriNet) -> BTreeSet<Om> {
    MinCov::default()
        .compute(net, &ctx())
        .expect("MinCov terminates")
        .elements()
        .iter()
        .cloned()
        .collect()
}

fn karp_miller(net: &PetriNet) -> BTreeSet<Om> {
    let graph = CoverabilityEngineImpl::default()
        .build_complete(net, CoverabilityStrategy::KarpMiller, &ctx())
        .expect("KM terminates");
    maximal(graph.nodes.iter().map(|n| {
        n.marking
            .tokens
            .iter()
            .map(|t| match t {
                TokenValue::Finite(v) => OmegaValue::Finite(*v),
                TokenValue::Omega => OmegaValue::Omega,
                TokenValue::Exact(_) => panic!("exact token"),
            })
            .collect()
    }))
}

fn reachable_maximal(net: &PetriNet, cap: usize) -> Option<BTreeSet<Om>> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([net.initial_marking().clone()]);
    seen.insert(net.initial_marking().clone());
    while let Some(m) = queue.pop_front() {
        for t in net.fireable_transitions(&m) {
            let next = net.fire(t, &m);
            if seen.insert(next.clone()) {
                if seen.len() > cap {
                    return None;
                }
                queue.push_back(next);
            }
        }
    }
    Some(maximal(seen.into_iter().map(|m| {
        m.tokens
            .iter()
            .map(|t| match t {
                TokenValue::Finite(v) => OmegaValue::Finite(*v),
                _ => unreachable!(),
            })
            .collect()
    })))
}

fn fin(v: &[u64]) -> Om {
    v.iter().map(|&n| OmegaValue::Finite(n)).collect()
}

#[test]
fn bounded_cycle_clover_is_the_maximal_reachable_markings() {
    // p0 -> t0 -> p1 -> t1 -> p0, one token
    let net = net_from(&[vec![1, 0], vec![0, 1]], &[vec![0, 1], vec![1, 0]], &[1, 0]);
    let set = MinCov::default().compute(&net, &ctx()).unwrap();
    assert!(set.is_bounded());
    assert_eq!(set.elements(), &[fin(&[0, 1]), fin(&[1, 0])]);
    assert!(set.covers(&[0, 1]) && !set.covers(&[1, 1]));
}

#[test]
fn growing_loop_is_accelerated_to_omega() {
    // t0: p0 -> p0 + p1 (p1 grows without bound)
    let net = net_from(&[vec![1, 0]], &[vec![1, 1]], &[1, 0]);
    let set = MinCov::default().compute(&net, &ctx()).unwrap();
    assert!(!set.is_bounded());
    assert_eq!(set.unbounded_places(), vec![1]);
    assert_eq!(
        set.elements(),
        &[vec![OmegaValue::Finite(1), OmegaValue::Omega]]
    );
    assert!(set.covers(&[1, 1_000_000]));
    assert!(!set.covers(&[2, 0]));
}

#[test]
fn consumption_inside_the_loop_blocks_acceleration() {
    // p0 -> p0 + p1, and p1 -> p2 : p2 and p1 both unbounded
    let net = net_from(
        &[vec![1, 0, 0], vec![0, 1, 0]],
        &[vec![1, 1, 0], vec![0, 0, 1]],
        &[1, 0, 0],
    );
    let set = MinCov::default().compute(&net, &ctx()).unwrap();
    assert_eq!(set.unbounded_places(), vec![1, 2]);
}

#[test]
fn extended_nets_are_refused() {
    let mut b = PetriNetBuilder::default();
    b.add_place("p".into(), None);
    let t = b.add_transition("t".into(), None);
    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(PlaceId(0)), 1);
    b.set_transition_kind(t, TransitionKind::Immediate);
    let net = b.build(vec![TokenValue::Finite(0)]).unwrap();
    assert!(matches!(
        MinCov::default().compute(&net, &ctx()),
        Err(racon_coverability::CoverabilityError::Unsupported(_))
    ));
    let _ = TransitionId(0);
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
}

fn random_net(rng: &mut Lcg) -> PetriNet {
    let places = 2 + rng.next(3) as usize;
    let transitions = 2 + rng.next(4) as usize;
    let mut pre = vec![vec![0u32; places]; transitions];
    let mut post = vec![vec![0u32; places]; transitions];
    for t in 0..transitions {
        for p in 0..places {
            if rng.next(3) == 0 {
                pre[t][p] = 1 + rng.next(2) as u32;
            }
            if rng.next(3) == 0 {
                post[t][p] = 1 + rng.next(2) as u32;
            }
        }
        if pre[t].iter().chain(&post[t]).all(|&w| w == 0) {
            post[t][0] = 1;
        }
    }
    for p in 0..places {
        if (0..transitions).all(|t| pre[t][p] == 0 && post[t][p] == 0) {
            post[0][p] = 1;
        }
    }
    let m0: Vec<u64> = (0..places).map(|_| rng.next(3)).collect();
    net_from(&pre, &post, &m0)
}

#[test]
fn mincov_matches_karp_miller_and_exhaustive_reachability_on_random_nets() {
    let mut rng = Lcg(0xFEED_FACE);
    let (mut unbounded, mut bounded) = (0, 0);
    for case in 0..600 {
        let net = random_net(&mut rng);
        let clover = mincov(&net);
        assert_eq!(
            clover,
            karp_miller(&net),
            "case {case}: MinCov differs from maximal Karp-Miller labels"
        );
        match reachable_maximal(&net, 5_000) {
            Some(exact) => {
                // finite reachability set: Clover has no ω and equals the maximal markings
                if clover.iter().all(|m| m.iter().all(|v| *v != OmegaValue::Omega)) {
                    assert_eq!(clover, exact, "case {case}: bounded net");
                    bounded += 1;
                } else {
                    unbounded += 1;
                }
            }
            None => unbounded += 1,
        }
        // antichain property (Proposition 9)
        for a in &clover {
            for b in &clover {
                assert!(a == b || !geq(a, b), "case {case}: not an antichain");
            }
        }
    }
    assert!(bounded >= 50 && unbounded >= 50, "bounded={bounded} unbounded={unbounded}");
}
