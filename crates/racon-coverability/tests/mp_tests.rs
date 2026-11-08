//! Conformance tests for Monotone-Pruning (Reynier-Servais, Algorithm 2) and CovProc (Geeraerts-Raskin-Van Begin).
//!
//! Oracles (spec §8.1.4-§8.1.6): exhaustive reachability on bounded nets and
//! the classical Karp-Miller construction, compared after normalisation to the
//! antichain of maximal ω-markings.

use std::collections::{BTreeSet, HashSet, VecDeque};

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::{
    ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue,
};
use racon_coverability::{
    CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy, CovProc, CovProcConfig, MonotonePruning, OmegaValue, WaitStrategy, MonotonePruningConfig,
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
    mp(net, WaitStrategy::BreadthFirst)
}

fn mp(net: &PetriNet, strategy: WaitStrategy) -> BTreeSet<Om> {
    MonotonePruning::new(MonotonePruningConfig { strategy, ..Default::default() })
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
fn paper_example_2_1_has_the_four_element_mcs() {
    // Fig. 1(a): p1..p6 -> indices 0..5.
    // t1: p1 -> p3; t3: p3 -> p4; t4: p4 -> p3 + p5; t5: p1 -> p6; t6: p6 -> p4 + 2p5.
    let pre = vec![
        vec![1, 0, 0, 0, 0, 0],
        vec![0, 0, 1, 0, 0, 0],
        vec![0, 0, 0, 1, 0, 0],
        vec![1, 0, 0, 0, 0, 0],
        vec![0, 0, 0, 0, 0, 1],
    ];
    let post = vec![
        vec![0, 0, 1, 0, 0, 0],
        vec![0, 0, 0, 1, 0, 0],
        vec![0, 0, 1, 0, 1, 0],
        vec![0, 0, 0, 0, 0, 1],
        vec![0, 0, 0, 1, 2, 0],
    ];
    let net = net_from(&pre, &post, &[1, 0, 0, 0, 0, 0]);
    let w = OmegaValue::Omega;
    let f = OmegaValue::Finite;
    let expected: BTreeSet<Om> = [
        vec![f(1), f(0), f(0), f(0), f(0), f(0)],
        vec![f(0), f(0), f(0), f(0), f(0), f(1)],
        vec![f(0), f(0), f(1), f(0), w, f(0)],
        vec![f(0), f(0), f(0), f(1), w, f(0)],
    ]
    .into_iter()
    .collect();
    for s in [WaitStrategy::BreadthFirst, WaitStrategy::DepthFirst] {
        assert_eq!(mp(&net, s), expected, "{s:?}");
    }
    for oracle in [true, false] {
        let cp = CovProc::new(CovProcConfig { use_oracle: oracle, ..Default::default() })
            .compute(&net, &ctx())
            .unwrap();
        let cp: BTreeSet<Om> = cp.elements().iter().cloned().collect();
        assert_eq!(cp, expected, "CovProc oracle={oracle}");
    }
}

#[test]
fn mp_matches_karp_miller_and_exhaustive_reachability_on_random_nets() {
    let mut rng = Lcg(0xC0FF_EE11);
    let (mut unbounded, mut bounded) = (0, 0);
    for case in 0..600 {
        let net = random_net(&mut rng);
        for s in [WaitStrategy::BreadthFirst, WaitStrategy::DepthFirst] {
            let set = mp(&net, s);
            assert_eq!(set, karp_miller(&net), "case {case} {s:?}: differs from Karp-Miller");
            for oracle in [true, false] {
                let cp = CovProc::new(CovProcConfig { use_oracle: oracle, ..Default::default() })
                    .compute(&net, &ctx())
                    .expect("CovProc terminates");
                let cp: BTreeSet<Om> = cp.elements().iter().cloned().collect();
                assert_eq!(cp, set, "case {case}: CovProc(oracle={oracle}) differs");
            }
            for a in &set {
                for b in &set {
                    assert!(a == b || !geq(a, b), "case {case}: not an antichain");
                }
            }
        }
        let set = mincov(&net);
        let has_omega = set.iter().any(|m| m.iter().any(|v| *v == OmegaValue::Omega));
        if !has_omega {
            if let Some(exact) = reachable_maximal(&net, 5_000) {
                assert_eq!(set, exact, "case {case}: bounded net");
            }
            bounded += 1;
        } else {
            unbounded += 1;
        }
    }
    assert!(bounded >= 50 && unbounded >= 50, "bounded={bounded} unbounded={unbounded}");
}
