//! Reduced ordered binary decision diagrams (ROBDD) and symbolic
//! reachability of **1-safe** Petri nets.
//!
//! The data structure is that of R. Bryant, *Graph-Based Algorithms for
//! Boolean Function Manipulation*, IEEE TC 35(8), 1986 (`Bryant_1986.pdf`):
//!
//! * canonical form (Theorem 1): no vertex has `low == high` and no two
//!   vertices share `(index, low, high)`. `Reduce` (Fig. 4) is applied
//!   incrementally: every vertex is created through [`Bdd::mk`], which
//!   consults a *unique table*;
//! * `Apply` / `ITE` (Fig. 6, Sect. 4.5) with a *computed table* of
//!   `(f, g, h) → result`, as suggested in Sect. 6;
//! * `Restrict` (Sect. 4.4), existential quantification
//!   `∃x.f = f|x=0 + f|x=1`, `Satisfy-one`, `Satisfy-all` and
//!   `Satisfy-count` (Sect. 4.6, Figs. 9–10).
//!
//! Encoding: one Boolean variable per place (`x_p = 1` iff the place holds
//! its token), variable index = place index. The reachable set `S` is
//! computed as a least fixed point `S ← S ∪ Img_t(S)` over the transitions
//! (chaining). For a transition `t` with preset `•t` and postset `t•`:
//!
//! ```text
//! Img_t(S) = (∃ (•t ∪ t•). S ∧ ⋀_{p∈•t} x_p)  ∧  ⋀_{p∈t•} x_p  ∧  ⋀_{p∈•t∖t•} ¬x_p
//! ```
//!
//! A transition whose output place is already marked (and is not consumed by
//! the same transition) would put two tokens in one place: the net is then
//! not 1-safe and generation fails with `NotApplicable` instead of returning
//! a wrong set. Arc weights other than 1 are rejected for the same reason;
//! use the MDD engine (`decision_diagrams`) for general bounded nets.
//! Like the MDD engine, the result is the reachable *set* only, not the
//! transition graph.

use std::collections::{BTreeSet, HashMap};

use racon_core::marking::TokenValue;
use racon_core::net::PetriNet;

use crate::error::ReductionError;

/// Handle of a vertex of a [`Bdd`] manager.
pub type Ref = usize;
/// Terminal vertex with value 0.
pub const FALSE: Ref = 0;
/// Terminal vertex with value 1.
pub const TRUE: Ref = 1;

const TERMINAL_VAR: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
struct Vertex {
    var: u32,
    low: Ref,
    high: Ref,
}

/// A manager owning every vertex; all functions share one unique table.
#[derive(Debug, Clone)]
pub struct Bdd {
    vertices: Vec<Vertex>,
    unique: HashMap<(u32, Ref, Ref), Ref>,
    ite_cache: HashMap<(Ref, Ref, Ref), Ref>,
    max_nodes: usize,
    overflow: bool,
}

impl Bdd {
    pub fn new(max_nodes: usize) -> Self {
        let terminal = Vertex { var: TERMINAL_VAR, low: FALSE, high: FALSE };
        Self {
            vertices: vec![terminal, terminal],
            unique: HashMap::new(),
            ite_cache: HashMap::new(),
            max_nodes,
            overflow: false,
        }
    }

    /// `true` once more than `max_nodes` vertices were requested; results
    /// computed after that point are meaningless.
    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Total vertices allocated, terminals included.
    pub fn allocated(&self) -> usize {
        self.vertices.len()
    }

    fn var_of(&self, f: Ref) -> u32 {
        self.vertices[f].var
    }

    /// Creates (or finds) the vertex `(var, low, high)`; the two reduction
    /// rules of Bryant's `Reduce` hold by construction.
    pub fn mk(&mut self, var: u32, low: Ref, high: Ref) -> Ref {
        if low == high {
            return low;
        }
        if let Some(&r) = self.unique.get(&(var, low, high)) {
            return r;
        }
        if self.vertices.len() >= self.max_nodes {
            self.overflow = true;
            return FALSE;
        }
        self.vertices.push(Vertex { var, low, high });
        let r = self.vertices.len() - 1;
        self.unique.insert((var, low, high), r);
        r
    }

    /// The function `x_i`.
    pub fn var(&mut self, i: u32) -> Ref {
        self.mk(i, FALSE, TRUE)
    }

    /// The function `¬x_i`.
    pub fn nvar(&mut self, i: u32) -> Ref {
        self.mk(i, TRUE, FALSE)
    }

    fn cofactors(&self, f: Ref, var: u32) -> (Ref, Ref) {
        let v = self.vertices[f];
        if v.var == var { (v.low, v.high) } else { (f, f) }
    }

    /// `ITE(f, g, h) = f·g + ¬f·h` (Sect. 4.5), the single primitive from
    /// which `Apply` for every Boolean operator is derived.
    pub fn ite(&mut self, f: Ref, g: Ref, h: Ref) -> Ref {
        if f == TRUE {
            return g;
        }
        if f == FALSE {
            return h;
        }
        if g == h {
            return g;
        }
        if g == TRUE && h == FALSE {
            return f;
        }
        if self.overflow {
            return FALSE;
        }
        if let Some(&r) = self.ite_cache.get(&(f, g, h)) {
            return r;
        }
        let top = self.var_of(f).min(self.var_of(g)).min(self.var_of(h));
        let (f0, f1) = self.cofactors(f, top);
        let (g0, g1) = self.cofactors(g, top);
        let (h0, h1) = self.cofactors(h, top);
        let low = self.ite(f0, g0, h0);
        let high = self.ite(f1, g1, h1);
        let r = self.mk(top, low, high);
        self.ite_cache.insert((f, g, h), r);
        r
    }

    pub fn not(&mut self, f: Ref) -> Ref {
        self.ite(f, FALSE, TRUE)
    }

    pub fn and(&mut self, f: Ref, g: Ref) -> Ref {
        self.ite(f, g, FALSE)
    }

    pub fn or(&mut self, f: Ref, g: Ref) -> Ref {
        self.ite(f, TRUE, g)
    }

    /// `f|x_i = b` (Sect. 4.4).
    pub fn restrict(&mut self, f: Ref, var: u32, value: bool) -> Ref {
        let mut memo = HashMap::new();
        self.restrict_rec(f, var, value, &mut memo)
    }

    fn restrict_rec(&mut self, f: Ref, var: u32, value: bool, memo: &mut HashMap<Ref, Ref>) -> Ref {
        let v = self.vertices[f];
        if v.var > var {
            return f; // terminal, or the function does not depend on `var`
        }
        if v.var == var {
            return if value { v.high } else { v.low };
        }
        if let Some(&r) = memo.get(&f) {
            return r;
        }
        let low = self.restrict_rec(v.low, var, value, memo);
        let high = self.restrict_rec(v.high, var, value, memo);
        let r = self.mk(v.var, low, high);
        memo.insert(f, r);
        r
    }

    /// `∃ {x_i : quantified[i]} . f`, i.e. `f|x=0 + f|x=1` for each variable.
    pub fn exists(&mut self, f: Ref, quantified: &[bool]) -> Ref {
        let mut memo = HashMap::new();
        self.exists_rec(f, quantified, &mut memo)
    }

    fn exists_rec(&mut self, f: Ref, q: &[bool], memo: &mut HashMap<Ref, Ref>) -> Ref {
        if f <= TRUE {
            return f;
        }
        if let Some(&r) = memo.get(&f) {
            return r;
        }
        let v = self.vertices[f];
        let low = self.exists_rec(v.low, q, memo);
        let high = self.exists_rec(v.high, q, memo);
        let r = if q.get(v.var as usize).copied().unwrap_or(false) {
            self.or(low, high)
        } else {
            self.mk(v.var, low, high)
        };
        memo.insert(f, r);
        r
    }

    /// Number of vertices reachable from `f`, terminals excluded.
    pub fn node_count(&self, f: Ref) -> usize {
        let mut seen = BTreeSet::new();
        let mut stack = vec![f];
        while let Some(n) = stack.pop() {
            if n > TRUE && seen.insert(n) {
                stack.push(self.vertices[n].low);
                stack.push(self.vertices[n].high);
            }
        }
        seen.len()
    }

    /// Evaluates `f` on a full assignment.
    pub fn eval(&self, f: Ref, assignment: &[bool]) -> bool {
        let mut n = f;
        while n > TRUE {
            let v = self.vertices[n];
            n = if assignment[v.var as usize] { v.high } else { v.low };
        }
        n == TRUE
    }

    /// `Satisfy-one` (Fig. 9): some satisfying assignment over `vars`
    /// variables, `None` if `f` is unsatisfiable. Linear in `vars` because a
    /// reduced graph never needs to backtrack past a failed terminal
    /// (Lemma 3).
    pub fn satisfy_one(&self, f: Ref, vars: usize) -> Option<Vec<bool>> {
        if f == FALSE {
            return None;
        }
        let mut x = vec![false; vars];
        let mut n = f;
        while n > TRUE {
            let v = self.vertices[n];
            if v.low != FALSE {
                x[v.var as usize] = false;
                n = v.low;
            } else {
                x[v.var as usize] = true;
                n = v.high;
            }
        }
        Some(x)
    }

    /// `Satisfy-count` (Sect. 4.6): `|S_f|` over `vars` variables, in time
    /// linear in the graph, or `None` on `u128` overflow.
    pub fn satisfy_count(&self, f: Ref, vars: usize) -> Option<u128> {
        let mut memo: HashMap<Ref, Option<u128>> = HashMap::new();
        let alpha = self.alpha(f, vars as u32, &mut memo)?;
        let skipped = self.level(f, vars as u32);
        alpha.checked_mul(1u128.checked_shl(skipped)?)
    }

    fn level(&self, f: Ref, vars: u32) -> u32 {
        if f <= TRUE { vars } else { self.vertices[f].var }
    }

    /// `α_v`: models of `v` over the variables `index(v)..n`.
    fn alpha(&self, f: Ref, vars: u32, memo: &mut HashMap<Ref, Option<u128>>) -> Option<u128> {
        match f {
            FALSE => return Some(0),
            TRUE => return Some(1),
            _ => {}
        }
        if let Some(&c) = memo.get(&f) {
            return c;
        }
        let v = self.vertices[f];
        let mut total = Some(0u128);
        for child in [v.low, v.high] {
            let gap = self.level(child, vars) - v.var - 1;
            let term = self
                .alpha(child, vars, memo)
                .and_then(|a| 1u128.checked_shl(gap).and_then(|s| a.checked_mul(s)));
            total = total.and_then(|t| term.and_then(|x| t.checked_add(x)));
        }
        memo.insert(f, total);
        total
    }

    /// `Satisfy-all` (Fig. 10): every model over `vars` variables, or `None`
    /// if there are more than `cap`.
    pub fn satisfy_all(&self, f: Ref, vars: usize, cap: usize) -> Option<BTreeSet<Vec<bool>>> {
        let mut out = BTreeSet::new();
        let mut x = vec![false; vars];
        if self.sat_all_rec(0, f, &mut x, &mut out, cap) { Some(out) } else { None }
    }

    fn sat_all_rec(
        &self,
        i: usize,
        f: Ref,
        x: &mut Vec<bool>,
        out: &mut BTreeSet<Vec<bool>>,
        cap: usize,
    ) -> bool {
        if f == FALSE {
            return true;
        }
        if i == x.len() {
            out.insert(x.clone());
            return out.len() <= cap;
        }
        let (low, high) = if f > TRUE && self.vertices[f].var as usize == i {
            (self.vertices[f].low, self.vertices[f].high)
        } else {
            (f, f) // independent of x_i
        };
        x[i] = false;
        if !self.sat_all_rec(i + 1, low, x, out, cap) {
            return false;
        }
        x[i] = true;
        self.sat_all_rec(i + 1, high, x, out, cap)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BddConfig {
    /// Maximum number of BDD vertices ever allocated.
    pub max_nodes: usize,
}

impl Default for BddConfig {
    fn default() -> Self {
        Self { max_nodes: 5_000_000 }
    }
}

/// The reachable markings of a 1-safe net as a BDD over the places.
#[derive(Debug, Clone)]
pub struct BddStateSpace {
    bdd: Bdd,
    root: Ref,
    places: usize,
    /// Fixed-point iterations (sweeps over all transitions) until stable.
    pub iterations: usize,
}

struct SafeTransition {
    pre: Vec<u32>,
    post: Vec<u32>,
}

fn not_applicable(reason: &'static str) -> ReductionError {
    ReductionError::NotApplicable { name: "bdd", reason }
}

/// Symbolic reachability set of a 1-safe net (`least fixed point` of the
/// per-transition images, chaining order).
pub fn generate(net: &PetriNet, config: &BddConfig) -> Result<BddStateSpace, ReductionError> {
    if net.is_extended() {
        return Err(not_applicable("nets with inhibitor arcs or immediate-transition precedence are not supported"));
    }
    let places = net.place_count();
    let mut m0 = Vec::with_capacity(places);
    for token in &net.initial_marking().tokens {
        match token {
            TokenValue::Finite(0) => m0.push(false),
            TokenValue::Finite(1) => m0.push(true),
            TokenValue::Finite(_) => return Err(not_applicable("initial marking has a place with more than one token")),
            _ => return Err(not_applicable("initial marking is not a finite 0/1 vector")),
        }
    }
    let mut transitions = Vec::new();
    for t in net.transitions() {
        let side = |arcs: &[(racon_core::PlaceId, u32)]| -> Result<Vec<u32>, ReductionError> {
            let mut places: Vec<u32> = Vec::new();
            for &(p, w) in arcs {
                if w != 1 || places.contains(&p.0) {
                    return Err(not_applicable("arc weights other than 1 are not 1-safe"));
                }
                places.push(p.0);
            }
            Ok(places)
        };
        let pre = side(net.preset(t.id))?;
        let post = side(net.postset(t.id))?;
        transitions.push(SafeTransition { pre, post });
    }

    let mut bdd = Bdd::new(config.max_nodes);
    let limit = || ReductionError::LimitExceeded { limit: config.max_nodes };

    // Initial marking as a cube, built bottom-up (last variable first).
    let mut s = TRUE;
    for (p, &marked) in m0.iter().enumerate().rev() {
        s = if marked { bdd.mk(p as u32, FALSE, s) } else { bdd.mk(p as u32, s, FALSE) };
    }

    let mut iterations = 0;
    loop {
        iterations += 1;
        let before = s;
        for t in &transitions {
            let img = image(&mut bdd, s, t, places)?;
            if bdd.overflowed() {
                return Err(limit());
            }
            s = bdd.or(s, img);
            if bdd.overflowed() {
                return Err(limit());
            }
        }
        if s == before {
            break;
        }
    }
    Ok(BddStateSpace { bdd, root: s, places, iterations })
}

/// `Img_t(S)`; fails if firing `t` could put a second token in a place.
fn image(bdd: &mut Bdd, s: Ref, t: &SafeTransition, places: usize) -> Result<Ref, ReductionError> {
    let mut enabled = s;
    for &p in &t.pre {
        let x = bdd.var(p);
        enabled = bdd.and(enabled, x);
    }
    if enabled == FALSE || bdd.overflowed() {
        return Ok(FALSE);
    }
    // Output places that are not consumed must be empty (1-safeness).
    for &p in t.post.iter().filter(|p| !t.pre.contains(p)) {
        let clash = bdd.restrict(enabled, p, true);
        if clash != FALSE {
            return Err(not_applicable("the net is not 1-safe: a reachable firing would put two tokens in a place"));
        }
    }
    let mut touched = vec![false; places];
    for &p in t.pre.iter().chain(&t.post) {
        touched[p as usize] = true;
    }
    let mut img = bdd.exists(enabled, &touched);
    for &p in &t.pre {
        if !t.post.contains(&p) {
            let n = bdd.nvar(p);
            img = bdd.and(img, n);
        }
    }
    for &p in &t.post {
        let x = bdd.var(p);
        img = bdd.and(img, x);
    }
    Ok(img)
}

impl BddStateSpace {
    /// Vertices of the final BDD (terminals excluded).
    pub fn node_count(&self) -> usize {
        self.bdd.node_count(self.root)
    }

    pub fn allocated_nodes(&self) -> usize {
        self.bdd.allocated()
    }

    /// `|S|`, or `None` on `u128` overflow.
    pub fn state_count(&self) -> Option<u128> {
        self.bdd.satisfy_count(self.root, self.places)
    }

    /// Is `marking` (one value per place, each 0 or 1) reachable?
    pub fn contains(&self, marking: &[u64]) -> bool {
        if marking.len() != self.places || marking.iter().any(|&v| v > 1) {
            return false;
        }
        let assignment: Vec<bool> = marking.iter().map(|&v| v == 1).collect();
        self.bdd.eval(self.root, &assignment)
    }

    /// All reachable markings, or `None` if there are more than `cap`.
    pub fn states(&self, cap: usize) -> Option<BTreeSet<Vec<u64>>> {
        let all = self.bdd.satisfy_all(self.root, self.places, cap)?;
        Some(all.into_iter().map(|m| m.into_iter().map(u64::from).collect()).collect())
    }

    /// One reachable marking (`Satisfy-one`); `None` is impossible for a
    /// generated space because the initial marking always belongs to it.
    pub fn any_state(&self) -> Option<Vec<u64>> {
        self.bdd.satisfy_one(self.root, self.places).map(|m| m.into_iter().map(u64::from).collect())
    }

    /// Exact maximum (0 or 1) of each place over all reachable markings.
    pub fn place_bounds(&mut self) -> Vec<u64> {
        (0..self.places as u32)
            .map(|p| {
                let x = self.bdd.var(p);
                u64::from(self.bdd.and(self.root, x) != FALSE)
            })
            .collect()
    }
}
