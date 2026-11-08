//! `MinCov` — minimal coverability set construction.
//!
//! Faithful implementation of **Algorithm 1** of
//! A. Finkel, S. Haddad, I. Khmelnitsky, *Minimal Coverability Tree
//! Construction Made Complete and Efficient*, FoSSaCS 2020, LNCS 12077,
//! pp. 237-256, DOI 10.1007/978-3-030-45231-5_13 [R6 of the RACoN algorithm
//! specification].
//!
//! Notation follows the paper: `Pre(a)`, `C(a)` for ω-transitions
//! (Definition 4), `Pre(σ)`/`C(σ)` of sequences (recurrence after Def. 4),
//! accelerations from Proposition 3, `trunc` from Proposition 7. Line numbers
//! in comments refer to Algorithm 1.
//!
//! The result is the *set* `Clover(N, m0)` of maximal ω-markings, **not** a
//! transition graph: it answers coverability and boundedness queries only and
//! must never be used for reachability, liveness, reversibility or CTL.

use std::collections::{BTreeSet, HashSet};

use racon_core::execution::AnalysisContext;
use racon_core::marking::TokenValue;
use racon_core::net::PetriNet;

use crate::error::{CoverabilityError, MONOTONICITY_REQUIRED};

/// An element of `N ∪ {ω}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OmegaValue {
    Finite(u64),
    Omega,
}

impl OmegaValue {
    /// `self ≥ other` in the order of `N_ω` (`ω > n` for every `n ∈ N`).
    pub(crate) fn geq(self, other: Self) -> bool {
        match (self, other) {
            (Self::Omega, _) => true,
            (Self::Finite(_), Self::Omega) => false,
            (Self::Finite(a), Self::Finite(b)) => a >= b,
        }
    }
}

type OmegaMarking = Vec<OmegaValue>;

pub(crate) fn marking_ge(a: &[OmegaValue], b: &[OmegaValue]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.geq(*y))
}

fn marking_gt(a: &[OmegaValue], b: &[OmegaValue]) -> bool {
    marking_ge(a, b) && a != b
}

/// Incidence entry in `Z ∪ {ω}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Incidence {
    Finite(i128),
    Omega,
}

/// ω-transition (Definition 4).
#[derive(Debug, Clone)]
struct OmegaTransition {
    pre: Vec<OmegaValue>,
    c: Vec<Incidence>,
}

impl OmegaTransition {
    fn fireable_from(&self, m: &[OmegaValue]) -> bool {
        marking_ge(m, &self.pre)
    }

    /// `m' = m + C(a)` for a fireable ω-transition.
    fn fire(&self, m: &[OmegaValue]) -> Result<OmegaMarking, CoverabilityError> {
        m.iter()
            .zip(&self.c)
            .map(|(value, delta)| match (value, delta) {
                (OmegaValue::Omega, _) | (_, Incidence::Omega) => Ok(OmegaValue::Omega),
                (OmegaValue::Finite(n), Incidence::Finite(d)) => {
                    let sum = i128::from(*n) + d;
                    u64::try_from(sum).map(OmegaValue::Finite).map_err(|_| {
                        CoverabilityError::ArithmeticOverflow(format!(
                            "token count {sum} is outside 0..=u64::MAX"
                        ))
                    })
                }
            })
            .collect()
    }
}

/// `Pre(σ)` and `C(σ)` of a sequence of ω-transitions, by the recurrence
/// following Definition 4: for `σ = t σ'`,
/// `C(σ) = C(t) + C(σ')`; per place, `Pre(σ) = Pre(t)` if `C(t) = ω`, else
/// `max(Pre(t), Pre(σ') − C(t))`.
fn sequence_incidence(
    sequence: &[&OmegaTransition],
    places: usize,
) -> (Vec<OmegaValue>, Vec<Incidence>) {
    let mut pre = vec![OmegaValue::Finite(0); places];
    let mut c = vec![Incidence::Finite(0); places];
    for t in sequence.iter().rev() {
        for p in 0..places {
            let new_pre = match t.c[p] {
                Incidence::Omega => t.pre[p],
                Incidence::Finite(ct) => {
                    let shifted = match pre[p] {
                        OmegaValue::Omega => OmegaValue::Omega,
                        OmegaValue::Finite(rest) => {
                            // Pre(σ') − C(t); a negative value is dominated by
                            // Pre(t) ≥ 0 in the max below.
                            let diff = i128::from(rest) - ct;
                            OmegaValue::Finite(u64::try_from(diff.max(0)).unwrap_or(u64::MAX))
                        }
                    };
                    if t.pre[p].geq(shifted) {
                        t.pre[p]
                    } else {
                        shifted
                    }
                }
            };
            let new_c = match (t.c[p], c[p]) {
                (Incidence::Omega, _) | (_, Incidence::Omega) => Incidence::Omega,
                (Incidence::Finite(a), Incidence::Finite(b)) => Incidence::Finite(a + b),
            };
            pre[p] = new_pre;
            c[p] = new_c;
        }
    }
    (pre, c)
}

/// One element of an edge label `δ(e) ∈ T_o · Acc*`.
#[derive(Debug, Clone, Copy)]
enum Step {
    Transition(usize),
    Acceleration(usize),
}

#[derive(Debug)]
struct Vertex {
    parent: usize,
    children: Vec<usize>,
    marking: OmegaMarking,
    label: Vec<Step>,
    alive: bool,
}

/// Resource limits for [`MinCov`].
#[derive(Debug, Clone, Copy)]
pub struct MinCovConfig {
    /// Maximum number of simultaneously live tree vertices.
    pub max_vertices: usize,
}

impl Default for MinCovConfig {
    fn default() -> Self {
        Self {
            max_vertices: 5_000_000,
        }
    }
}

/// `Clover(N, m0)`: the finite antichain of maximal ω-markings whose
/// downward closure equals `Cover(N, m0)` (R6, Definition 3 and §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalCoverabilitySet {
    elements: Vec<Vec<OmegaValue>>,
    pub tree_vertices: usize,
    pub accelerations: usize,
    pub iterations: usize,
}

impl MinimalCoverabilitySet {
    pub(crate) fn from_parts(
        elements: Vec<Vec<OmegaValue>>,
        tree_vertices: usize,
        accelerations: usize,
        iterations: usize,
    ) -> Self {
        Self { elements, tree_vertices, accelerations, iterations }
    }

    /// The ω-markings, sorted for deterministic output.
    pub fn elements(&self) -> &[Vec<OmegaValue>] {
        &self.elements
    }

    /// A net is bounded iff `Cover` is finite iff no element of `Clover`
    /// contains ω.
    pub fn is_bounded(&self) -> bool {
        self.elements
            .iter()
            .all(|m| m.iter().all(|v| *v != OmegaValue::Omega))
    }

    /// Places that are unbounded (ω in some element of `Clover`).
    pub fn unbounded_places(&self) -> Vec<usize> {
        let places = self.elements.first().map_or(0, Vec::len);
        (0..places)
            .filter(|&p| self.elements.iter().any(|m| m[p] == OmegaValue::Omega))
            .collect()
    }

    /// Whether `marking` is covered by a reachable marking:
    /// `marking ∈ ↓Clover`.
    pub fn covers(&self, marking: &[u64]) -> bool {
        let target: Vec<OmegaValue> = marking.iter().map(|&n| OmegaValue::Finite(n)).collect();
        self.elements.iter().any(|m| marking_ge(m, &target))
    }
}

/// The MinCov algorithm.
#[derive(Debug, Default)]
pub struct MinCov {
    pub config: MinCovConfig,
}

impl MinCov {
    pub fn new(config: MinCovConfig) -> Self {
        Self { config }
    }

    /// Computes `Clover(net, m0)`.
    ///
    /// Only valid for ordinary weighted P/T nets: nets with inhibitor arcs or
    /// immediate-transition precedence are refused (the algorithm relies on
    /// monotonicity of the firing relation).
    pub fn compute(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<MinimalCoverabilitySet, CoverabilityError> {
        if net.is_extended() {
            return Err(CoverabilityError::Unsupported(MONOTONICITY_REQUIRED.to_owned()));
        }
        let places = net.place_count();

        // Ordinary transitions as ω-transitions with finite entries.
        let transitions: Vec<OmegaTransition> = net
            .transitions()
            .iter()
            .map(|t| {
                let mut pre = vec![OmegaValue::Finite(0); places];
                let mut c = vec![0i128; places];
                for &(p, w) in net.preset(t.id) {
                    pre[p.0 as usize] = OmegaValue::Finite(u64::from(w));
                    c[p.0 as usize] -= i128::from(w);
                }
                for &(p, w) in net.postset(t.id) {
                    c[p.0 as usize] += i128::from(w);
                }
                OmegaTransition {
                    pre,
                    c: c.into_iter().map(Incidence::Finite).collect(),
                }
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

        // `e` and `d` of Proposition 6/7 (used only to justify `trunc`).
        let d = places;
        let e = transitions
            .iter()
            .flat_map(|t| {
                (0..places).flat_map(move |p| {
                    let pre = match t.pre[p] {
                        OmegaValue::Finite(n) => i128::from(n),
                        OmegaValue::Omega => 0,
                    };
                    let post = match t.c[p] {
                        Incidence::Finite(c) => pre + c,
                        Incidence::Omega => 0,
                    };
                    [pre, post]
                })
            })
            .max()
            .unwrap_or(0);

        let mut accelerations: Vec<OmegaTransition> = Vec::new();
        let mut tree = vec![Vertex {
            parent: 0,
            children: Vec::new(),
            marking: m0,
            label: Vec::new(),
            alive: true,
        }];
        let mut front: BTreeSet<usize> = BTreeSet::from([0]);
        let mut live = 1usize;
        let mut iterations = 0usize;

        // line 2
        while let Some(&u) = front.iter().next() {
            context
                .check_cancellation()
                .map_err(|_| CoverabilityError::Cancelled)?;
            iterations += 1;

            // lines 4-6: fire a maximal sequence of accelerations. Firing an
            // acceleration only adds ω's, which only enables more
            // accelerations, so the greedy closure is the maximum.
            loop {
                let m = &tree[u].marking;
                let found = accelerations.iter().position(|a| {
                    a.fireable_from(m)
                        && (0..places)
                            .any(|p| m[p] != OmegaValue::Omega && a.c[p] == Incidence::Omega)
                });
                let Some(index) = found else { break };
                let next = accelerations[index].fire(&tree[u].marking)?;
                tree[u].marking = next;
                tree[u].label.push(Step::Acceleration(index));
            }

            let lambda_u = tree[u].marking.clone();

            // line 7: cleaning
            let covered = (0..tree.len()).any(|v| {
                v != u && tree[v].alive && !front.contains(&v) && marking_ge(&tree[v].marking, &lambda_u)
            });
            if covered {
                delete(&mut tree, &mut front, &mut live, u);
                continue;
            }

            // line 8: acceleration found against a (strict) ancestor
            let mut ancestor = None;
            let mut cursor = u;
            while cursor != 0 {
                cursor = tree[cursor].parent;
                if marking_gt(&lambda_u, &tree[cursor].marking) {
                    ancestor = Some(cursor);
                    break;
                }
            }
            if let Some(u_prime) = ancestor {
                // line 9: path γ from u' to u; flatten δ(γ)
                let mut path = vec![u];
                let mut node = u;
                while tree[node].parent != u_prime {
                    node = tree[node].parent;
                    path.push(node);
                }
                path.reverse();
                let steps: Vec<&OmegaTransition> = path
                    .iter()
                    .flat_map(|&v| tree[v].label.iter())
                    .map(|step| match *step {
                        Step::Transition(i) => &transitions[i],
                        Step::Acceleration(i) => &accelerations[i],
                    })
                    .collect();
                let (pre_seq, c_seq) = sequence_incidence(&steps, places);

                // lines 10-15 (Proposition 3)
                let mut a = OmegaTransition {
                    pre: vec![OmegaValue::Finite(0); places],
                    c: vec![Incidence::Finite(0); places],
                };
                for p in 0..places {
                    match c_seq[p] {
                        Incidence::Finite(x) if x < 0 => {
                            a.pre[p] = OmegaValue::Omega;
                            a.c[p] = Incidence::Omega;
                        }
                        Incidence::Finite(0) => {
                            a.pre[p] = pre_seq[p];
                            a.c[p] = Incidence::Finite(0);
                        }
                        _ => {
                            a.pre[p] = pre_seq[p];
                            a.c[p] = Incidence::Omega;
                        }
                    }
                }
                // line 16: trunc (Proposition 7). The cap
                // e·(3de)^((d+1)^(2d+4)) is at least 3^64 > u64::MAX whenever
                // d ≥ 1 and e ≥ 1, so no u64-representable Pre(p,a) is ever
                // reduced and `trunc` is the identity here.
                debug_assert!(d >= 1 && e >= 1);
                accelerations.push(a);
                prune(&mut tree, &mut front, &mut live, u_prime);
                front.insert(u_prime);
                continue;
            }

            // lines 18-20: remove vertices strictly covered by λ(u)
            for v in 0..tree.len() {
                if v != u && tree[v].alive && marking_gt(&lambda_u, &tree[v].marking) {
                    prune(&mut tree, &mut front, &mut live, v);
                    delete(&mut tree, &mut front, &mut live, v);
                }
            }
            // line 21
            front.remove(&u);
            // lines 22-25: children
            for (index, t) in transitions.iter().enumerate() {
                if !t.fireable_from(&lambda_u) {
                    continue;
                }
                if live >= self.config.max_vertices {
                    return Err(CoverabilityError::NodeLimitExceeded {
                        limit: self.config.max_vertices,
                        current: live,
                    });
                }
                let child_marking = t.fire(&lambda_u)?;
                let id = tree.len();
                tree.push(Vertex {
                    parent: u,
                    children: Vec::new(),
                    marking: child_marking,
                    label: vec![Step::Transition(index)],
                    alive: true,
                });
                tree[u].children.push(id);
                front.insert(id);
                live += 1;
            }
        }

        let mut seen = HashSet::new();
        let mut elements: Vec<Vec<OmegaValue>> = tree
            .iter()
            .filter(|v| v.alive)
            .map(|v| v.marking.clone())
            .filter(|m| seen.insert(m.clone()))
            .collect();
        elements.sort();
        Ok(MinimalCoverabilitySet {
            elements,
            tree_vertices: live,
            accelerations: accelerations.len(),
            iterations,
        })
    }
}

/// `Delete(v)`: removes the leaf `v`.
fn delete(tree: &mut [Vertex], front: &mut BTreeSet<usize>, live: &mut usize, v: usize) {
    debug_assert!(tree[v].children.is_empty(), "Delete applies to leaves only");
    if !tree[v].alive || v == 0 {
        return;
    }
    tree[v].alive = false;
    front.remove(&v);
    let parent = tree[v].parent;
    tree[parent].children.retain(|&c| c != v);
    *live -= 1;
}

/// `Prune(v)`: removes every descendant of `v`, keeping `v`.
fn prune(tree: &mut [Vertex], front: &mut BTreeSet<usize>, live: &mut usize, v: usize) {
    let mut stack = std::mem::take(&mut tree[v].children);
    while let Some(node) = stack.pop() {
        stack.extend(std::mem::take(&mut tree[node].children));
        if tree[node].alive {
            tree[node].alive = false;
            front.remove(&node);
            *live -= 1;
        }
    }
}
