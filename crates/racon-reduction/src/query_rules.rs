//! Query-dependent structural reduction rules A–I
//! (Bønneland, Jensen, Larsen, Muñiz, Srba — *Stubborn versus structural
//! reductions for Petri nets*, JLAMP 102, 2019, §4, Figs. 3–10, Alg. 2).
//!
//! The rules rewrite a weighted net with inhibitor arcs so that reachability of
//! a cardinality formula `φ` (a [`ReachGoal`]) is preserved (Def. 3, Lemmas
//! 6–14). Rules A–F and H also preserve the existence of a reachable deadlock
//! (Thm 15); G and I do not, and are disabled for [`QueryKind::Deadlock`].
//!
//! The rules depend on `places(φ)`: the reduced net answers *that* query only.
//! It is not a general-purpose simplification and must not be used to draw the
//! reachability graph. Nets with immediate transitions are rejected (outside
//! the scope of the source).
//!
//! Deviation from the paper: in rule C the paper states condition C3(b) only
//! for `t ∈ •p0`; here it is checked for **every** transition (a transition
//! that feeds `p1` but not `p0` would break `M(p0) ≥ k·M(p1)`), which is
//! strictly safer.
//!
//! Each application that would leave the net without places, transitions or
//! arcs (which [`PetriNet`] cannot represent) is skipped.

use std::collections::{BTreeMap, HashMap, HashSet};

use num_bigint::BigUint;
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TransitionId};

use crate::error::ReductionError;
use crate::goal_search::{LinearConstraint, ReachGoal};

/// What the reduced net must preserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// Reachability of a cardinality formula over `protected` places.
    Reachability,
    /// Existence of a reachable deadlock (rules A–F, H only).
    Deadlock,
}

/// Number of applications per rule (`"A"` … `"I"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleCount {
    pub rule: &'static str,
    pub count: usize,
}

/// A net reduced for one query, with the maps needed to translate it.
#[derive(Debug, Clone)]
pub struct QueryReduction {
    pub net: PetriNet,
    /// Original place index → place in `net`, `None` if removed.
    pub place_map: Vec<Option<PlaceId>>,
    pub transition_map: Vec<Option<TransitionId>>,
    pub applied: Vec<RuleCount>,
}

impl QueryReduction {
    /// Translate a formula over the original places to the reduced net.
    /// Returns `None` if it mentions a removed place (cannot happen for places
    /// that were passed as protected).
    pub fn translate_goal(&self, goal: &ReachGoal) -> Option<ReachGoal> {
        let mut disjuncts = Vec::new();
        for conjunction in &goal.disjuncts {
            let mut out = Vec::new();
            for constraint in conjunction {
                let mut terms = Vec::new();
                for &(place, coeff) in &constraint.terms {
                    terms.push((self.place_map.get(place.0 as usize).copied().flatten()?, coeff));
                }
                out.push(LinearConstraint {
                    terms,
                    op: constraint.op,
                    constant: constraint.constant,
                });
            }
            disjuncts.push(out);
        }
        Some(ReachGoal { disjuncts })
    }

    pub fn total_applications(&self) -> usize {
        self.applied.iter().map(|r| r.count).sum()
    }
}

/// Places mentioned by a goal (`places(φ)`).
pub fn goal_places(goal: &ReachGoal) -> Vec<PlaceId> {
    let mut places: Vec<PlaceId> = goal
        .disjuncts
        .iter()
        .flatten()
        .flat_map(|c| c.terms.iter().map(|&(p, _)| p))
        .collect();
    places.sort_by_key(|p| p.0);
    places.dedup();
    places
}

type Arcs = BTreeMap<usize, u64>;

#[derive(Clone)]
struct Work {
    p_alive: Vec<bool>,
    t_alive: Vec<bool>,
    pre: Vec<Arcs>,
    post: Vec<Arcs>,
    inh: Vec<Arcs>,
    m0: Vec<u128>,
}

struct Index {
    prod: Vec<Vec<usize>>,
    cons: Vec<Vec<usize>>,
    inhibited: Vec<Vec<usize>>,
}

fn mul(a: u64, b: u64) -> Option<u64> {
    a.checked_mul(b).filter(|&v| v <= u64::from(u32::MAX))
}

fn add(a: u64, b: u64) -> Option<u64> {
    a.checked_add(b).filter(|&v| v <= u64::from(u32::MAX))
}

impl Work {
    fn from_net(net: &PetriNet) -> Result<Self, ReductionError> {
        if net.has_immediate_transitions() {
            return Err(ReductionError::NotApplicable {
                name: "STRUCTURAL_QUERY_RULES",
                reason: "rules A-I are proved for weighted nets with inhibitor arcs; immediate transitions are not supported",
            });
        }
        let places = net.place_count();
        let transitions = net.transition_count();
        let mut m0 = Vec::with_capacity(places);
        for token in &net.initial_marking().tokens {
            let value = token
                .as_biguint()
                .and_then(|v| u128::try_from(v).ok())
                .ok_or(ReductionError::NotApplicable {
                    name: "STRUCTURAL_QUERY_RULES",
                    reason: "initial marking exceeds the supported token range",
                })?;
            m0.push(value);
        }
        let mut work = Work {
            p_alive: vec![true; places],
            t_alive: vec![true; transitions],
            pre: vec![Arcs::new(); transitions],
            post: vec![Arcs::new(); transitions],
            inh: vec![Arcs::new(); transitions],
            m0,
        };
        for t in 0..transitions {
            let id = TransitionId(t as u32);
            for &(p, w) in net.preset(id) {
                work.pre[t].insert(p.0 as usize, u64::from(w));
            }
            for &(p, w) in net.postset(id) {
                work.post[t].insert(p.0 as usize, u64::from(w));
            }
            for &(p, k) in net.inhibitors(id) {
                work.inh[t].insert(p.0 as usize, k);
            }
        }
        Ok(work)
    }

    fn index(&self) -> Index {
        let places = self.p_alive.len();
        let mut ix = Index {
            prod: vec![Vec::new(); places],
            cons: vec![Vec::new(); places],
            inhibited: vec![Vec::new(); places],
        };
        for t in 0..self.t_alive.len() {
            if !self.t_alive[t] {
                continue;
            }
            for (&p, &w) in &self.post[t] {
                if w > 0 {
                    ix.prod[p].push(t);
                }
            }
            for (&p, &w) in &self.pre[t] {
                if w > 0 {
                    ix.cons[p].push(t);
                }
            }
            for &p in self.inh[t].keys() {
                ix.inhibited[p].push(t);
            }
        }
        ix
    }

    fn kill_place(&mut self, p: usize) {
        self.p_alive[p] = false;
        for t in 0..self.t_alive.len() {
            self.pre[t].remove(&p);
            self.post[t].remove(&p);
            self.inh[t].remove(&p);
        }
    }

    fn kill_transition(&mut self, t: usize) {
        self.t_alive[t] = false;
        self.pre[t].clear();
        self.post[t].clear();
        self.inh[t].clear();
    }

    fn buildable(&self) -> bool {
        let arcs: usize = (0..self.t_alive.len())
            .filter(|&t| self.t_alive[t])
            .map(|t| self.pre[t].len() + self.post[t].len())
            .sum();
        self.p_alive.iter().any(|&a| a) && self.t_alive.iter().any(|&a| a) && arcs > 0
    }
}

fn get(arcs: &Arcs, place: usize) -> u64 {
    arcs.get(&place).copied().unwrap_or(0)
}

// ---- Rule A: sequential transition removal -------------------------------

fn rule_a(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    'outer: for t0 in 0..w.t_alive.len() {
        if !w.t_alive[t0] || w.pre[t0].len() != 1 || !w.inh[t0].is_empty() {
            continue;
        }
        let (&p0, &w0) = w.pre[t0].iter().next().unwrap();
        if w0 != 1
            || ix.cons[p0] != [t0]
            || !ix.inhibited[p0].is_empty()
            || q.contains(&p0)
            || w.post[t0].contains_key(&p0)
        {
            continue;
        }
        let outs: Vec<(usize, u64)> = w.post[t0].iter().map(|(&p, &x)| (p, x)).collect();
        if outs
            .iter()
            .any(|(p, _)| !ix.inhibited[*p].is_empty() || q.contains(p))
        {
            continue;
        }
        let mut updates: Vec<(usize, usize, u64)> = Vec::new();
        for &t in &ix.prod[p0] {
            let wt = w.post[t][&p0];
            for &(p, x) in &outs {
                let Some(extra) = mul(wt, x) else {
                    continue 'outer;
                };
                updates.push((t, p, extra));
            }
        }
        let mut m_updates = Vec::new();
        for &(p, x) in &outs {
            let Some(extra) = w.m0[p0].checked_mul(u128::from(x)) else {
                continue 'outer;
            };
            m_updates.push((p, extra));
        }
        for &(t, p, extra) in &updates {
            let Some(value) = add(get(&w.post[t], p), extra) else {
                continue 'outer;
            };
            if t != t0 {
                w.post[t].insert(p, value);
            }
        }
        for (p, extra) in m_updates {
            w.m0[p] += extra;
        }
        w.kill_place(p0);
        w.kill_transition(t0);
        return true;
    }
    false
}

// ---- Rule B: sequential place removal -------------------------------------

fn rule_b(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    'outer: for p0 in 0..w.p_alive.len() {
        if !w.p_alive[p0]
            || q.contains(&p0)
            || !ix.inhibited[p0].is_empty()
            || ix.prod[p0].len() != 1
            || ix.cons[p0].len() != 1
        {
            continue;
        }
        let (t0, t1) = (ix.prod[p0][0], ix.cons[p0][0]);
        if t0 == t1 || w.pre[t1].len() != 1 || !w.inh[t0].is_empty() || !w.inh[t1].is_empty() {
            continue;
        }
        let w1 = w.pre[t1][&p0];
        let kw = w.post[t0][&p0];
        if w1 == 0 || kw % w1 != 0 || kw / w1 < 1 {
            continue;
        }
        let k = kw / w1;
        let outs: Vec<(usize, u64)> = w.post[t1].iter().map(|(&p, &x)| (p, x)).collect();
        if outs
            .iter()
            .any(|(p, _)| !ix.inhibited[*p].is_empty() || q.contains(p) || *p == p0)
        {
            continue;
        }
        let tokens = w.m0[p0] / u128::from(w1);
        let mut post_updates = Vec::new();
        let mut m_updates = Vec::new();
        for &(p, x) in &outs {
            let (Some(extra), Some(m_extra)) = (mul(k, x), tokens.checked_mul(u128::from(x)))
            else {
                continue 'outer;
            };
            let Some(value) = add(get(&w.post[t0], p), extra) else {
                continue 'outer;
            };
            post_updates.push((p, value));
            m_updates.push((p, m_extra));
        }
        for (p, value) in post_updates {
            w.post[t0].insert(p, value);
        }
        for (p, extra) in m_updates {
            w.m0[p] += extra;
        }
        w.kill_place(p0);
        w.kill_transition(t1);
        return true;
    }
    false
}

// ---- Rule C: parallel place removal ---------------------------------------

fn rule_c(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    for p0 in 0..w.p_alive.len() {
        if !w.p_alive[p0] || q.contains(&p0) || !ix.inhibited[p0].is_empty() {
            continue;
        }
        let Some(&first) = ix.cons[p0].first() else {
            continue; // never consumed: rule F covers it
        };
        let candidates: Vec<usize> = w.pre[first].keys().copied().filter(|&p| p != p0).collect();
        'cand: for p1 in candidates {
            if !w.p_alive[p1] || !ix.inhibited[p1].is_empty() {
                continue;
            }
            // Smallest k satisfying (c); larger k only hardens (a) and (b).
            let mut k: u64 = 1;
            for &t in &ix.cons[p0] {
                let (a, b) = (w.pre[t][&p0], get(&w.pre[t], p1));
                if b == 0 {
                    continue 'cand;
                }
                k = k.max(a.div_ceil(b));
            }
            if w.m0[p0] < w.m0[p1].saturating_mul(u128::from(k)) {
                continue;
            }
            for &t in &ix.prod[p1] {
                let need = u128::from(w.post[t][&p1]) * u128::from(k);
                if u128::from(get(&w.post[t], p0)) < need {
                    continue 'cand;
                }
            }
            w.kill_place(p0);
            return true;
        }
    }
    false
}

// ---- Rule D: parallel transition removal ----------------------------------

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn rule_d(w: &mut Work) -> bool {
    type Signature = (Vec<(usize, u64)>, Vec<(usize, u64)>);
    let mut seen: HashMap<Signature, (usize, u64)> = HashMap::new();
    for t in 0..w.t_alive.len() {
        if !w.t_alive[t] || !w.inh[t].is_empty() {
            continue;
        }
        let g = w
            .pre[t]
            .values()
            .chain(w.post[t].values())
            .fold(0, |acc, &x| gcd(acc, x));
        let normalise = |arcs: &Arcs| -> Vec<(usize, u64)> {
            arcs.iter().map(|(&p, &x)| (p, if g == 0 { x } else { x / g })).collect()
        };
        let signature = (normalise(&w.pre[t]), normalise(&w.post[t]));
        match seen.get(&signature).copied() {
            None => {
                seen.insert(signature, (t, g));
            }
            Some((other, other_g)) => {
                // t = (g / other_g)·other or the reverse; drop the larger.
                let drop = if g == 0 || other_g == 0 || g % other_g == 0 {
                    t
                } else if other_g % g == 0 {
                    other
                } else {
                    continue;
                };
                w.kill_transition(drop);
                return true;
            }
        }
    }
    false
}

// ---- Rule E: dead transition removal --------------------------------------

fn rule_e(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    for p0 in 0..w.p_alive.len() {
        if !w.p_alive[p0] {
            continue;
        }
        for &t0 in &ix.cons[p0] {
            if w.m0[p0] >= u128::from(w.pre[t0][&p0]) {
                continue;
            }
            let ok = ix.prod[p0].iter().all(|&t| {
                get(&w.post[t], p0) <= get(&w.pre[t], p0)
                    || w.m0[p0] < u128::from(get(&w.pre[t], p0))
            });
            if !ok {
                continue;
            }
            let only = ix.cons[p0] == [t0];
            let removable = only && ix.inhibited[p0].is_empty() && !q.contains(&p0);
            w.kill_transition(t0);
            if removable {
                w.kill_place(p0);
            }
            return true;
        }
    }
    false
}

// ---- Rule F: redundant place removal --------------------------------------

fn rule_f(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    for p0 in 0..w.p_alive.len() {
        if !w.p_alive[p0] || q.contains(&p0) || !ix.inhibited[p0].is_empty() {
            continue;
        }
        let ok = ix.cons[p0].iter().all(|&t| {
            let need = w.pre[t][&p0];
            get(&w.post[t], p0) >= need && w.m0[p0] >= u128::from(need)
        });
        if ok {
            w.kill_place(p0);
            return true;
        }
    }
    false
}

// ---- Rule G: redundant transition removal (not deadlock-preserving) -------

fn rule_g(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    for t0 in 0..w.t_alive.len() {
        if !w.t_alive[t0] || !w.inh[t0].is_empty() {
            continue;
        }
        if w.pre[t0].keys().any(|&p| !ix.inhibited[p].is_empty()) {
            continue;
        }
        if w.post[t0].keys().any(|p| !w.pre[t0].contains_key(p)) {
            continue;
        }
        let ok = w.pre[t0].iter().all(|(&p, &pre)| {
            let post = get(&w.post[t0], p);
            pre == post || (pre > post && !q.contains(&p))
        });
        if ok {
            w.kill_transition(t0);
            return true;
        }
    }
    false
}

// ---- Rule H: simple cycle removal -----------------------------------------

fn rule_h(w: &mut Work, q: &HashSet<usize>) -> bool {
    let ix = w.index();
    let single = |arcs: &Arcs| -> Option<usize> {
        (arcs.len() == 1 && arcs.values().all(|&x| x == 1)).then(|| *arcs.keys().next().unwrap())
    };
    for t0 in 0..w.t_alive.len() {
        if !w.t_alive[t0] || !w.inh[t0].is_empty() {
            continue;
        }
        let (Some(p0), Some(p1)) = (single(&w.pre[t0]), single(&w.post[t0])) else {
            continue;
        };
        if p0 == p1
            || q.contains(&p0)
            || q.contains(&p1)
            || !ix.inhibited[p0].is_empty()
            || !ix.inhibited[p1].is_empty()
        {
            continue;
        }
        let Some(&t1) = ix.cons[p1].iter().find(|&&t| {
            t != t0
                && w.inh[t].is_empty()
                && single(&w.pre[t]) == Some(p1)
                && single(&w.post[t]) == Some(p0)
        }) else {
            continue;
        };
        // Merge p0 into p1.
        let mut changes: Vec<(usize, bool, u64)> = Vec::new();
        for &t in &ix.prod[p0] {
            changes.push((t, true, w.post[t][&p0]));
        }
        for &t in &ix.cons[p0] {
            changes.push((t, false, w.pre[t][&p0]));
        }
        let mut staged = Vec::new();
        for (t, is_post, weight) in changes {
            let current = if is_post {
                get(&w.post[t], p1)
            } else {
                get(&w.pre[t], p1)
            };
            let Some(value) = add(current, weight) else {
                return false;
            };
            staged.push((t, is_post, value));
        }
        for (t, is_post, value) in staged {
            if is_post {
                w.post[t].insert(p1, value);
            } else {
                w.pre[t].insert(p1, value);
            }
        }
        w.post[t1].insert(p1, 1);
        w.pre[t1].insert(p1, 1);
        w.m0[p1] += w.m0[p0];
        w.kill_transition(t0);
        w.kill_place(p0);
        return true;
    }
    false
}

// ---- Rule I: removal of irrelevant places and transitions -----------------

fn rule_i(w: &mut Work, q: &HashSet<usize>) -> bool {
    let places = w.p_alive.len();
    let mut increases: Vec<Vec<usize>> = vec![Vec::new(); places]; // +p
    let mut decreases: Vec<Vec<usize>> = vec![Vec::new(); places]; // p-
    for t in 0..w.t_alive.len() {
        if !w.t_alive[t] {
            continue;
        }
        let mut touched: HashSet<usize> = w.pre[t].keys().copied().collect();
        touched.extend(w.post[t].keys().copied());
        for p in touched {
            let (pre, post) = (get(&w.pre[t], p), get(&w.post[t], p));
            if post > pre {
                increases[p].push(t);
            } else if pre > post {
                decreases[p].push(t);
            }
        }
    }
    let mut x: HashSet<usize> = HashSet::new();
    let mut unprocessed: Vec<usize> = Vec::new();
    for &p in q {
        unprocessed.extend(increases[p].iter().chain(decreases[p].iter()).copied());
    }
    while let Some(t) = unprocessed.pop() {
        if !x.insert(t) {
            continue;
        }
        for &p in w.pre[t].keys() {
            unprocessed.extend(increases[p].iter().filter(|u| !x.contains(u)));
        }
        for &p in w.inh[t].keys() {
            unprocessed.extend(decreases[p].iter().filter(|u| !x.contains(u)));
        }
    }
    let mut keep_places: HashSet<usize> = q.clone();
    for &t in &x {
        keep_places.extend(w.pre[t].keys().copied());
        keep_places.extend(w.inh[t].keys().copied());
    }
    let drop_places: Vec<usize> = (0..places)
        .filter(|&p| w.p_alive[p] && !keep_places.contains(&p))
        .collect();
    let drop_transitions: Vec<usize> = (0..w.t_alive.len())
        .filter(|&t| w.t_alive[t] && !x.contains(&t))
        .collect();
    if drop_places.is_empty() && drop_transitions.is_empty() {
        return false;
    }
    for t in drop_transitions {
        w.kill_transition(t);
    }
    for p in drop_places {
        w.kill_place(p);
    }
    true
}

fn into_net(original: &PetriNet, w: &Work) -> Result<QueryReduction, ReductionError> {
    let unsupported = ReductionError::NotApplicable {
        name: "STRUCTURAL_QUERY_RULES",
        reason: "reduced net exceeds representable weights",
    };
    let mut builder = PetriNetBuilder::default();
    let mut place_map = vec![None; w.p_alive.len()];
    let mut transition_map = vec![None; w.t_alive.len()];
    let mut tokens = Vec::new();
    for (p, place) in original.places().iter().enumerate() {
        if w.p_alive[p] {
            place_map[p] = Some(builder.add_place(place.name.clone(), place.label.clone()));
            tokens.push(TokenValue::exact(BigUint::from(w.m0[p])));
        }
    }
    for (t, transition) in original.transitions().iter().enumerate() {
        if w.t_alive[t] {
            transition_map[t] =
                Some(builder.add_transition(transition.name.clone(), transition.label.clone()));
        }
    }
    let weight = |x: u64| u32::try_from(x).map_err(|_| unsupported.clone());
    for t in 0..w.t_alive.len() {
        let Some(nt) = transition_map[t] else {
            continue;
        };
        for (&p, &x) in &w.pre[t] {
            builder.add_arc(
                ArcEndpoint::Place(place_map[p].expect("live arc on dead place")),
                ArcEndpoint::Transition(nt),
                weight(x)?,
            );
        }
        for (&p, &x) in &w.post[t] {
            builder.add_arc(
                ArcEndpoint::Transition(nt),
                ArcEndpoint::Place(place_map[p].expect("live arc on dead place")),
                weight(x)?,
            );
        }
        for (&p, &k) in &w.inh[t] {
            builder.add_inhibitor_arc(place_map[p].expect("live arc on dead place"), nt, k);
        }
    }
    let net = builder
        .build(tokens)
        .map_err(|_| ReductionError::NotApplicable {
            name: "STRUCTURAL_QUERY_RULES",
            reason: "reduced net could not be built",
        })?;
    Ok(QueryReduction {
        net,
        place_map,
        transition_map,
        applied: Vec::new(),
    })
}

/// Apply rules A–I to a fixed point for the query that mentions `protected`
/// places (`places(φ)`; ignored for [`QueryKind::Deadlock`], which needs none).
pub fn reduce_for_query(
    net: &PetriNet,
    protected: &[PlaceId],
    kind: QueryKind,
) -> Result<QueryReduction, ReductionError> {
    let mut work = Work::from_net(net)?;
    let q: HashSet<usize> = match kind {
        QueryKind::Reachability => protected.iter().map(|p| p.0 as usize).collect(),
        QueryKind::Deadlock => HashSet::new(),
    };
    let reachability = kind == QueryKind::Reachability;
    let names = ["A", "B", "C", "D", "E", "F", "G", "H", "I"];
    let mut counts = [0usize; 9];

    loop {
        let mut progressed = false;
        for rule in 0..9 {
            if !reachability && (rule == 6 || rule == 8) {
                continue; // G and I do not preserve deadlocks (Thm 15)
            }
            let snapshot = work.clone();
            let applied = match rule {
                0 => rule_a(&mut work, &q),
                1 => rule_b(&mut work, &q),
                2 => rule_c(&mut work, &q),
                3 => rule_d(&mut work),
                4 => rule_e(&mut work, &q),
                5 => rule_f(&mut work, &q),
                6 => rule_g(&mut work, &q),
                7 => rule_h(&mut work, &q),
                _ => rule_i(&mut work, &q),
            };
            if !applied {
                work = snapshot;
                continue;
            }
            if work.buildable() {
                counts[rule] += 1;
                progressed = true;
                break;
            }
            work = snapshot;
        }
        if !progressed {
            break;
        }
    }

    let mut reduction = into_net(net, &work)?;
    reduction.applied = names
        .iter()
        .zip(counts)
        .filter(|&(_, count)| count > 0)
        .map(|(&rule, count)| RuleCount { rule, count })
        .collect();
    Ok(reduction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_search::GoalOp;
    use racon_core::marking::Marking;
    use std::collections::{HashSet as Set, VecDeque};

    type Spec<'a> = (&'a [(usize, u32)], &'a [(usize, u32)], &'a [(usize, u64)]);

    fn make(places: usize, transitions: &[Spec], m0: &[u64]) -> PetriNet {
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..places).map(|i| b.add_place(format!("p{i}"), None)).collect();
        for (i, (pre, post, inh)) in transitions.iter().enumerate() {
            let t = b.add_transition(format!("t{i}"), None);
            for &(p, x) in *pre {
                b.add_arc(ArcEndpoint::Place(ps[p]), ArcEndpoint::Transition(t), x);
            }
            for &(p, x) in *post {
                b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(ps[p]), x);
            }
            for &(p, k) in *inh {
                b.add_inhibitor_arc(ps[p], t, k);
            }
        }
        b.build(m0.iter().map(|&v| TokenValue::Finite(v)).collect())
            .unwrap()
    }

    fn applied(reduction: &QueryReduction, rule: &str) -> usize {
        reduction
            .applied
            .iter()
            .find(|r| r.rule == rule)
            .map_or(0, |r| r.count)
    }

    /// Full BFS; `None` when the budget is exceeded. Returns (goal reachable,
    /// deadlock reachable).
    fn explore(net: &PetriNet, goal: Option<&ReachGoal>, cap: usize) -> Option<(bool, bool)> {
        let mut seen: Set<Marking> = Set::from([net.initial_marking().clone()]);
        let mut queue = VecDeque::from([net.initial_marking().clone()]);
        let (mut reached, mut deadlock) = (false, false);
        while let Some(m) = queue.pop_front() {
            reached |= goal.is_some_and(|g| g.is_satisfied(&m));
            let mut any = false;
            for t in net.transitions() {
                if net.is_enabled(t.id, &m) {
                    any = true;
                    let next = net.fire(t.id, &m);
                    if seen.insert(next.clone()) {
                        if seen.len() > cap {
                            return None;
                        }
                        queue.push_back(next);
                    }
                }
            }
            deadlock |= !any;
        }
        Some((reached, deadlock))
    }

    #[test]
    fn rule_a_merges_a_sequential_transition() {
        // p0 -t0-> p1 (t0 feeds nothing else); t1 consumes p1.
        let net = make(
            3,
            &[(&[(0, 1)], &[(1, 1)], &[]), (&[(1, 1)], &[(2, 1)], &[]), (&[(2, 1)], &[(0, 1)], &[])],
            &[2, 0, 0],
        );
        let r = reduce_for_query(&net, &[PlaceId(2)], QueryKind::Reachability).unwrap();
        assert!(applied(&r, "A") >= 1, "{:?}", r.applied);
        assert!(r.net.place_count() < 3);
    }

    #[test]
    fn rule_f_removes_a_never_decreasing_place() {
        let net = make(
            3,
            &[(&[(0, 1), (2, 1)], &[(1, 1), (2, 1)], &[]), (&[(1, 1)], &[(0, 1)], &[])],
            &[1, 0, 1],
        );
        let r = reduce_for_query(&net, &[PlaceId(0), PlaceId(1)], QueryKind::Reachability).unwrap();
        assert_eq!(applied(&r, "F"), 1, "{:?}", r.applied);
        assert!(r.place_map[2].is_none());
    }

    #[test]
    fn rule_d_removes_a_proportional_duplicate() {
        let net = make(
            2,
            &[(&[(0, 1)], &[(1, 1)], &[]), (&[(0, 2)], &[(1, 2)], &[]), (&[(1, 1)], &[(0, 1)], &[])],
            &[2, 0],
        );
        let r = reduce_for_query(&net, &[PlaceId(0), PlaceId(1)], QueryKind::Reachability).unwrap();
        assert_eq!(applied(&r, "D"), 1, "{:?}", r.applied);
        assert_eq!(r.net.transition_count(), 2);
    }

    #[test]
    fn rule_e_removes_a_dead_transition() {
        let net = make(
            3,
            &[(&[(0, 1)], &[(2, 1)], &[]), (&[(1, 1)], &[(2, 1)], &[]), (&[(2, 1)], &[(0, 1)], &[])],
            &[1, 0, 0],
        );
        let r = reduce_for_query(&net, &[PlaceId(0), PlaceId(2)], QueryKind::Reachability).unwrap();
        assert!(applied(&r, "E") >= 1, "{:?}", r.applied);
        assert!(r.transition_map[1].is_none());
    }

    #[test]
    fn rule_h_collapses_a_two_place_cycle() {
        // p0 <-> p1 via t0/t1, both feeding/consumed by the rest of the net.
        let net = make(
            3,
            &[
                (&[(0, 1)], &[(1, 1)], &[]),
                (&[(1, 1)], &[(0, 1)], &[]),
                (&[(1, 1)], &[(2, 1)], &[]),
                (&[(2, 1)], &[(0, 1)], &[]),
            ],
            &[1, 0, 0],
        );
        // Other rules may consume the shape first, so exercise H in isolation.
        let mut work = Work::from_net(&net).unwrap();
        let q: HashSet<usize> = HashSet::from([2]);
        assert!(rule_h(&mut work, &q));
        assert!(!work.p_alive[0] && work.p_alive[1]);
        assert_eq!(work.m0[1], 1);
        assert!(work.buildable());
    }

    #[test]
    fn rule_i_drops_irrelevant_parts() {
        // Two independent subnets; the query only concerns the first.
        let net = make(
            4,
            &[(&[(0, 1)], &[(1, 1)], &[]), (&[(1, 1)], &[(0, 1)], &[]), (&[(2, 1)], &[(3, 1)], &[]), (&[(3, 1)], &[(2, 1)], &[])],
            &[1, 0, 1, 0],
        );
        let r = reduce_for_query(&net, &[PlaceId(1)], QueryKind::Reachability).unwrap();
        assert!(r.place_map[2].is_none() && r.place_map[3].is_none());
    }

    #[test]
    fn extended_with_immediate_is_rejected() {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.set_transition_kind(t, racon_core::net::TransitionKind::Immediate);
        let net = b.build(vec![TokenValue::Finite(1)]).unwrap();
        let err = reduce_for_query(&net, &[], QueryKind::Reachability).unwrap_err();
        assert_eq!(err.code(), "REDUCTION_NOT_APPLICABLE");
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
        let places = 2 + rng.next(4) as usize;
        let transitions = 2 + rng.next(5) as usize;
        let mut b = PetriNetBuilder::default();
        let ps: Vec<_> = (0..places).map(|i| b.add_place(format!("p{i}"), None)).collect();
        for i in 0..transitions {
            let t = b.add_transition(format!("t{i}"), None);
            let mut arcs = 0;
            for &p in &ps {
                // sparse nets so that A/B/H style shapes appear
                if rng.next(4) == 0 {
                    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1 + rng.next(2) as u32 * rng.next(2) as u32);
                    arcs += 1;
                }
                if rng.next(4) == 0 {
                    b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1 + rng.next(2) as u32 * rng.next(2) as u32);
                    arcs += 1;
                }
                if rng.next(12) == 0 {
                    b.add_inhibitor_arc(p, t, 1 + rng.next(3));
                }
            }
            if arcs == 0 {
                b.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(t), 1);
            }
        }
        let tokens = (0..places).map(|_| TokenValue::Finite(rng.next(4))).collect();
        b.build(tokens).unwrap()
    }

    fn random_goal(rng: &mut Lcg, places: usize) -> ReachGoal {
        let mut terms = Vec::new();
        for p in 0..places {
            if rng.next(2) == 0 {
                terms.push((PlaceId(p as u32), 1 + rng.next(2) as i64));
            }
        }
        if terms.is_empty() {
            terms.push((PlaceId(0), 1));
        }
        let op = [GoalOp::Ge, GoalOp::Le, GoalOp::Eq][rng.next(3) as usize];
        ReachGoal::single(LinearConstraint { terms, op, constant: rng.next(5) as i64 })
    }

    #[test]
    fn rules_preserve_reachability_and_deadlock_on_random_nets() {
        let mut rng = Lcg(std::env::var("RACON_FUZZ_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(0xC0FFEE));
        let (mut compared, mut reduced, mut hit) = (0usize, 0usize, HashSet::new());
        let iterations: usize = std::env::var("RACON_FUZZ_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(1200);
        for _ in 0..iterations {
            let net = random_net(&mut rng);
            let goal = random_goal(&mut rng, net.place_count());

            // Cardinality reachability.
            let protected = goal_places(&goal);
            let r = reduce_for_query(&net, &protected, QueryKind::Reachability).unwrap();
            let translated = r.translate_goal(&goal).expect("protected places survive");
            if let (Some(a), Some(b)) = (
                explore(&net, Some(&goal), 4000),
                explore(&r.net, Some(&translated), 4000),
            ) {
                assert_eq!(a.0, b.0, "reachability changed by {:?}\nnet: {net:?}\ngoal: {goal:?}", r.applied);
                compared += 1;
                if r.total_applications() > 0 {
                    reduced += 1;
                }
                for rule in &r.applied {
                    hit.insert(rule.rule);
                }
            }

            // Deadlock (A-F, H only).
            let d = reduce_for_query(&net, &[], QueryKind::Deadlock).unwrap();
            assert!(d.applied.iter().all(|r| r.rule != "G" && r.rule != "I"));
            if let (Some(a), Some(b)) = (explore(&net, None, 4000), explore(&d.net, None, 4000)) {
                assert_eq!(a.1, b.1, "deadlock changed by {:?}\nnet: {net:?}", d.applied);
            }
        }
        assert!(compared > iterations / 3, "too few bounded samples: {compared}");
        assert!(reduced > iterations / 12, "rules rarely fired: {reduced}");
        eprintln!("rules exercised by the random test: {hit:?}");
    }
}
