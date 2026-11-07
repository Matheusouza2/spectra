//! Boundedness through stubborn sets (Schmidt 1999, §6, Thms 1–2).
//!
//! At every marking only the enabled transitions of a weakly stubborn superset
//! of an *attractor set* `A` are fired:
//!
//! * whole net: `A` = transitions that produce more tokens than they consume
//!   (Thm 1: the reduced graph is finite iff the full graph is finite);
//! * a single place `p`: `A = •p`, the transitions with an arc into `p`
//!   (Thm 2: `p` is bounded in the reduced graph iff it is bounded in the net).
//!
//! No ignoring proviso is needed. The decision procedure built on top is sound
//! in both directions regardless of how the stubborn sets are chosen:
//!
//! * `Bounded` — the reduced graph was exhausted, hence finite (Thm 1/2);
//! * `Unbounded` — a path of the reduced graph (a real firing sequence) reaches
//!   a marking `m_j ≥ m_i` of an earlier marking `m_i` on the same path, with
//!   `m_j ≠ m_i` (whole net) or `m_j(p) > m_i(p)` (place); by monotonicity the
//!   segment can be repeated forever.
//!
//! Exceeding the node budget yields `LimitExceeded`, never a verdict. Only
//! plain P/T nets are supported (monotonicity fails with inhibitor arcs).

use std::collections::{HashMap, VecDeque};

use racon_core::execution::AnalysisContext;
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{PetriNet, PlaceId, TransitionId};

use crate::error::ReductionError;
use crate::goal_search::{close_stubborn, NetIndex};

/// What boundedness question is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundTarget {
    Net,
    Place(PlaceId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundVerdict {
    Bounded,
    /// `path` is a firing sequence from the initial marking; the marking after
    /// `covered_prefix` steps is strictly covered by the final marking.
    Unbounded {
        path: Vec<TransitionId>,
        covered_prefix: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct BoundSearchConfig {
    pub max_nodes: usize,
    /// When false every enabled transition is fired (baseline for comparison).
    pub use_stubborn: bool,
}

impl Default for BoundSearchConfig {
    fn default() -> Self {
        Self {
            max_nodes: 5_000_000,
            use_stubborn: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundSearchReport {
    pub verdict: BoundVerdict,
    pub visited: usize,
}

fn tokens(marking: &Marking, place: usize) -> u128 {
    marking
        .get(place)
        .and_then(TokenValue::as_biguint)
        .and_then(|v| u128::try_from(v).ok())
        .unwrap_or(u128::MAX)
}

/// `earlier ≤ later` componentwise, and strictly larger for the target.
fn strictly_covered(earlier: &Marking, later: &Marking, target: BoundTarget) -> bool {
    let n = earlier.len();
    let mut strict = false;
    for place in 0..n {
        let (a, b) = (tokens(earlier, place), tokens(later, place));
        if a > b {
            return false;
        }
        if a < b {
            strict |= match target {
                BoundTarget::Net => true,
                BoundTarget::Place(p) => p.0 as usize == place,
            };
        }
    }
    strict
}

/// Attractor set `A` as a membership mask.
fn attractor(net: &PetriNet, target: BoundTarget) -> Vec<bool> {
    net.transitions()
        .iter()
        .map(|transition| {
            let produced: u64 = net.postset(transition.id).iter().map(|&(_, w)| u64::from(w)).sum();
            let consumed: u64 = net.preset(transition.id).iter().map(|&(_, w)| u64::from(w)).sum();
            match target {
                BoundTarget::Net => produced > consumed,
                BoundTarget::Place(p) => net.postset(transition.id).iter().any(|&(q, _)| q == p),
            }
        })
        .collect()
}

/// Decide boundedness of the net or of one place on the stubborn-reduced graph.
pub fn decide_boundedness_stubborn(
    net: &PetriNet,
    target: BoundTarget,
    config: BoundSearchConfig,
    context: &AnalysisContext,
) -> Result<BoundSearchReport, ReductionError> {
    if net.is_extended() {
        return Err(ReductionError::NotApplicable {
            name: "STUBBORN_BOUNDEDNESS",
            reason: "boundedness stubborn sets are proved for plain P/T nets; nets with inhibitor arcs or immediate transitions are not supported",
        });
    }
    if let BoundTarget::Place(p) = target {
        if p.0 as usize >= net.place_count() {
            return Err(ReductionError::NotApplicable {
                name: "STUBBORN_BOUNDEDNESS",
                reason: "place index out of range",
            });
        }
    }
    let index = NetIndex::new(net);
    let attract = attractor(net, target);

    let initial = net.initial_marking().clone();
    let mut markings = vec![initial.clone()];
    let mut parents: Vec<Option<(usize, TransitionId)>> = vec![None];
    let mut seen: HashMap<Marking, usize> = HashMap::from([(initial, 0)]);
    let mut queue = VecDeque::from([0usize]);

    while let Some(node) = queue.pop_front() {
        context
            .check_cancellation()
            .map_err(|_| ReductionError::Cancelled)?;
        let marking = markings[node].clone();

        let stubborn = if config.use_stubborn {
            let mut included = attract.clone();
            let mut stack: Vec<usize> = (0..included.len()).filter(|&t| included[t]).collect();
            close_stubborn(net, &index, &marking, &mut included, &mut stack);
            Some(included)
        } else {
            None
        };

        for transition in net.transitions() {
            let id = transition.id;
            if stubborn.as_ref().is_some_and(|set| !set[id.0 as usize]) {
                continue;
            }
            if !net.is_enabled(id, &marking) {
                continue;
            }
            let successor = net.fire(id, &marking);
            if seen.contains_key(&successor) {
                continue;
            }
            if markings.len() >= config.max_nodes {
                return Err(ReductionError::LimitExceeded {
                    limit: config.max_nodes,
                });
            }
            let created = markings.len();
            parents.push(Some((node, id)));

            // Compare with every ancestor on the tree path (and `node` itself).
            let mut ancestor = Some(node);
            let mut covered = None;
            while let Some(current) = ancestor {
                if strictly_covered(&markings[current], &successor, target) {
                    covered = Some(current);
                    break;
                }
                ancestor = parents[current].map(|(parent, _)| parent);
            }
            if let Some(covered) = covered {
                let mut path = Vec::new();
                let mut cursor = created;
                while let Some((parent, fired)) = parents[cursor] {
                    path.push(fired);
                    cursor = parent;
                }
                path.reverse();
                let mut depth = 0;
                let mut cursor = covered;
                while let Some((parent, _)) = parents[cursor] {
                    depth += 1;
                    cursor = parent;
                }
                return Ok(BoundSearchReport {
                    verdict: BoundVerdict::Unbounded {
                        path,
                        covered_prefix: depth,
                    },
                    visited: markings.len() + 1,
                });
            }

            markings.push(successor.clone());
            seen.insert(successor, created);
            queue.push_back(created);
        }
    }

    Ok(BoundSearchReport {
        verdict: BoundVerdict::Bounded,
        visited: markings.len(),
    })
}

/// Verdict of the ω-accelerated search (no concrete witness: an ω may stand
/// for a path through generalised markings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmegaVerdict {
    Bounded,
    Unbounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmegaSearchReport {
    pub verdict: OmegaVerdict,
    pub visited: usize,
}

/// ω value of a token count: `None` is ω.
fn omega_value(value: &TokenValue) -> Option<u128> {
    value
        .as_biguint()
        .map(|v| u128::try_from(v).unwrap_or(u128::MAX))
}

/// `a ≤ b` for generalised markings (ω above every finite value).
fn omega_leq(a: &Marking, b: &Marking) -> bool {
    a.tokens.iter().zip(&b.tokens).all(|(x, y)| match (omega_value(x), omega_value(y)) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(x), Some(y)) => x <= y,
    })
}

/// Karp–Miller tree whose nodes expand only the enabled transitions of a
/// weakly stubborn superset of the attractor set (Schmidt 1999, §6: "the
/// combined approach works exactly like usual coverability graph generation
/// but considers a stubborn set of transitions at every generalised marking").
///
/// Unlike [`decide_boundedness_stubborn`] this always terminates on a finite
/// tree, so a single place can be decided even when other places grow forever.
///
/// * `Unbounded` is sound: an ω is only ever created from an ancestor `m_i`
///   and a descendant `m_j ≥ m_i` on a path of real firings (monotonicity).
/// * `Bounded` rests on Schmidt's Thm 2 for the reduced system and is checked
///   empirically against the unreduced Karp–Miller tree (see tests).
///
/// Plain P/T nets only. A node budget overrun is `LimitExceeded`.
pub fn decide_boundedness_stubborn_omega(
    net: &PetriNet,
    target: BoundTarget,
    config: BoundSearchConfig,
    context: &AnalysisContext,
) -> Result<OmegaSearchReport, ReductionError> {
    if net.is_extended() {
        return Err(ReductionError::NotApplicable {
            name: "STUBBORN_BOUNDEDNESS",
            reason: "boundedness stubborn sets are proved for plain P/T nets; nets with inhibitor arcs or immediate transitions are not supported",
        });
    }
    if let BoundTarget::Place(p) = target {
        if p.0 as usize >= net.place_count() {
            return Err(ReductionError::NotApplicable {
                name: "STUBBORN_BOUNDEDNESS",
                reason: "place index out of range",
            });
        }
    }
    let unbounded_now = |marking: &Marking| match target {
        BoundTarget::Net => marking.has_omega(),
        BoundTarget::Place(p) => matches!(marking.get(p.0 as usize), Some(TokenValue::Omega)),
    };
    let index = NetIndex::new(net);
    let attract = attractor(net, target);

    let root = net.initial_marking().clone();
    let mut markings = vec![root];
    let mut parents: Vec<Option<usize>> = vec![None];
    let mut queue = VecDeque::from([0usize]);

    while let Some(node) = queue.pop_front() {
        context
            .check_cancellation()
            .map_err(|_| ReductionError::Cancelled)?;
        let marking = markings[node].clone();
        let stubborn = if config.use_stubborn {
            let mut included = attract.clone();
            let mut stack: Vec<usize> = (0..included.len()).filter(|&t| included[t]).collect();
            close_stubborn(net, &index, &marking, &mut included, &mut stack);
            Some(included)
        } else {
            None
        };

        for transition in net.transitions() {
            let id = transition.id;
            if stubborn.as_ref().is_some_and(|set| !set[id.0 as usize]) {
                continue;
            }
            if !net.is_enabled(id, &marking) {
                continue;
            }
            let mut successor = net.fire(id, &marking);

            // ω-acceleration against every ancestor (and the node itself).
            let mut duplicate = false;
            let mut ancestor = Some(node);
            while let Some(current) = ancestor {
                let earlier = &markings[current];
                if *earlier == successor {
                    duplicate = true;
                    break;
                }
                if omega_leq(earlier, &successor) {
                    for place in 0..successor.len() {
                        let grows = match (omega_value(&earlier.tokens[place]), omega_value(&successor.tokens[place])) {
                            (Some(a), Some(b)) => a < b,
                            _ => false,
                        };
                        if grows {
                            successor.tokens[place] = TokenValue::Omega;
                        }
                    }
                }
                ancestor = parents[current];
            }
            if unbounded_now(&successor) {
                return Ok(OmegaSearchReport {
                    verdict: OmegaVerdict::Unbounded,
                    visited: markings.len() + 1,
                });
            }
            if duplicate {
                continue;
            }
            if markings.len() >= config.max_nodes {
                return Err(ReductionError::LimitExceeded {
                    limit: config.max_nodes,
                });
            }
            markings.push(successor);
            parents.push(Some(node));
            queue.push_back(markings.len() - 1);
        }
    }
    Ok(OmegaSearchReport {
        verdict: OmegaVerdict::Bounded,
        visited: markings.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::AnalysisConfig;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

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
        let places = 2 + rng.next(4) as usize;
        let transitions = 2 + rng.next(4) as usize;
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..places).map(|i| b.add_place(format!("p{i}"), None)).collect();
        for i in 0..transitions {
            let t = b.add_transition(format!("t{i}"), None);
            let mut has_input = false;
            for &p in &ps {
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1 + rng.next(2) as u32);
                    has_input = true;
                }
                if rng.next(3) == 0 {
                    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1 + rng.next(2) as u32);
                }
            }
            if !has_input {
                b.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(t), 1);
            }
        }
        let tokens = (0..places).map(|_| TokenValue::Finite(rng.next(3))).collect();
        b.build(tokens).unwrap()
    }

    /// Independent ground truth: classical Karp–Miller tree. Returns the set of
    /// unbounded places, or `None` if the node budget is exceeded.
    fn karp_miller_unbounded(net: &PetriNet, cap: usize) -> Option<Vec<bool>> {
        type Omega = Vec<Option<u64>>; // None = ω
        let covers = |a: &Omega, b: &Omega| {
            a.iter().zip(b).all(|(x, y)| match (x, y) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(x), Some(y)) => x >= y,
            })
        };
        let enabled = |m: &Omega, t: TransitionId| {
            net.preset(t).iter().all(|&(p, w)| match m[p.0 as usize] {
                None => true,
                Some(v) => v >= u64::from(w),
            })
        };
        let root: Omega = net
            .initial_marking()
            .tokens
            .iter()
            .map(|t| t.as_biguint().and_then(|v| u64::try_from(v).ok()))
            .collect();
        let mut nodes: Vec<(Omega, Option<usize>)> = vec![(root, None)];
        let mut work = vec![0usize];
        while let Some(i) = work.pop() {
            let m = nodes[i].0.clone();
            // Skip if an ancestor carries the identical marking.
            let mut anc = nodes[i].1;
            let mut dup = false;
            while let Some(a) = anc {
                if nodes[a].0 == m {
                    dup = true;
                    break;
                }
                anc = nodes[a].1;
            }
            if dup {
                continue;
            }
            for t in net.transitions() {
                if !enabled(&m, t.id) {
                    continue;
                }
                let mut next = m.clone();
                for &(p, w) in net.preset(t.id) {
                    if let Some(v) = next[p.0 as usize].as_mut() {
                        *v -= u64::from(w);
                    }
                }
                for &(p, w) in net.postset(t.id) {
                    if let Some(v) = next[p.0 as usize].as_mut() {
                        *v += u64::from(w);
                    }
                }
                let mut anc = Some(i);
                while let Some(a) = anc {
                    let am = &nodes[a].0;
                    if covers(&next, am) && &next != am {
                        for k in 0..next.len() {
                            if next[k].is_some() && (am[k].is_none() || am[k] < next[k]) {
                                next[k] = None;
                            }
                        }
                    }
                    anc = nodes[a].1;
                }
                if nodes.len() >= cap {
                    return None;
                }
                nodes.push((next, Some(i)));
                work.push(nodes.len() - 1);
            }
        }
        let mut unbounded = vec![false; net.place_count()];
        for (m, _) in &nodes {
            for (k, v) in m.iter().enumerate() {
                unbounded[k] |= v.is_none();
            }
        }
        Some(unbounded)
    }

    fn replay(net: &PetriNet, path: &[TransitionId]) -> Vec<Marking> {
        let mut m = net.initial_marking().clone();
        let mut all = vec![m.clone()];
        for &t in path {
            assert!(net.is_enabled(t, &m));
            m = net.fire(t, &m);
            all.push(m.clone());
        }
        all
    }

    #[test]
    fn stubborn_boundedness_agrees_with_karp_miller_on_random_nets() {
        let context = AnalysisContext::new(AnalysisConfig::default());
        let mut rng = Lcg(0xB0DD);
        let iterations: usize = std::env::var("RACON_FUZZ_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(150);
        let (mut compared, mut unknown, mut smaller) = (0usize, 0usize, 0usize);
        for _ in 0..iterations {
            let net = random_net(&mut rng);
            let Some(truth) = karp_miller_unbounded(&net, 4_000) else {
                continue;
            };
            let budget = BoundSearchConfig {
                max_nodes: 1_500,
                use_stubborn: true,
            };
            let mut targets = vec![BoundTarget::Net];
            targets.extend((0..net.place_count()).map(|p| BoundTarget::Place(PlaceId(p as u32))));
            for target in targets {
                let expected_unbounded = match target {
                    BoundTarget::Net => truth.iter().any(|&u| u),
                    BoundTarget::Place(p) => truth[p.0 as usize],
                };
                let report = match decide_boundedness_stubborn(&net, target, budget, &context) {
                    Ok(report) => report,
                    Err(ReductionError::LimitExceeded { .. }) => {
                        unknown += 1;
                        continue;
                    }
                    Err(other) => panic!("{other}"),
                };
                compared += 1;
                match report.verdict {
                    BoundVerdict::Bounded => {
                        assert!(!expected_unbounded, "{target:?} wrongly bounded: {net:?}")
                    }
                    BoundVerdict::Unbounded { ref path, covered_prefix } => {
                        assert!(expected_unbounded, "{target:?} wrongly unbounded: {net:?}");
                        let markings = replay(&net, path);
                        assert!(strictly_covered(
                            &markings[covered_prefix],
                            markings.last().unwrap(),
                            target
                        ));
                    }
                }
                if let Ok(full) = decide_boundedness_stubborn(
                    &net,
                    target,
                    BoundSearchConfig { use_stubborn: false, ..budget },
                    &context,
                ) {
                    if report.visited < full.visited {
                        smaller += 1;
                    }
                }
            }
        }
        assert!(compared > iterations, "too few decided queries: {compared}");
        assert!(unknown * 20 < compared, "too many undecided: {unknown}/{compared}");
        assert!(smaller > 0, "stubborn never reduced the search");
    }

    #[test]
    fn independent_bounded_components_are_not_interleaved() {
        // Two independent finite pipelines: the full graph is a product.
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..4).map(|i| b.add_place(format!("p{i}"), None)).collect();
        let ta = b.add_transition("ta".into(), None);
        let tb = b.add_transition("tb".into(), None);
        b.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(ta), 1);
        b.add_arc(ArcEndpoint::Transition(ta), ArcEndpoint::Place(ps[1]), 1);
        b.add_arc(ArcEndpoint::Place(ps[2]), ArcEndpoint::Transition(tb), 1);
        b.add_arc(ArcEndpoint::Transition(tb), ArcEndpoint::Place(ps[3]), 1);
        let net = b
            .build(vec![TokenValue::Finite(4), TokenValue::Finite(0), TokenValue::Finite(4), TokenValue::Finite(0)])
            .unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let run = |use_stubborn| {
            decide_boundedness_stubborn(
                &net,
                BoundTarget::Net,
                BoundSearchConfig { use_stubborn, ..BoundSearchConfig::default() },
                &context,
            )
            .unwrap()
        };
        let (reduced, full) = (run(true), run(false));
        assert_eq!(reduced.verdict, BoundVerdict::Bounded);
        assert_eq!(full.visited, 25);
        // A is empty for a conservative net: only the initial marking is kept.
        assert_eq!(reduced.visited, 1);
    }

    #[test]
    fn unbounded_place_is_found_and_extended_nets_are_rejected() {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let q = b.add_place("q".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(q), 1);
        let net = b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)]).unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let report = decide_boundedness_stubborn(&net, BoundTarget::Place(q), BoundSearchConfig::default(), &context).unwrap();
        assert!(matches!(report.verdict, BoundVerdict::Unbounded { .. }));
        // p is bounded but q grows forever, so the reduced graph is infinite:
        // exhaustion never happens and the answer is "undecided", not a verdict.
        let error = decide_boundedness_stubborn(
            &net,
            BoundTarget::Place(p),
            BoundSearchConfig { max_nodes: 200, ..BoundSearchConfig::default() },
            &context,
        )
        .unwrap_err();
        assert_eq!(error.code(), "RESOURCE_LIMIT_EXCEEDED");

        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        b.add_inhibitor_arc(p, t, 2);
        let net = b.build(vec![TokenValue::Finite(0)]).unwrap();
        let err = decide_boundedness_stubborn(&net, BoundTarget::Net, BoundSearchConfig::default(), &context).unwrap_err();
        assert_eq!(err.code(), "REDUCTION_NOT_APPLICABLE");
    }

    #[test]
    fn omega_variant_decides_every_place_like_karp_miller() {
        let context = AnalysisContext::new(AnalysisConfig::default());
        let mut rng = Lcg(0x0E6A);
        let iterations: usize = std::env::var("RACON_FUZZ_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(150);
        let (mut compared, mut smaller) = (0usize, 0usize);
        for _ in 0..iterations {
            let net = random_net(&mut rng);
            let Some(truth) = karp_miller_unbounded(&net, 4_000) else {
                continue;
            };
            let budget = BoundSearchConfig { max_nodes: 4_000, use_stubborn: true };
            let mut targets = vec![BoundTarget::Net];
            targets.extend((0..net.place_count()).map(|p| BoundTarget::Place(PlaceId(p as u32))));
            for target in targets {
                let expected_unbounded = match target {
                    BoundTarget::Net => truth.iter().any(|&u| u),
                    BoundTarget::Place(p) => truth[p.0 as usize],
                };
                let report = decide_boundedness_stubborn_omega(&net, target, budget, &context)
                    .expect("the omega tree is finite and within budget here");
                assert_eq!(
                    report.verdict == OmegaVerdict::Unbounded,
                    expected_unbounded,
                    "{target:?} disagrees with Karp-Miller: {net:?}"
                );
                compared += 1;
                let full = decide_boundedness_stubborn_omega(
                    &net,
                    target,
                    BoundSearchConfig { use_stubborn: false, ..budget },
                    &context,
                )
                .unwrap();
                assert_eq!(full.verdict, report.verdict);
                if report.visited < full.visited {
                    smaller += 1;
                }
            }
        }
        assert!(compared > iterations, "too few queries: {compared}");
        assert!(smaller > 0, "stubborn never shrank the omega tree");
    }

    #[test]
    fn omega_variant_decides_a_bounded_place_next_to_an_unbounded_one() {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let q = b.add_place("q".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(q), 1);
        let net = b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)]).unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let run = |target| {
            decide_boundedness_stubborn_omega(&net, target, BoundSearchConfig::default(), &context)
                .unwrap()
                .verdict
        };
        assert_eq!(run(BoundTarget::Place(p)), OmegaVerdict::Bounded);
        assert_eq!(run(BoundTarget::Place(q)), OmegaVerdict::Unbounded);
        assert_eq!(run(BoundTarget::Net), OmegaVerdict::Unbounded);
    }
}
