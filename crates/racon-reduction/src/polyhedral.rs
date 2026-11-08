//! Polyhedral abstraction for Petri nets (N. Amat, B. Berthomieu,
//! S. Dal Zilio, *A Polyhedral Abstraction for Petri Nets and its Application
//! to SMT-Based Model Checking*, Fundamenta Informaticae 187(2-4), 2022;
//! `References/Bibliografia/2104.09850v5.pdf`, R11).
//!
//! A reduction produces a triple `(N, E, N')`: the reduced net `N'` and a
//! system `E` of linear equations relating the places of `N` to those of
//! `N'` (Section 3). `E` is kept as an ordered list of [`Equation`]s, so that
//! the markings of `N` can be rebuilt from a marking of `N'` ([`lift`]).
//!
//! Rules implemented (marking-level versions of Section 3.4; the observation
//! sequences / labels of the paper are not tracked, so the equivalence holds
//! for the reachable *markings*, which is what the properties below observe):
//!
//! * `DEADP`    — unmarked place without feeders: `p = 0` ([DEADT] / [CONSTANT]);
//! * `CONSTANT` — place with `Pre = Post` on every transition: `p = m0(p)`;
//! * `CONCAT`   — chain `y1 -τ-> y2`, `y2` unmarked and fed only by `τ`:
//!                `x = y1 + y2` (Fig. 2);
//! * `AGG`      — loop `y1 <-τ-> y2`: `x = y1 + y2` (Fig. 3);
//! * `RED`      — redundant place `z = y + C` or `z = y1 + y2 + C` whose
//!                enabling conditions are implied (generalises [RED], [SHORTCUT]).
//!
//! Not implemented: the weighted / longer-cycle variants of CONCAT and AGG,
//! [REDT], [SOURCE], and the SMT-backed procedures of Section 5 as an
//! in-process solver. No SMT solver is available in the workspace: properties
//! are decided by *explicit* exploration of `N'` (Theorem 4.3), and
//! [`PolyhedralReduction::bmc_smtlib`] emits the BMC encoding of Section 5.2
//! for an external solver (not executed here).
//!
//! Checking `EF F` on `N` is done on `N'` with the E-transform
//! `F' = ∃x. E(x, y) ∧ F(x)` (Definition 4.1), evaluated by enumerating the
//! solutions of `E` for each reachable marking of `N'`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId};

use crate::error::ReductionError;

// ── properties ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
    Ne,
}

/// Boolean combination of linear constraints on place markings (Sect. 2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    True,
    /// `Σ coeff·m(place) ⋈ rhs`
    Atom { terms: Vec<(PlaceId, i64)>, op: Cmp, rhs: i64 },
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

impl Formula {
    pub fn eval(&self, marking: &[u64]) -> bool {
        match self {
            Formula::True => true,
            Formula::Atom { terms, op, rhs } => {
                let sum: i128 =
                    terms.iter().map(|(p, c)| i128::from(*c) * i128::from(marking[p.0 as usize])).sum();
                let rhs = i128::from(*rhs);
                match op {
                    Cmp::Le => sum <= rhs,
                    Cmp::Lt => sum < rhs,
                    Cmp::Ge => sum >= rhs,
                    Cmp::Gt => sum > rhs,
                    Cmp::Eq => sum == rhs,
                    Cmp::Ne => sum != rhs,
                }
            }
            Formula::Not(f) => !f.eval(marking),
            Formula::And(fs) => fs.iter().all(|f| f.eval(marking)),
            Formula::Or(fs) => fs.iter().any(|f| f.eval(marking)),
        }
    }
}

// ── reduction constraints E ──────────────────────────────────────────────────

/// One equation of `E`. Variables are indices into
/// [`PolyhedralReduction::variables`]: the places of the original net first,
/// then the fresh place variables introduced by CONCAT / AGG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Equation {
    /// `var = value`
    Const { var: usize, value: u64 },
    /// `var = Σ coeff·term + constant`
    Linear { var: usize, terms: Vec<(usize, u64)>, constant: u64 },
    /// `fused = left + right` with `left, right ≥ 0` free.
    Split { fused: usize, left: usize, right: usize },
}

#[derive(Debug, Clone)]
struct Work {
    names: Vec<String>,
    alive: Vec<bool>,
    m0: Vec<u64>,
    t_names: Vec<String>,
    t_alive: Vec<bool>,
    pre: Vec<BTreeMap<usize, u64>>,
    post: Vec<BTreeMap<usize, u64>>,
}

impl Work {
    fn alive_p(&self) -> Vec<usize> {
        (0..self.names.len()).filter(|&p| self.alive[p]).collect()
    }
    fn alive_t(&self) -> Vec<usize> {
        (0..self.pre.len()).filter(|&t| self.t_alive[t]).collect()
    }
    fn get(map: &BTreeMap<usize, u64>, p: usize) -> u64 {
        map.get(&p).copied().unwrap_or(0)
    }
    fn effect(&self, t: usize, p: usize) -> i128 {
        i128::from(Self::get(&self.post[t], p)) - i128::from(Self::get(&self.pre[t], p))
    }
    fn touches(&self, t: usize, p: usize) -> bool {
        self.pre[t].contains_key(&p) || self.post[t].contains_key(&p)
    }
    fn kill_place(&mut self, p: usize) {
        self.alive[p] = false;
        for t in 0..self.pre.len() {
            self.pre[t].remove(&p);
            self.post[t].remove(&p);
        }
    }
    fn feeders(&self, p: usize) -> Vec<usize> {
        self.alive_t().into_iter().filter(|&t| self.post[t].contains_key(&p)).collect()
    }
    fn consumers(&self, p: usize) -> Vec<usize> {
        self.alive_t().into_iter().filter(|&t| self.pre[t].contains_key(&p)).collect()
    }
    /// Replaces places `a` and `b` by a fresh place and returns its index.
    fn fuse(&mut self, a: usize, b: usize, name: String) -> usize {
        let x = self.names.len();
        self.names.push(name);
        self.alive.push(true);
        self.m0.push(self.m0[a] + self.m0[b]);
        for t in 0..self.pre.len() {
            for map in [&mut self.pre[t], &mut self.post[t]] {
                let total = map.remove(&a).unwrap_or(0) + map.remove(&b).unwrap_or(0);
                if total > 0 {
                    map.insert(x, total);
                }
            }
        }
        self.alive[a] = false;
        self.alive[b] = false;
        x
    }
}

/// Result of [`reduce`]: the triple `(N, E, N')`.
#[derive(Debug, Clone)]
pub struct PolyhedralReduction {
    original_places: usize,
    work: Work,
    pub equations: Vec<Equation>,
    /// Number of applications per rule name.
    pub applications: BTreeMap<&'static str, usize>,
}

pub fn reduce(net: &PetriNet) -> Result<PolyhedralReduction, ReductionError> {
    const NAME: &str = "polyhedral_abstraction";
    if net.is_extended() {
        return Err(ReductionError::NotApplicable {
            name: NAME,
            reason: "nets with inhibitor arcs or immediate-transition precedence are not supported",
        });
    }
    let mut m0 = Vec::new();
    for token in &net.initial_marking().tokens {
        match token {
            TokenValue::Finite(n) => m0.push(*n),
            _ => {
                return Err(ReductionError::NotApplicable {
                    name: NAME,
                    reason: "initial marking is not a finite u64 vector",
                })
            }
        }
    }
    let mut pre = Vec::new();
    let mut post = Vec::new();
    for t in net.transitions() {
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        for &(p, w) in net.preset(t.id) {
            *a.entry(p.0 as usize).or_insert(0) += u64::from(w);
        }
        for &(p, w) in net.postset(t.id) {
            *b.entry(p.0 as usize).or_insert(0) += u64::from(w);
        }
        pre.push(a);
        post.push(b);
    }
    let n = net.place_count();
    let mut w = Work {
        names: net.places().iter().map(|p| p.name.clone()).collect(),
        alive: vec![true; n],
        m0,
        t_names: net.transitions().iter().map(|t| t.name.clone()).collect(),
        t_alive: vec![true; net.transitions().len()],
        pre,
        post,
    };
    let mut equations = Vec::new();
    let mut applications: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut fresh = 0usize;
    loop {
        let applied = rule_dead(&mut w, &mut equations)
            .or_else(|| rule_constant(&mut w, &mut equations))
            .or_else(|| rule_concat(&mut w, &mut equations, &mut fresh))
            .or_else(|| rule_agg(&mut w, &mut equations, &mut fresh))
            .or_else(|| rule_red(&mut w, &mut equations));
        match applied {
            Some(name) => *applications.entry(name).or_insert(0) += 1,
            None => break,
        }
    }
    Ok(PolyhedralReduction { original_places: n, work: w, equations, applications })
}

fn rule_dead(w: &mut Work, eqs: &mut Vec<Equation>) -> Option<&'static str> {
    for p in w.alive_p() {
        if w.m0[p] == 0 && w.feeders(p).is_empty() {
            for t in w.consumers(p) {
                w.t_alive[t] = false;
            }
            eqs.push(Equation::Const { var: p, value: 0 });
            w.kill_place(p);
            return Some("DEADP");
        }
    }
    None
}

fn rule_constant(w: &mut Work, eqs: &mut Vec<Equation>) -> Option<&'static str> {
    for p in w.alive_p() {
        let constant = w.alive_t().into_iter().all(|t| Work::get(&w.pre[t], p) == Work::get(&w.post[t], p));
        if !constant {
            continue;
        }
        for t in w.consumers(p) {
            if Work::get(&w.pre[t], p) > w.m0[p] {
                w.t_alive[t] = false;
            }
        }
        eqs.push(Equation::Const { var: p, value: w.m0[p] });
        w.kill_place(p);
        return Some("CONSTANT");
    }
    None
}

fn rule_concat(w: &mut Work, eqs: &mut Vec<Equation>, fresh: &mut usize) -> Option<&'static str> {
    for tau in w.alive_t() {
        let (Some((&y1, &1)), Some((&y2, &1))) = (
            w.pre[tau].iter().next().filter(|_| w.pre[tau].len() == 1),
            w.post[tau].iter().next().filter(|_| w.post[tau].len() == 1),
        ) else {
            continue;
        };
        if y1 == y2 || w.m0[y2] != 0 || w.feeders(y2) != vec![tau] {
            continue;
        }
        if w.alive_t().into_iter().any(|t| t != tau && w.touches(t, y1) && w.touches(t, y2)) {
            continue;
        }
        w.t_alive[tau] = false;
        *fresh += 1;
        let x = w.fuse(y1, y2, format!("a{fresh}"));
        eqs.push(Equation::Split { fused: x, left: y1, right: y2 });
        return Some("CONCAT");
    }
    None
}

fn rule_agg(w: &mut Work, eqs: &mut Vec<Equation>, fresh: &mut usize) -> Option<&'static str> {
    let single = |m: &BTreeMap<usize, u64>| -> Option<usize> {
        (m.len() == 1).then(|| m.iter().next().filter(|(_, v)| **v == 1).map(|(k, _)| *k)).flatten()
    };
    let ts = w.alive_t();
    for &t1 in &ts {
        let (Some(a), Some(b)) = (single(&w.pre[t1]), single(&w.post[t1])) else { continue };
        if a == b {
            continue;
        }
        for &t2 in &ts {
            if t2 == t1 || single(&w.pre[t2]) != Some(b) || single(&w.post[t2]) != Some(a) {
                continue;
            }
            if ts.iter().any(|&t| t != t1 && t != t2 && w.touches(t, a) && w.touches(t, b)) {
                continue;
            }
            w.t_alive[t1] = false;
            w.t_alive[t2] = false;
            *fresh += 1;
            let x = w.fuse(a, b, format!("a{fresh}"));
            eqs.push(Equation::Split { fused: x, left: a, right: b });
            return Some("AGG");
        }
    }
    None
}

fn rule_red(w: &mut Work, eqs: &mut Vec<Equation>) -> Option<&'static str> {
    let places = w.alive_p();
    let ts = w.alive_t();
    for &z in &places {
        let mut candidates: Vec<Vec<usize>> = places.iter().filter(|&&y| y != z).map(|&y| vec![y]).collect();
        for (i, &y1) in places.iter().enumerate() {
            for &y2 in &places[i + 1..] {
                if y1 != z && y2 != z {
                    candidates.push(vec![y1, y2]);
                }
            }
        }
        for ys in candidates {
            let invariant = ts.iter().all(|&t| {
                w.effect(t, z) == ys.iter().map(|&y| w.effect(t, y)).sum::<i128>()
            });
            if !invariant {
                continue;
            }
            let c = i128::from(w.m0[z]) - ys.iter().map(|&y| i128::from(w.m0[y])).sum::<i128>();
            if c < 0 {
                continue;
            }
            let implied = ts.iter().all(|&t| {
                i128::from(Work::get(&w.pre[t], z))
                    <= ys.iter().map(|&y| i128::from(Work::get(&w.pre[t], y))).sum::<i128>() + c
            });
            if !implied {
                continue;
            }
            eqs.push(Equation::Linear {
                var: z,
                terms: ys.iter().map(|&y| (y, 1)).collect(),
                constant: c as u64,
            });
            w.kill_place(z);
            return Some("RED");
        }
    }
    None
}

// ── lifting and verification ─────────────────────────────────────────────────

/// Result of [`PolyhedralReduction::check_reachable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// `EF F` holds; marking of the *original* net satisfying `F`, and the
    /// reduced marking it comes from.
    Reachable { original_marking: Vec<u64>, reduced_marking: Vec<u64> },
    /// Exhaustive search of the reduced net finished: `F` is not reachable.
    Unreachable,
    /// A resource limit was hit before a conclusion.
    Unknown { explored: usize },
}

#[derive(Debug, Clone, Copy)]
pub struct CheckConfig {
    pub max_states: usize,
    /// Cap on the solutions of `E` enumerated for a single reduced marking.
    pub max_lifts_per_state: usize,
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self { max_states: 1_000_000, max_lifts_per_state: 100_000 }
    }
}

impl PolyhedralReduction {
    /// Names of the variables of `E`: original places, then fresh ones.
    pub fn variables(&self) -> &[String] {
        &self.work.names
    }
    pub fn original_place_count(&self) -> usize {
        self.original_places
    }
    pub fn remaining_places(&self) -> Vec<&str> {
        self.work.alive_p().into_iter().map(|p| self.work.names[p].as_str()).collect()
    }
    pub fn remaining_transitions(&self) -> Vec<&str> {
        self.work.alive_t().into_iter().map(|t| self.work.t_names[t].as_str()).collect()
    }
    pub fn initial_marking(&self) -> Vec<u64> {
        self.work.alive_p().into_iter().map(|p| self.work.m0[p]).collect()
    }

    /// `E` as readable equations, in the order they were produced.
    pub fn equation_strings(&self) -> Vec<String> {
        let n = &self.work.names;
        self.equations
            .iter()
            .map(|e| match e {
                Equation::Const { var, value } => format!("{} = {}", n[*var], value),
                Equation::Linear { var, terms, constant } => {
                    let mut parts: Vec<String> = terms.iter().map(|(t, c)| {
                        if *c == 1 { n[*t].clone() } else { format!("{c}·{}", n[*t]) }
                    }).collect();
                    if *constant > 0 || parts.is_empty() {
                        parts.push(constant.to_string());
                    }
                    format!("{} = {}", n[*var], parts.join(" + "))
                }
                Equation::Split { fused, left, right } => {
                    format!("{} = {} + {}", n[*fused], n[*left], n[*right])
                }
            })
            .collect()
    }

    /// The reduced net `N'`; `None` when it is empty or not representable
    /// (no place, no transition or no arc left).
    pub fn reduced_net(&self) -> Option<PetriNet> {
        let ps = self.work.alive_p();
        let ts = self.work.alive_t();
        let arcs: usize = ts.iter().map(|&t| self.work.pre[t].len() + self.work.post[t].len()).sum();
        if ps.is_empty() || ts.is_empty() || arcs == 0 {
            return None;
        }
        let index: BTreeMap<usize, usize> = ps.iter().enumerate().map(|(i, p)| (*p, i)).collect();
        let mut b = PetriNetBuilder::default();
        for &p in &ps {
            b.add_place(self.work.names[p].clone(), None);
        }
        for &t in &ts {
            let tid = b.add_transition(self.work.t_names[t].clone(), None);
            for (p, w) in &self.work.pre[t] {
                b.add_arc(ArcEndpoint::Place(PlaceId(index[p] as u32)), ArcEndpoint::Transition(tid), *w as u32);
            }
            for (p, w) in &self.work.post[t] {
                b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(index[p] as u32)), *w as u32);
            }
        }
        b.build(ps.iter().map(|&p| TokenValue::Finite(self.work.m0[p])).collect()).ok()
    }

    /// Visits every marking of the original net compatible with `reduced`
    /// (a marking of the remaining places) under `E`; stops when `visit`
    /// returns `true`.
    fn lift_rec(&self, idx: usize, vals: &mut Vec<Option<u64>>, visit: &mut dyn FnMut(&[u64]) -> bool) -> bool {
        if idx == 0 {
            let marking: Vec<u64> = vals[..self.original_places].iter().map(|v| v.unwrap_or(0)).collect();
            return visit(&marking);
        }
        match &self.equations[idx - 1] {
            Equation::Const { var, value } => {
                vals[*var] = Some(*value);
                self.lift_rec(idx - 1, vals, visit)
            }
            Equation::Linear { var, terms, constant } => {
                let sum: u64 = terms.iter().map(|(t, c)| c * vals[*t].unwrap_or(0)).sum::<u64>() + constant;
                vals[*var] = Some(sum);
                self.lift_rec(idx - 1, vals, visit)
            }
            Equation::Split { fused, left, right } => {
                let total = vals[*fused].unwrap_or(0);
                for l in 0..=total {
                    vals[*left] = Some(l);
                    vals[*right] = Some(total - l);
                    if self.lift_rec(idx - 1, vals, visit) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// All markings of the original net that satisfy `E` together with the
    /// given marking of the remaining places.
    pub fn lift(&self, reduced: &[u64]) -> Vec<Vec<u64>> {
        let mut out = Vec::new();
        self.for_each_lift(reduced, &mut |m| {
            out.push(m.to_vec());
            false
        });
        out
    }

    fn for_each_lift(&self, reduced: &[u64], visit: &mut dyn FnMut(&[u64]) -> bool) -> bool {
        let mut vals: Vec<Option<u64>> = vec![None; self.work.names.len()];
        for (value, p) in reduced.iter().zip(self.work.alive_p()) {
            vals[p] = Some(*value);
        }
        self.lift_rec(self.equations.len(), &mut vals, visit)
    }

    /// Decides `EF formula` on the *original* net by exhaustive exploration
    /// of the reduced net, testing the E-transform of `formula` (Theorem 4.3)
    /// on every reachable reduced marking.
    pub fn check_reachable(&self, formula: &Formula, config: &CheckConfig) -> Verdict {
        let places = self.work.alive_p();
        let ts = self.work.alive_t();
        let index: BTreeMap<usize, usize> = places.iter().enumerate().map(|(i, p)| (*p, i)).collect();
        let arcs: Vec<(Vec<(usize, u64)>, Vec<(usize, u64)>)> = ts
            .iter()
            .map(|&t| {
                (
                    self.work.pre[t].iter().map(|(p, w)| (index[p], *w)).collect(),
                    self.work.post[t].iter().map(|(p, w)| (index[p], *w)).collect(),
                )
            })
            .collect();
        let start = self.initial_marking();
        let mut seen: BTreeSet<Vec<u64>> = BTreeSet::from([start.clone()]);
        let mut queue = VecDeque::from([start]);
        while let Some(m) = queue.pop_front() {
            let mut found: Option<Vec<u64>> = None;
            let mut lifts = 0usize;
            let capped = {
                let mut capped = false;
                self.for_each_lift(&m, &mut |orig| {
                    lifts += 1;
                    if formula.eval(orig) {
                        found = Some(orig.to_vec());
                        return true;
                    }
                    if lifts >= config.max_lifts_per_state {
                        capped = true;
                        return true;
                    }
                    false
                });
                capped
            };
            if let Some(original_marking) = found {
                return Verdict::Reachable { original_marking, reduced_marking: m };
            }
            if capped {
                return Verdict::Unknown { explored: seen.len() };
            }
            for (pre, post) in &arcs {
                if pre.iter().all(|&(p, w)| m[p] >= w) {
                    let mut n = m.clone();
                    for &(p, w) in pre {
                        n[p] -= w;
                    }
                    for &(p, w) in post {
                        n[p] += w;
                    }
                    if seen.insert(n.clone()) {
                        if seen.len() > config.max_states {
                            return Verdict::Unknown { explored: seen.len() };
                        }
                        queue.push_back(n);
                    }
                }
            }
        }
        Verdict::Unreachable
    }

    /// SMT-LIB 2 script for BMC of `EF formula` on the reduced net up to
    /// `depth` steps (Section 5.2 with the E-transform of Section 4): for
    /// every `k ≤ depth`, `m0(x_0) ∧ T(x_0,x_1) ∧ … ∧ T(x_{k-1},x_k) ∧ Ẽ ∧ F`
    /// is checked with `push/pop`. `sat` at some `k` means `EF formula`.
    ///
    /// The script is generated text; this workspace has no SMT solver, so it
    /// has not been run here.
    pub fn bmc_smtlib(&self, formula: &Formula, depth: usize) -> String {
        let q = |s: &str| format!("|{s}|");
        let ps = self.work.alive_p();
        let ts = self.work.alive_t();
        let mut out = String::from("(set-logic QF_LIA)\n");
        let state = |k: usize, p: usize| q(&format!("{}@{k}", self.work.names[p]));
        for k in 0..=depth {
            for &p in &ps {
                out += &format!("(declare-const {} Int)\n(assert (>= {} 0))\n", state(k, p), state(k, p));
            }
        }
        for &p in &ps {
            out += &format!("(assert (= {} {}))\n", state(0, p), self.work.m0[p]);
        }
        for k in 0..depth {
            let mut moves = vec![format!(
                "(and {})",
                ps.iter().map(|&p| format!("(= {} {})", state(k, p), state(k + 1, p))).collect::<Vec<_>>().join(" ")
            )];
            for &t in &ts {
                let mut parts = Vec::new();
                for &p in &ps {
                    let pre = Work::get(&self.work.pre[t], p);
                    let post = Work::get(&self.work.post[t], p);
                    parts.push(format!("(>= {} {})", state(k, p), pre));
                    parts.push(format!("(= {} (+ {} {}))", state(k + 1, p), state(k, p), post as i128 - pre as i128));
                }
                moves.push(format!("(and {})", parts.join(" ")));
            }
            out += &format!("(assert (or {}))\n", moves.join(" "));
        }
        let var = |k: usize, v: usize| -> String {
            if self.work.alive[v] { state(k, v) } else { q(&format!("{}#{k}", self.work.names[v])) }
        };
        for k in 0..=depth {
            out += "(push 1)\n";
            for v in 0..self.work.names.len() {
                if !self.work.alive[v] {
                    out += &format!("(declare-const {} Int)\n(assert (>= {} 0))\n", var(k, v), var(k, v));
                }
            }
            for e in &self.equations {
                out += &match e {
                    Equation::Const { var: v, value } => format!("(assert (= {} {}))\n", var(k, *v), value),
                    Equation::Linear { var: v, terms, constant } => {
                        let mut parts: Vec<String> =
                            terms.iter().map(|(t, c)| format!("(* {c} {})", var(k, *t))).collect();
                        parts.push(constant.to_string());
                        format!("(assert (= {} (+ {})))\n", var(k, *v), parts.join(" "))
                    }
                    Equation::Split { fused, left, right } => {
                        format!("(assert (= {} (+ {} {})))\n", var(k, *fused), var(k, *left), var(k, *right))
                    }
                };
            }
            out += &format!("(assert {})\n", self.formula_smt(formula, &|p| var(k, p)));
            out += "(check-sat)\n(pop 1)\n";
        }
        out
    }

    fn formula_smt(&self, f: &Formula, var: &dyn Fn(usize) -> String) -> String {
        match f {
            Formula::True => "true".to_owned(),
            Formula::Atom { terms, op, rhs } => {
                let lit = |n: i64| if n < 0 { format!("(- {})", n.unsigned_abs()) } else { n.to_string() };
                let sum = if terms.is_empty() {
                    "0".to_owned()
                } else {
                    format!(
                        "(+ {})",
                        terms.iter().map(|(p, c)| format!("(* {} {})", lit(*c), var(p.0 as usize))).collect::<Vec<_>>().join(" ")
                    )
                };
                let rhs = lit(*rhs);
                match op {
                    Cmp::Le => format!("(<= {sum} {rhs})"),
                    Cmp::Lt => format!("(< {sum} {rhs})"),
                    Cmp::Ge => format!("(>= {sum} {rhs})"),
                    Cmp::Gt => format!("(> {sum} {rhs})"),
                    Cmp::Eq => format!("(= {sum} {rhs})"),
                    Cmp::Ne => format!("(not (= {sum} {rhs}))"),
                }
            }
            Formula::Not(g) => format!("(not {})", self.formula_smt(g, var)),
            Formula::And(gs) => {
                format!("(and true {})", gs.iter().map(|g| self.formula_smt(g, var)).collect::<Vec<_>>().join(" "))
            }
            Formula::Or(gs) => {
                format!("(or false {})", gs.iter().map(|g| self.formula_smt(g, var)).collect::<Vec<_>>().join(" "))
            }
        }
    }
}
