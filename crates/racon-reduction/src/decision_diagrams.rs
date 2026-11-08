//! Symbolic state-space generation with multi-valued decision diagrams and
//! the **saturation** strategy of G. Ciardo, G. Lüttgen and R. Siminiceanu,
//! *Saturation: An Efficient Iteration Strategy for Symbolic State-Space
//! Generation*, TACAS 2001 (`3-540-45319-9_23.pdf`, Fig. 2 and Theorem 1).
//! The canonical-form discipline (a *unique table* so that equal sub-functions
//! share one node) is that of R. Bryant, *Graph-Based Algorithms for Boolean
//! Function Manipulation*, IEEE TC 35(8), 1986 (`Bryant_1986.pdf`).
//!
//! Encoding (paper Section 2): one level per place (`K = |P|`, place `0` on
//! level `K`, the last place on level 1), quasi-reduced ordered MDD, terminals
//! `0` (empty set) and `1` (the single empty substate). Each transition is an
//! event with the *product form* `N_e = N_e^K × … × N_e^1`: place `p` is a
//! pure function `i ↦ i − Pre + Post` enabled when `i ≥ Pre`, and the
//! identity for places the transition does not touch. `First(e)` / `Last(e)`
//! are the highest / lowest level the event depends on; events with
//! `First = Last` are local.
//!
//! Differences from the paper, forced by arbitrary (not pre-bounded) nets:
//! node arcs are sparse (`value → child`, absent means `0`), so no per-place
//! bound `n_k` is required; the state space must nevertheless be **finite**.
//! For an unbounded net generation never saturates and stops with
//! `LimitExceeded` when `max_nodes` is reached. The result represents the set
//! of reachable markings only, not the transition graph: it answers
//! reachability of a marking, state count and exact place bounds; it is not
//! meant for liveness, paths or CTL.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use racon_core::marking::TokenValue;
use racon_core::net::PetriNet;

use crate::error::ReductionError;

const EMPTY: usize = 0;
const ONE: usize = 1;

#[derive(Debug, Clone, Copy)]
pub struct DdConfig {
    /// Maximum number of MDD nodes ever allocated.
    pub max_nodes: usize,
}

impl Default for DdConfig {
    fn default() -> Self {
        Self { max_nodes: 5_000_000 }
    }
}

#[derive(Debug, Clone)]
struct Node {
    level: usize,
    arcs: BTreeMap<u64, usize>,
}

/// One transition as an event of the product form.
#[derive(Debug, Clone)]
struct Event {
    first: usize,
    last: usize,
    /// level → (pre, post) for the places the event depends on.
    effect: HashMap<usize, (u64, u64)>,
}

impl Event {
    /// `N_e^k(i)`: the successor local state, if enabled.
    fn local(&self, level: usize, i: u64) -> Option<u64> {
        match self.effect.get(&level) {
            None => Some(i),
            Some(&(pre, post)) => (i >= pre).then(|| i - pre + post),
        }
    }
}

struct Mdd {
    nodes: Vec<Node>,
    unique: Vec<HashMap<Vec<(u64, usize)>, usize>>,
    union_cache: HashMap<(usize, usize, usize), usize>,
    fire_cache: HashMap<(usize, usize, usize), usize>,
    events: Vec<Event>,
    by_first: Vec<Vec<usize>>,
    max_nodes: usize,
    overflow: bool,
}

impl Mdd {
    fn new_node(&mut self, level: usize) -> usize {
        if self.nodes.len() >= self.max_nodes {
            self.overflow = true;
        }
        self.nodes.push(Node { level, arcs: BTreeMap::new() });
        self.nodes.len() - 1
    }

    /// Hash-conses a saturated node; an empty node becomes the terminal 0.
    fn check(&mut self, level: usize, p: usize) -> usize {
        if self.nodes[p].arcs.is_empty() {
            return EMPTY;
        }
        let key: Vec<(u64, usize)> = self.nodes[p].arcs.iter().map(|(&i, &q)| (i, q)).collect();
        match self.unique[level].get(&key) {
            Some(&q) => q,
            None => {
                self.unique[level].insert(key, p);
                p
            }
        }
    }

    fn union(&mut self, level: usize, p: usize, q: usize) -> usize {
        if p == ONE || q == ONE {
            return ONE;
        }
        if p == EMPTY || p == q {
            return q;
        }
        if q == EMPTY {
            return p;
        }
        let key = (level, p.min(q), p.max(q));
        if let Some(&s) = self.union_cache.get(&key) {
            return s;
        }
        let s = self.new_node(level);
        let (pa, qa) = (self.nodes[p].arcs.clone(), self.nodes[q].arcs.clone());
        let values: BTreeSet<u64> = pa.keys().chain(qa.keys()).copied().collect();
        for i in values {
            if self.overflow {
                return EMPTY;
            }
            let u = self.union(
                level - 1,
                pa.get(&i).copied().unwrap_or(EMPTY),
                qa.get(&i).copied().unwrap_or(EMPTY),
            );
            if u != EMPTY {
                self.nodes[s].arcs.insert(i, u);
            }
        }
        let s = self.check(level, s);
        self.union_cache.insert(key, s);
        s
    }

    /// Local states of node `p` (level `k`) that enable event `e` locally.
    fn locals(&self, e: usize, k: usize, p: usize) -> Vec<u64> {
        self.nodes[p]
            .arcs
            .iter()
            .filter(|(_, &q)| q != EMPTY)
            .map(|(&i, _)| i)
            .filter(|&i| self.events[e].local(k, i).is_some())
            .collect()
    }

    /// `RecFire(e, l, q)`: node encoding `N^{≤l}(N_e(B(q)))` (Fig. 2).
    fn rec_fire(&mut self, e: usize, l: usize, q: usize) -> usize {
        if l < self.events[e].last {
            return q;
        }
        if let Some(&s) = self.fire_cache.get(&(l, q, e)) {
            return s;
        }
        let s = self.new_node(l);
        let mut changed = false;
        for i in self.locals(e, l, q) {
            if self.overflow {
                return EMPTY;
            }
            let child = self.nodes[q].arcs[&i];
            let f = self.rec_fire(e, l - 1, child);
            if f == EMPTY {
                continue;
            }
            let j = self.events[e].local(l, i).expect("enabled");
            let old = self.nodes[s].arcs.get(&j).copied().unwrap_or(EMPTY);
            let u = self.union(l - 1, f, old);
            if u != old {
                self.nodes[s].arcs.insert(j, u);
                changed = true;
            }
        }
        if changed {
            self.saturate(l, s);
        }
        let s = self.check(l, s);
        self.fire_cache.insert((l, q, e), s);
        s
    }

    /// `Saturate(k, p)`: fire every event with `First(e) ≤ k` until a fixed
    /// point; children are already saturated (Theorem 1).
    fn saturate(&mut self, k: usize, p: usize) {
        loop {
            let mut changed = false;
            for e in self.by_first[k].clone() {
                let mut pending: Vec<u64> = self.locals(e, k, p);
                while let Some(i) = pending.pop() {
                    if self.overflow {
                        return;
                    }
                    let child = self.nodes[p].arcs.get(&i).copied().unwrap_or(EMPTY);
                    let f = self.rec_fire(e, k - 1, child);
                    if f == EMPTY {
                        continue;
                    }
                    let j = self.events[e].local(k, i).expect("enabled");
                    let old = self.nodes[p].arcs.get(&j).copied().unwrap_or(EMPTY);
                    let u = self.union(k - 1, f, old);
                    if u != old {
                        self.nodes[p].arcs.insert(j, u);
                        changed = true;
                        if self.events[e].local(k, j).is_some() && !pending.contains(&j) {
                            pending.push(j);
                        }
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }
}

/// The reachable markings of a net as a saturated MDD.
#[derive(Debug, Clone)]
pub struct SymbolicStateSpace {
    nodes: Vec<Node>,
    root: usize,
    places: usize,
    pub allocated_nodes: usize,
    pub unique_nodes: usize,
}

/// Generates the reachable state space (`Generate` of Fig. 2).
pub fn generate(net: &PetriNet, config: &DdConfig) -> Result<SymbolicStateSpace, ReductionError> {
    const NAME: &str = "decision_diagrams";
    if net.is_extended() {
        return Err(ReductionError::NotApplicable {
            name: NAME,
            reason: "nets with inhibitor arcs or immediate-transition precedence are not supported",
        });
    }
    let places = net.place_count();
    let mut m0 = Vec::with_capacity(places);
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
    let level_of = |p: usize| places - p;
    let mut events = Vec::new();
    for t in net.transitions() {
        let mut effect: HashMap<usize, (u64, u64)> = HashMap::new();
        for &(p, w) in net.preset(t.id) {
            effect.entry(level_of(p.0 as usize)).or_insert((0, 0)).0 += u64::from(w);
        }
        for &(p, w) in net.postset(t.id) {
            effect.entry(level_of(p.0 as usize)).or_insert((0, 0)).1 += u64::from(w);
        }
        let first = effect.keys().copied().max().unwrap_or(1);
        let last = effect.keys().copied().min().unwrap_or(1);
        events.push(Event { first, last, effect });
    }
    let mut by_first = vec![Vec::new(); places + 1];
    for (e, ev) in events.iter().enumerate() {
        by_first[ev.first].push(e);
    }
    let mut mdd = Mdd {
        nodes: vec![Node { level: 0, arcs: BTreeMap::new() }, Node { level: 0, arcs: BTreeMap::new() }],
        unique: vec![HashMap::new(); places + 1],
        union_cache: HashMap::new(),
        fire_cache: HashMap::new(),
        events,
        by_first,
        max_nodes: config.max_nodes,
        overflow: false,
    };
    // Generate(s): build the initial state bottom-up, saturating each node.
    let mut p = ONE;
    for k in 1..=places {
        let r = mdd.new_node(k);
        let local = m0[places - k];
        mdd.nodes[r].arcs.insert(local, p);
        mdd.saturate(k, r);
        p = mdd.check(k, r);
        if mdd.overflow {
            return Err(ReductionError::LimitExceeded { limit: config.max_nodes });
        }
    }
    if mdd.overflow {
        return Err(ReductionError::LimitExceeded { limit: config.max_nodes });
    }
    let unique_nodes = mdd.unique.iter().map(HashMap::len).sum();
    Ok(SymbolicStateSpace { allocated_nodes: mdd.nodes.len(), unique_nodes, nodes: mdd.nodes, root: p, places })
}

impl SymbolicStateSpace {
    /// Number of nodes reachable from the root (the size of the final MDD).
    pub fn node_count(&self) -> usize {
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(n) = stack.pop() {
            if n > ONE && seen.insert(n) {
                stack.extend(self.nodes[n].arcs.values().copied());
            }
        }
        seen.len()
    }

    /// `|S|`, or `None` on `u128` overflow.
    pub fn state_count(&self) -> Option<u128> {
        let mut memo: HashMap<usize, Option<u128>> = HashMap::new();
        self.count(self.root, &mut memo)
    }

    fn count(&self, n: usize, memo: &mut HashMap<usize, Option<u128>>) -> Option<u128> {
        match n {
            EMPTY => return Some(0),
            ONE => return Some(1),
            _ => {}
        }
        if let Some(&c) = memo.get(&n) {
            return c;
        }
        let mut total: Option<u128> = Some(0);
        for &child in self.nodes[n].arcs.values() {
            total = total.and_then(|t| self.count(child, memo).and_then(|c| t.checked_add(c)));
        }
        memo.insert(n, total);
        total
    }

    /// Is `marking` (one value per place) a reachable marking?
    pub fn contains(&self, marking: &[u64]) -> bool {
        if marking.len() != self.places {
            return false;
        }
        let mut n = self.root;
        for &v in marking {
            match self.nodes[n].arcs.get(&v) {
                Some(&child) => n = child,
                None => return false,
            }
        }
        n == ONE
    }

    /// All reachable markings, or `None` if there are more than `cap`.
    pub fn states(&self, cap: usize) -> Option<BTreeSet<Vec<u64>>> {
        let mut out = BTreeSet::new();
        let mut prefix = Vec::with_capacity(self.places);
        if self.walk(self.root, &mut prefix, &mut out, cap) { Some(out) } else { None }
    }

    fn walk(&self, n: usize, prefix: &mut Vec<u64>, out: &mut BTreeSet<Vec<u64>>, cap: usize) -> bool {
        if n == ONE {
            out.insert(prefix.clone());
            return out.len() <= cap;
        }
        for (&v, &child) in &self.nodes[n].arcs {
            prefix.push(v);
            let ok = self.walk(child, prefix, out, cap);
            prefix.pop();
            if !ok {
                return false;
            }
        }
        true
    }

    /// Exact maximum number of tokens of each place over all reachable
    /// markings (every arc of the final MDD lies on a path to terminal 1).
    pub fn place_bounds(&self) -> Vec<u64> {
        let mut bounds = vec![0u64; self.places];
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(n) = stack.pop() {
            if n > ONE && seen.insert(n) {
                let place = self.places - self.nodes[n].level;
                for (&v, &child) in &self.nodes[n].arcs {
                    bounds[place] = bounds[place].max(v);
                    stack.push(child);
                }
            }
        }
        bounds
    }
}
