//! Monotone-Pruning (MP) — minimal coverability set by Karp–Miller with
//! pruning between branches.
//!
//! Faithful implementation of **Algorithm 2** of P.-A. Reynier and
//! F. Servais, *Minimal Coverability Set for Petri Nets: Karp and Miller
//! Algorithm with Pruning*, Fundamenta Informaticae XX (2012) 1–30
//! (`References/Bibliografia/fi13.pdf`). Line numbers in comments refer to
//! Algorithm 2 of that paper.
//!
//! Differences from the historical MCT pruning (Finkel 1993): a node `x` is
//! deactivated by the new node `n` iff it has an ancestor `y` (x included) with
//! `Λ(y) ≤ Λ(n)` **and** (`y` is active **or** `y` is not an ancestor of `n`).
//! The simpler condition `Λ(y) ≤ Λ(n)` loses termination (Remark 3.1); the
//! MCT variant loses completeness.
//!
//! The result is the *set* `MCS(N)` of maximal ω-markings, not a transition
//! graph: it answers coverability and boundedness queries only.

use std::collections::VecDeque;

use racon_core::execution::AnalysisContext;
use racon_core::marking::TokenValue;
use racon_core::net::PetriNet;

use crate::error::{CoverabilityError, MONOTONICITY_REQUIRED};
use crate::mincov::{marking_ge, MinimalCoverabilitySet, OmegaValue};

type OmegaMarking = Vec<OmegaValue>;

/// Order in which pairs `(node, transition)` are popped from `Wait`.
/// The paper proves correctness for any strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaitStrategy {
    #[default]
    BreadthFirst,
    DepthFirst,
}

#[derive(Debug, Clone, Copy)]
pub struct MonotonePruningConfig {
    /// Maximum number of tree nodes ever created (active or not).
    pub max_nodes: usize,
    pub strategy: WaitStrategy,
}

impl Default for MonotonePruningConfig {
    fn default() -> Self {
        Self { max_nodes: 5_000_000, strategy: WaitStrategy::default() }
    }
}

struct Node {
    parent: usize,
    marking: OmegaMarking,
    active: bool,
}

#[derive(Debug, Default)]
pub struct MonotonePruning {
    pub config: MonotonePruningConfig,
}

fn gt(a: &[OmegaValue], b: &[OmegaValue]) -> bool {
    marking_ge(a, b) && a != b
}

/// `Acc(M, m)`: `ω` in place `p` iff some `m' ∈ M` has `m' < m` and
/// `m'(p) < m(p) < ω`.
fn accelerate<'a>(
    ancestors: impl Iterator<Item = &'a OmegaMarking>,
    m: &[OmegaValue],
) -> OmegaMarking {
    let mut result = m.to_vec();
    for anc in ancestors {
        if gt(m, anc) {
            for (p, slot) in result.iter_mut().enumerate() {
                if m[p] != OmegaValue::Omega && anc[p] != m[p] {
                    // m' < m implies m'(p) ≤ m(p); inequality means strict.
                    *slot = OmegaValue::Omega;
                }
            }
        }
    }
    result
}

impl MonotonePruning {
    pub fn new(config: MonotonePruningConfig) -> Self {
        Self { config }
    }

    pub fn compute(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<MinimalCoverabilitySet, CoverabilityError> {
        if net.is_extended() {
            return Err(CoverabilityError::Unsupported(MONOTONICITY_REQUIRED.to_owned()));
        }
        let places = net.place_count();
        // (pre, post) per transition.
        let transitions: Vec<(Vec<u64>, Vec<u64>)> = net
            .transitions()
            .iter()
            .map(|t| {
                let mut pre = vec![0u64; places];
                let mut post = vec![0u64; places];
                for &(p, w) in net.preset(t.id) {
                    pre[p.0 as usize] += u64::from(w);
                }
                for &(p, w) in net.postset(t.id) {
                    post[p.0 as usize] += u64::from(w);
                }
                (pre, post)
            })
            .collect();

        let m0: OmegaMarking = net
            .initial_marking()
            .tokens
            .iter()
            .map(|token| match token {
                TokenValue::Finite(n) => Ok(OmegaValue::Finite(*n)),
                _ => Err(CoverabilityError::ArithmeticOverflow(
                    "initial marking outside the u64 range".to_owned(),
                )),
            })
            .collect::<Result<_, _>>()?;

        let enabled = |m: &[OmegaValue], t: usize| {
            m.iter().zip(&transitions[t].0).all(|(v, &need)| v.geq(OmegaValue::Finite(need)))
        };
        let post = |m: &[OmegaValue], t: usize| -> Result<OmegaMarking, CoverabilityError> {
            m.iter()
                .enumerate()
                .map(|(p, v)| match v {
                    OmegaValue::Omega => Ok(OmegaValue::Omega),
                    OmegaValue::Finite(n) => n
                        .checked_sub(transitions[t].0[p])
                        .and_then(|r| r.checked_add(transitions[t].1[p]))
                        .map(OmegaValue::Finite)
                        .ok_or_else(|| {
                            CoverabilityError::ArithmeticOverflow(
                                "token count outside 0..=u64::MAX".to_owned(),
                            )
                        }),
                })
                .collect()
        };

        // line 1-2
        let mut nodes = vec![Node { parent: 0, marking: m0, active: true }];
        let mut wait: VecDeque<(usize, usize)> = VecDeque::new();
        for t in 0..transitions.len() {
            if enabled(&nodes[0].marking, t) {
                wait.push_back((0, t));
            }
        }
        let (mut iterations, mut accelerations) = (0usize, 0usize);

        // line 3
        loop {
            let popped = match self.config.strategy {
                WaitStrategy::BreadthFirst => wait.pop_front(),
                WaitStrategy::DepthFirst => wait.pop_back(),
            };
            let Some((np, t)) = popped else { break };
            context.check_cancellation().map_err(|_| CoverabilityError::Cancelled)?;
            // line 5
            if !nodes[np].active {
                continue;
            }
            iterations += 1;
            // line 6
            let m = post(&nodes[np].marking, t)?;
            // line 7: Acc over the active ancestors of n' (n' included)
            let mut active_anc: Vec<usize> = Vec::new();
            let mut cur = np;
            loop {
                if nodes[cur].active {
                    active_anc.push(cur);
                }
                if cur == 0 {
                    break;
                }
                cur = nodes[cur].parent;
            }
            let lambda = accelerate(active_anc.iter().map(|&i| &nodes[i].marking), &m);
            if lambda != m {
                accelerations += 1;
            }
            // line 8
            if nodes.len() >= self.config.max_nodes {
                return Err(CoverabilityError::NodeLimitExceeded {
                    limit: self.config.max_nodes,
                    current: nodes.len(),
                });
            }
            let n = nodes.len();
            nodes.push(Node { parent: np, marking: lambda, active: false });
            // line 9: Λ(n) ≰ Λ(Act)
            let covered = nodes[..n]
                .iter()
                .any(|x| x.active && marking_ge(&x.marking, &nodes[n].marking));
            if covered {
                continue;
            }
            // line 10: deactivate x iff some ancestor y of x (x included) has
            // Λ(y) ≤ Λ(n) and (y ∈ Act or y ∉ Ancestor(n)).
            let mut anc_of_n = vec![false; n];
            let mut cur = np;
            loop {
                anc_of_n[cur] = true;
                if cur == 0 {
                    break;
                }
                cur = nodes[cur].parent;
            }
            // Parents precede children in index order, so one pass suffices.
            let mut triggered = vec![false; n];
            for x in 0..n {
                let own = marking_ge(&nodes[n].marking, &nodes[x].marking)
                    && (nodes[x].active || !anc_of_n[x]);
                triggered[x] = own || (x != 0 && triggered[nodes[x].parent]);
            }
            for x in 0..n {
                if triggered[x] {
                    nodes[x].active = false;
                }
            }
            // line 11
            nodes[n].active = true;
            for u in 0..transitions.len() {
                if enabled(&nodes[n].marking, u) {
                    wait.push_back((n, u));
                }
            }
        }

        let mut elements: Vec<OmegaMarking> = nodes
            .iter()
            .filter(|x| x.active)
            .map(|x| x.marking.clone())
            .collect();
        elements.sort();
        elements.dedup();
        Ok(MinimalCoverabilitySet::from_parts(
            elements,
            nodes.len(),
            accelerations,
            iterations,
        ))
    }
}
