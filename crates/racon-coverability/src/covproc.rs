//! `CovProc` — minimal coverability set by the *covering sequence* of
//! G. Geeraerts, J.-F. Raskin and L. Van Begin, *On the efficient computation
//! of the minimal coverability set for Petri nets*
//! (`References/Bibliografia/covsetpn.pdf`).
//!
//! The paper works on **pairs** of ω-markings ordered by `⊑` and builds the
//! sequence `(V_i, F_i, O_i)` of Section 5 (maximal pairs only). Algorithm 3
//! (`CovProc`) instantiates the oracle with recursive calls started from the
//! accelerated ω-markings, which have strictly more ω's than the caller, so
//! the recursion depth is at most `|P|`.
//!
//! The result is the *set* `MCS(N)` of maximal ω-markings, not a transition
//! graph: it answers coverability and boundedness queries only.

use racon_core::execution::AnalysisContext;
use racon_core::marking::TokenValue;
use racon_core::net::PetriNet;

use crate::error::{CoverabilityError, MONOTONICITY_REQUIRED};
use crate::mincov::{marking_ge, MinimalCoverabilitySet, OmegaValue};

type OmegaMarking = Vec<OmegaValue>;
type Pair = (OmegaMarking, OmegaMarking);

const OMEGA: i128 = i128::MAX;
const MINUS_OMEGA: i128 = i128::MIN;

/// `(m2 ⊖ m1)(p)` in `Z ∪ {−ω, ω}`.
fn difference(m1: &[OmegaValue], m2: &[OmegaValue]) -> Vec<i128> {
    m1.iter()
        .zip(m2)
        .map(|(a, b)| match (a, b) {
            (_, OmegaValue::Omega) => OMEGA,
            (OmegaValue::Omega, _) => MINUS_OMEGA,
            (OmegaValue::Finite(x), OmegaValue::Finite(y)) => i128::from(*y) - i128::from(*x),
        })
        .collect()
}

/// `a ⊑ b`: `a1 ≤ b1`, `a2 ≤ b2` and `(a2⊖a1)(p) ≤ (b2⊖b1)(p)` for all `p`.
fn pair_le(a: &Pair, b: &Pair) -> bool {
    marking_ge(&b.0, &a.0)
        && marking_ge(&b.1, &a.1)
        && difference(&a.0, &a.1)
            .iter()
            .zip(difference(&b.0, &b.1))
            .all(|(x, y)| *x <= y)
}

/// `Max⊑(R)`, without duplicates.
fn max_pairs(items: Vec<Pair>) -> Vec<Pair> {
    let mut unique: Vec<Pair> = Vec::new();
    for item in items {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique
        .iter()
        .filter(|a| !unique.iter().any(|b| *a != b && pair_le(a, b)))
        .cloned()
        .collect()
}

fn covered_by(pair: &Pair, set: &[Pair]) -> bool {
    set.iter().any(|other| pair_le(pair, other))
}

fn flatten(set: &[Pair]) -> Vec<&OmegaMarking> {
    set.iter().map(|(_, m2)| m2).collect()
}

/// `↓≤(Flatten(a)) ⊆ ↓≤(Flatten(b))`.
fn flat_included(a: &[Pair], b: &[Pair]) -> bool {
    flatten(a)
        .iter()
        .all(|m| flatten(b).iter().any(|n| marking_ge(n, m)))
}

/// `AccelPair(m1, m2)` for `m1 ≼ m2`.
fn accel_pair(m1: &[OmegaValue], m2: &[OmegaValue]) -> OmegaMarking {
    m1.iter()
        .zip(m2)
        .map(|(a, b)| if a == b { *a } else { OmegaValue::Omega })
        .collect()
}

/// Resource limits and options for [`CovProc`].
#[derive(Debug, Clone, Copy)]
pub struct CovProcConfig {
    /// Maximum number of pairs held at once in `V ∪ F ∪ O` of one call.
    pub max_pairs: usize,
    /// Enables the recursive oracle of Algorithm 3. With `false` the empty
    /// oracle is used (plain covering sequence), which is also correct.
    pub use_oracle: bool,
}

impl Default for CovProcConfig {
    fn default() -> Self {
        Self { max_pairs: 2_000_000, use_oracle: true }
    }
}

#[derive(Debug, Default)]
pub struct CovProc {
    pub config: CovProcConfig,
}

struct Stats {
    peak_pairs: usize,
    accelerations: usize,
    iterations: usize,
}

struct Transitions {
    pre: Vec<Vec<u64>>,
    post: Vec<Vec<u64>>,
}

impl Transitions {
    fn successors(&self, m: &[OmegaValue]) -> Result<Vec<OmegaMarking>, CoverabilityError> {
        let mut out = Vec::new();
        for t in 0..self.pre.len() {
            if !m.iter().zip(&self.pre[t]).all(|(v, &n)| v.geq(OmegaValue::Finite(n))) {
                continue;
            }
            let next = m
                .iter()
                .enumerate()
                .map(|(p, v)| match v {
                    OmegaValue::Omega => Ok(OmegaValue::Omega),
                    OmegaValue::Finite(n) => n
                        .checked_sub(self.pre[t][p])
                        .and_then(|r| r.checked_add(self.post[t][p]))
                        .map(OmegaValue::Finite)
                        .ok_or_else(|| {
                            CoverabilityError::ArithmeticOverflow(
                                "token count outside 0..=u64::MAX".to_owned(),
                            )
                        }),
                })
                .collect::<Result<OmegaMarking, _>>()?;
            out.push(next);
        }
        Ok(out)
    }
}

impl CovProc {
    pub fn new(config: CovProcConfig) -> Self {
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
        let mut tr = Transitions { pre: Vec::new(), post: Vec::new() };
        for t in net.transitions() {
            let mut pre = vec![0u64; places];
            let mut post = vec![0u64; places];
            for &(p, w) in net.preset(t.id) {
                pre[p.0 as usize] += u64::from(w);
            }
            for &(p, w) in net.postset(t.id) {
                post[p.0 as usize] += u64::from(w);
            }
            tr.pre.push(pre);
            tr.post.push(post);
        }
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

        let mut stats = Stats { peak_pairs: 0, accelerations: 0, iterations: 0 };
        let result = self.cov_proc(&tr, m0, context, &mut stats)?;
        let all: Vec<&OmegaMarking> = flatten(&result);
        let mut elements: Vec<OmegaMarking> = all
            .iter()
            .filter(|m| !all.iter().any(|o| *o != **m && marking_ge(o, m)))
            .map(|m| (*m).clone())
            .collect();
        elements.sort();
        elements.dedup();
        Ok(MinimalCoverabilitySet::from_parts(
            elements,
            stats.peak_pairs,
            stats.accelerations,
            stats.iterations,
        ))
    }

    /// Algorithm 3.
    fn cov_proc(
        &self,
        tr: &Transitions,
        m0: OmegaMarking,
        context: &AnalysisContext,
        stats: &mut Stats,
    ) -> Result<Vec<Pair>, CoverabilityError> {
        let mut o: Vec<Pair> = Vec::new();
        let mut v: Vec<Pair> = Vec::new();
        let mut f: Vec<Pair> = vec![(m0.clone(), m0)];
        loop {
            context.check_cancellation().map_err(|_| CoverabilityError::Cancelled)?;
            stats.iterations += 1;

            // Accel(F_{i-1}): defined only for m1 ≺ m2.
            let accel: Vec<Pair> = f
                .iter()
                .filter(|(m1, m2)| marking_ge(m2, m1) && m1 != m2)
                .map(|(m1, m2)| (m2.clone(), accel_pair(m1, m2)))
                .collect();
            stats.accelerations += accel.len();

            // R_i: recursive calls on a subset of Flatten(Accel(F_{i-1})).
            let mut r: Vec<Pair> = Vec::new();
            if self.config.use_oracle {
                for (_, m) in &accel {
                    let known = flatten(&o).into_iter().chain(flatten(&r));
                    if known.into_iter().any(|c| marking_ge(c, m)) {
                        continue;
                    }
                    r.extend(self.cov_proc(tr, m.clone(), context, stats)?);
                    r = max_pairs(r);
                }
            }

            let new_o = max_pairs(o.iter().cloned().chain(r).collect());
            let new_v: Vec<Pair> = max_pairs(v.iter().chain(&f).cloned().collect())
                .into_iter()
                .filter(|p| !covered_by(p, &new_o))
                .collect();

            let mut candidates: Vec<Pair> = Vec::new();
            for (m1, m2) in &f {
                for m in tr.successors(m2)? {
                    candidates.push((m1.clone(), m.clone()));
                    candidates.push((m2.clone(), m));
                }
            }
            candidates.extend(accel);
            let barrier: Vec<Pair> = new_v.iter().chain(&new_o).cloned().collect();
            let new_f: Vec<Pair> = max_pairs(candidates)
                .into_iter()
                .filter(|p| !covered_by(p, &barrier))
                .collect();

            let size = new_o.len() + new_v.len() + new_f.len();
            stats.peak_pairs = stats.peak_pairs.max(size);
            if size > self.config.max_pairs {
                return Err(CoverabilityError::NodeLimitExceeded {
                    limit: self.config.max_pairs,
                    current: size,
                });
            }

            // until ↓Flatten(O_i ∪ V_i) ⊆ ↓Flatten(O_{i-1} ∪ V_{i-1})
            let new_ov: Vec<Pair> = new_o.iter().chain(&new_v).cloned().collect();
            let old_ov: Vec<Pair> = o.iter().chain(&v).cloned().collect();
            let done = flat_included(&new_ov, &old_ov);
            o = new_o;
            v = new_v;
            f = new_f;
            if done {
                return Ok(o.into_iter().chain(v).collect());
            }
        }
    }
}
