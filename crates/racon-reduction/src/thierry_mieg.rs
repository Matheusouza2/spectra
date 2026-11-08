//! Structural reduction rules of Y. Thierry-Mieg, *Structural Reductions
//! Revisited*, Petri Nets 2020, LNCS 12152, pp. 303-323
//! (`References/Bibliografia/978-3-030-51831-8_Chapter_15.pdf`, R8).
//!
//! Rules implemented (numbering and applicability follow Section 5):
//!
//! | rule | name                         | deadlock | safety |
//! |------|------------------------------|----------|--------|
//! | 1    | equal transitions modulo k   | yes      | yes    |
//! | 2    | dominated transition         | yes      | yes    |
//! | 3    | redundant composition        | yes      | yes    |
//! | 4    | neutral transition           | no       | yes    |
//! | 5    | sink transition              | no       | yes    |
//! | 6    | source transition            | yes      | no     |
//! | 7    | equal places modulo k        | yes      | yes    |
//! | 8    | sink place                   | yes      | yes    |
//! | 9    | constant place               | yes      | yes    |
//! | 10   | maximal unmarked siphon      | yes      | yes    |
//! | 11   | bounded marking place        | yes      | yes    |
//! | 14   | pre agglomeration            | yes      | yes    |
//! | 15   | post agglomeration           | yes      | yes    |
//! | 16   | free agglomeration           | no       | yes    |
//! | 17   | controlling marked place     | yes      | yes    |
//!
//! Not implemented: rules 12, 13 (implicit / future-equivalent places), the
//! graph-based rules 18-20 and the SMT-backed rules 21-22.
//!
//! Preservation: with `ReductionGoal::Deadlock` the net has a reachable
//! deadlock iff the reduced net does. With `ReductionGoal::Safety { support }`
//! the set of reachable markings projected on `support` is unchanged; support
//! places are never removed, merged or rewritten (rules 9 and 10 only discard
//! dead transitions around them). The reduced net is a *different* net: it
//! must not be used for reachability graphs, liveness, boundedness of removed
//! places, reversibility or CTL on the original net.

use std::collections::{BTreeMap, BTreeSet};

use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TransitionId};

use crate::error::ReductionError;

/// Property family the reduction must preserve (paper Section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReductionGoal {
    /// Existence of a reachable deadlock; the support is empty.
    Deadlock,
    /// Safety invariants whose atomic propositions read only `support`.
    Safety { support: Vec<PlaceId> },
}

#[derive(Debug, Clone, Copy)]
pub struct ThierryMiegConfig {
    /// Agglomeration is refused when `|•p| · |p•|` exceeds this (paper: 32).
    pub max_agglomerate_product: usize,
    /// Rule 3 is cubic in the number of transitions; it is skipped above this.
    pub rule3_max_transitions: usize,
    pub max_rounds: usize,
}

impl Default for ThierryMiegConfig {
    fn default() -> Self {
        Self { max_agglomerate_product: 32, rule3_max_transitions: 200, max_rounds: 100_000 }
    }
}

/// One agglomeration performed (Definition 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agglomeration {
    pub rule: u8,
    pub place: String,
    pub new_transitions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ThierryMiegResult {
    /// `None` when the residual model cannot be a [`PetriNet`] (no place, no
    /// transition or no arc left): this is the "empty net" outcome.
    pub reduced_net: Option<PetriNet>,
    pub remaining_places: Vec<String>,
    pub remaining_transitions: Vec<String>,
    /// Initial marking of `remaining_places` after the reduction (rule 17).
    pub initial_marking: Vec<u64>,
    /// Rule 6: a source transition exists, so no deadlock is reachable.
    pub deadlock_free: bool,
    /// Number of applications per rule id (`1..=17`).
    pub applications: BTreeMap<u8, usize>,
    pub agglomerations: Vec<Agglomeration>,
    pub rounds: usize,
}

impl ThierryMiegResult {
    pub fn total_applications(&self) -> usize {
        self.applications.values().sum()
    }
}

type Row = BTreeMap<usize, u64>;

struct Model {
    place_names: Vec<String>,
    place_labels: Vec<Option<String>>,
    trans_names: Vec<String>,
    trans_labels: Vec<Option<String>>,
    place_alive: Vec<bool>,
    trans_alive: Vec<bool>,
    pre: Vec<Row>,
    post: Vec<Row>,
    m0: Vec<u64>,
    support: BTreeSet<usize>,
    safety: bool,
}

fn scale_eq(a: &Row, b: &Row, k: u64) -> bool {
    a.len() == b.len() && a.iter().all(|(key, v)| b.get(key).is_some_and(|w| *v == k * *w))
}

/// Smallest-structure candidate `k ≥ 1` such that `a = k·b`, from the first
/// non-zero entry of `b`; `Some(1)` when every row is empty.
fn candidate_k(rows: &[(&Row, &Row)], scalars: &[(u64, u64)]) -> Option<u64> {
    for (a, b) in rows {
        if let Some((key, w)) = b.iter().next() {
            let v = a.get(key)?;
            return (*v % *w == 0 && *v / *w >= 1).then_some(*v / *w);
        }
    }
    for (a, b) in scalars {
        if *b != 0 {
            return (*a % *b == 0 && *a / *b >= 1).then_some(*a / *b);
        }
    }
    Some(1)
}

fn add_scaled(dst: &mut Row, src: &Row, k: u64) {
    for (p, w) in src {
        *dst.entry(*p).or_insert(0) += k * *w;
    }
}

impl Model {
    fn from_net(net: &PetriNet, goal: &ReductionGoal) -> Result<Self, ReductionError> {
        if net.is_extended() {
            return Err(ReductionError::NotApplicable {
                name: "thierry_mieg_structural",
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
                        name: "thierry_mieg_structural",
                        reason: "initial marking is not a finite u64 vector",
                    })
                }
            }
        }
        let (safety, support) = match goal {
            ReductionGoal::Deadlock => (false, BTreeSet::new()),
            ReductionGoal::Safety { support } => {
                if support.iter().any(|p| p.0 as usize >= places) {
                    return Err(ReductionError::NotApplicable {
                        name: "thierry_mieg_structural",
                        reason: "support place out of range",
                    });
                }
                (true, support.iter().map(|p| p.0 as usize).collect())
            }
        };
        let mut pre = Vec::new();
        let mut post = Vec::new();
        for t in net.transitions() {
            let mut a = Row::new();
            let mut b = Row::new();
            for &(p, w) in net.preset(t.id) {
                *a.entry(p.0 as usize).or_insert(0) += u64::from(w);
            }
            for &(p, w) in net.postset(t.id) {
                *b.entry(p.0 as usize).or_insert(0) += u64::from(w);
            }
            pre.push(a);
            post.push(b);
        }
        Ok(Self {
            place_names: net.places().iter().map(|p| p.name.clone()).collect(),
            place_labels: net.places().iter().map(|p| p.label.clone()).collect(),
            trans_names: net.transitions().iter().map(|t| t.name.clone()).collect(),
            trans_labels: net.transitions().iter().map(|t| t.label.clone()).collect(),
            place_alive: vec![true; places],
            trans_alive: vec![true; net.transitions().len()],
            pre,
            post,
            m0,
            support,
            safety,
        })
    }

    fn alive_t(&self) -> Vec<usize> {
        (0..self.pre.len()).filter(|&t| self.trans_alive[t]).collect()
    }
    fn alive_p(&self) -> Vec<usize> {
        (0..self.m0.len()).filter(|&p| self.place_alive[p]).collect()
    }
    fn in_support(&self, p: usize) -> bool {
        self.support.contains(&p)
    }
    fn effect(&self, t: usize) -> BTreeMap<usize, i128> {
        let mut e: BTreeMap<usize, i128> = BTreeMap::new();
        for (p, w) in &self.post[t] {
            *e.entry(*p).or_insert(0) += i128::from(*w);
        }
        for (p, w) in &self.pre[t] {
            *e.entry(*p).or_insert(0) -= i128::from(*w);
        }
        e.retain(|_, v| *v != 0);
        e
    }
    /// `t ∈ St`: no effect on any support place.
    fn stuttering(&self, t: usize) -> bool {
        let e = self.effect(t);
        self.support.iter().all(|p| !e.contains_key(p))
    }
    fn feeders(&self, p: usize) -> Vec<usize> {
        self.alive_t().into_iter().filter(|&t| self.post[t].contains_key(&p)).collect()
    }
    fn consumers(&self, p: usize) -> Vec<usize> {
        self.alive_t().into_iter().filter(|&t| self.pre[t].contains_key(&p)).collect()
    }
    fn kill_transition(&mut self, t: usize) {
        self.trans_alive[t] = false;
    }
    fn kill_place(&mut self, p: usize) {
        self.place_alive[p] = false;
        for t in 0..self.pre.len() {
            self.pre[t].remove(&p);
            self.post[t].remove(&p);
        }
    }
    fn place_rows(&self) -> BTreeMap<usize, (Row, Row)> {
        let mut rows: BTreeMap<usize, (Row, Row)> =
            self.alive_p().into_iter().map(|p| (p, (Row::new(), Row::new()))).collect();
        for t in self.alive_t() {
            for (p, w) in &self.pre[t] {
                if let Some(r) = rows.get_mut(p) {
                    r.0.insert(t, *w);
                }
            }
            for (p, w) in &self.post[t] {
                if let Some(r) = rows.get_mut(p) {
                    r.1.insert(t, *w);
                }
            }
        }
        rows
    }
}

// ── elementary transition rules ──────────────────────────────────────────────

/// Rule 1.
fn rule1(m: &mut Model) -> usize {
    let mut n = 0;
    for t in m.alive_t() {
        for u in m.alive_t() {
            if t == u || !m.trans_alive[t] {
                continue;
            }
            let Some(k) =
                candidate_k(&[(&m.pre[t], &m.pre[u]), (&m.post[t], &m.post[u])], &[])
            else {
                continue;
            };
            if scale_eq(&m.pre[t], &m.pre[u], k)
                && scale_eq(&m.post[t], &m.post[u], k)
                && (k > 1 || u < t)
            {
                m.kill_transition(t);
                n += 1;
            }
        }
    }
    n
}

/// Rule 2.
fn rule2(m: &mut Model) -> usize {
    let mut n = 0;
    for t in m.alive_t() {
        for u in m.alive_t() {
            if t == u || !m.trans_alive[t] || !m.trans_alive[u] {
                continue;
            }
            let dominated = m.effect(t) == m.effect(u)
                && m.pre[u].iter().all(|(p, w)| m.pre[t].get(p).is_some_and(|v| v >= w));
            if dominated && (m.pre[t] != m.pre[u] || u < t) {
                m.kill_transition(t);
                n += 1;
            }
        }
    }
    n
}

/// Rule 3.
fn rule3(m: &mut Model, cfg: &ThierryMiegConfig) -> usize {
    let alive = m.alive_t();
    if alive.len() > cfg.rule3_max_transitions {
        return 0;
    }
    let mut n = 0;
    for &t in &alive {
        'search: for &t1 in &alive {
            for &t2 in &alive {
                if !m.trans_alive[t] || t1 == t || t2 == t || !m.trans_alive[t1] || !m.trans_alive[t2] {
                    continue;
                }
                let covers = |a: &Row, b: &Row| b.iter().all(|(p, w)| a.get(p).is_some_and(|v| v >= w));
                if !covers(&m.pre[t], &m.pre[t1]) || !covers(&m.post[t1], &m.pre[t2]) {
                    continue;
                }
                let mut sum = m.effect(t1);
                for (p, v) in m.effect(t2) {
                    *sum.entry(p).or_insert(0) += v;
                }
                sum.retain(|_, v| *v != 0);
                if sum == m.effect(t) {
                    m.kill_transition(t);
                    n += 1;
                    break 'search;
                }
            }
        }
    }
    n
}

/// Rule 4 (safety only).
fn rule4(m: &mut Model) -> usize {
    if !m.safety {
        return 0;
    }
    let mut n = 0;
    for t in m.alive_t() {
        if m.pre[t] == m.post[t] {
            m.kill_transition(t);
            n += 1;
        }
    }
    n
}

/// Rule 5 (safety only).
fn rule5(m: &mut Model) -> usize {
    if !m.safety {
        return 0;
    }
    let mut n = 0;
    for t in m.alive_t() {
        if m.post[t].is_empty() && m.stuttering(t) {
            m.kill_transition(t);
            n += 1;
        }
    }
    n
}

/// Rule 6 (deadlock only): returns whether a source transition exists.
fn rule6(m: &mut Model) -> bool {
    if m.safety {
        return false;
    }
    let Some(source) = m.alive_t().into_iter().find(|&t| m.pre[t].is_empty()) else {
        return false;
    };
    for t in m.alive_t() {
        if t != source {
            m.kill_transition(t);
        }
    }
    for p in m.alive_p() {
        m.kill_place(p);
    }
    true
}

// ── elementary place rules ───────────────────────────────────────────────────

/// Rule 7.
fn rule7(m: &mut Model) -> usize {
    let mut n = 0;
    let rows = m.place_rows();
    let ids: Vec<usize> = rows.keys().copied().collect();
    for &p in &ids {
        for &q in &ids {
            if p == q || !m.place_alive[p] || !m.place_alive[q] || m.in_support(p) || m.in_support(q) {
                continue;
            }
            let (pp, pq) = (&rows[&p], &rows[&q]);
            let Some(k) = candidate_k(&[(&pp.0, &pq.0), (&pp.1, &pq.1)], &[(m.m0[p], m.m0[q])])
            else {
                continue;
            };
            if scale_eq(&pp.0, &pq.0, k)
                && scale_eq(&pp.1, &pq.1, k)
                && m.m0[p] == k * m.m0[q]
                && (k > 1 || q < p)
            {
                m.kill_place(p);
                n += 1;
            }
        }
    }
    n
}

/// Rule 8.
fn rule8(m: &mut Model) -> usize {
    let mut n = 0;
    for p in m.alive_p() {
        if !m.in_support(p) && m.consumers(p).is_empty() {
            m.kill_place(p);
            n += 1;
        }
    }
    n
}

/// Rule 9.
fn rule9(m: &mut Model) -> usize {
    let mut n = 0;
    for p in m.alive_p() {
        if m.in_support(p) || !m.place_alive[p] {
            continue;
        }
        let constant = m
            .alive_t()
            .into_iter()
            .all(|t| m.pre[t].get(&p).copied().unwrap_or(0) == m.post[t].get(&p).copied().unwrap_or(0));
        if !constant {
            continue;
        }
        for t in m.alive_t() {
            if m.pre[t].get(&p).copied().unwrap_or(0) > m.m0[p] {
                m.kill_transition(t);
            }
        }
        m.kill_place(p);
        n += 1;
    }
    n
}

/// Rule 10.
fn rule10(m: &mut Model) -> usize {
    let mut siphon: BTreeSet<usize> = m.alive_p().into_iter().filter(|&p| m.m0[p] == 0).collect();
    let mut ts: BTreeSet<usize> = m.alive_t().into_iter().collect();
    loop {
        let mut changed = false;
        for t in ts.clone() {
            if !m.post[t].keys().any(|p| siphon.contains(p)) {
                ts.remove(&t);
                changed = true;
            } else if !m.pre[t].keys().any(|p| siphon.contains(p)) {
                ts.remove(&t);
                for p in m.post[t].keys() {
                    siphon.remove(p);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if siphon.is_empty() {
        return 0;
    }
    let mut n = 0;
    for t in m.alive_t() {
        if m.pre[t].keys().any(|p| siphon.contains(p)) {
            m.kill_transition(t);
            n += 1;
        }
    }
    for p in siphon {
        if !m.in_support(p) {
            m.kill_place(p);
            n += 1;
        }
    }
    n
}

/// Rule 11.
fn rule11(m: &mut Model) -> usize {
    let mut n = 0;
    for p in m.alive_p() {
        let no_true_input = m.alive_t().into_iter().all(|t| {
            m.post[t].get(&p).copied().unwrap_or(0) <= m.pre[t].get(&p).copied().unwrap_or(0)
        });
        if !no_true_input {
            continue;
        }
        for t in m.alive_t() {
            if m.pre[t].get(&p).copied().unwrap_or(0) > m.m0[p] {
                m.kill_transition(t);
                n += 1;
            }
        }
    }
    n
}

/// Rule 17.
fn rule17(m: &mut Model) -> usize {
    let mut n = 0;
    for p in m.alive_p() {
        let consumers = m.consumers(p);
        if consumers.len() != 1 {
            continue;
        }
        let t = consumers[0];
        let only_p = m.pre[t].len() == 1;
        let w = m.pre[t].get(&p).copied().unwrap_or(0);
        if !only_p || w == 0 || m.post[t].contains_key(&p) || m.m0[p] == 0 || m.m0[p] % w != 0 {
            continue;
        }
        if !m.stuttering(t) {
            continue;
        }
        let k = m.m0[p] / w;
        m.m0[p] = 0;
        for (q, v) in m.post[t].clone() {
            m.m0[q] += k * v;
        }
        n += 1;
    }
    n
}

// ── agglomeration rules ──────────────────────────────────────────────────────

fn agglomerate(
    m: &mut Model,
    p: usize,
    feeders: &[usize],
    consumers: &[usize],
    rule: u8,
    out: &mut Vec<Agglomeration>,
) -> bool {
    let mut created: Vec<(String, Row, Row, Option<String>)> = Vec::new();
    for &h in feeders {
        for &f in consumers {
            let produced = m.post[h][&p];
            let consumed = m.pre[f][&p];
            if produced % consumed != 0 {
                return false;
            }
            let k = produced / consumed;
            let mut pre = m.pre[h].clone();
            add_scaled(&mut pre, &m.pre[f], k);
            let mut post = m.post[h].clone();
            add_scaled(&mut post, &m.post[f], k);
            pre.remove(&p);
            post.remove(&p);
            if pre.values().chain(post.values()).any(|w| *w > u64::from(u32::MAX)) {
                return false;
            }
            created.push((
                format!("{}.{}", m.trans_names[h], m.trans_names[f]),
                pre,
                post,
                m.trans_labels[h].clone(),
            ));
        }
    }
    for &t in feeders.iter().chain(consumers) {
        m.kill_transition(t);
    }
    let place = m.place_names[p].clone();
    m.kill_place(p);
    let mut names = Vec::new();
    for (name, pre, post, label) in created {
        let mut unique = name.clone();
        let mut suffix = 1;
        while m.trans_names.contains(&unique) {
            unique = format!("{name}#{suffix}");
            suffix += 1;
        }
        names.push(unique.clone());
        m.trans_names.push(unique);
        m.trans_labels.push(label);
        m.trans_alive.push(true);
        m.pre.push(pre);
        m.post.push(post);
    }
    out.push(Agglomeration { rule, place, new_transitions: names });
    true
}

/// Rules 14, 15 and 16: tries to agglomerate around place `p`.
fn try_agglomeration(
    m: &mut Model,
    p: usize,
    cfg: &ThierryMiegConfig,
    out: &mut Vec<Agglomeration>,
) -> Option<u8> {
    if m.in_support(p) || m.m0[p] != 0 {
        return None;
    }
    let feeders = m.feeders(p);
    let consumers = m.consumers(p);
    if feeders.is_empty()
        || consumers.is_empty()
        || feeders.len() * consumers.len() > cfg.max_agglomerate_product
        || feeders.iter().any(|h| consumers.contains(h))
    {
        return None;
    }
    let h_single_output = |h: usize| m.post[h].len() == 1 && m.post[h].get(&p) == Some(&1);
    let consume_one = consumers.iter().all(|&f| m.pre[f].get(&p) == Some(&1));
    let feeders_stutter = feeders.iter().all(|&h| m.stuttering(h));

    // Rule 14: pre agglomeration.
    let pre_ok = feeders_stutter
        && consume_one
        && feeders.iter().all(|&h| {
            h_single_output(h)
                && m.pre[h].iter().any(|(q, w)| m.post[h].get(q).copied().unwrap_or(0) < *w)
                && m.pre[h].keys().all(|&p2| m.consumers(p2) == vec![h])
        });
    if pre_ok {
        return agglomerate(m, p, &feeders, &consumers, 14, out).then_some(14);
    }
    // Rule 15: post agglomeration.
    let post_ok = consumers.iter().all(|&f| m.stuttering(f) && m.pre[f].len() == 1)
        && feeders.iter().all(|&h| {
            consumers.iter().all(|&f| m.post[h][&p] % m.pre[f][&p] == 0)
        });
    if post_ok {
        return agglomerate(m, p, &feeders, &consumers, 15, out).then_some(15);
    }
    // Rule 16: free agglomeration (safety only).
    let free_ok = m.safety && feeders_stutter && consume_one && feeders.iter().all(|&h| h_single_output(h));
    if free_ok {
        return agglomerate(m, p, &feeders, &consumers, 16, out).then_some(16);
    }
    None
}

// ── driver ───────────────────────────────────────────────────────────────────

/// Applies the rules to a fixed point.
pub fn reduce(
    net: &PetriNet,
    goal: &ReductionGoal,
    config: &ThierryMiegConfig,
) -> Result<ThierryMiegResult, ReductionError> {
    let mut m = Model::from_net(net, goal)?;
    let mut applications: BTreeMap<u8, usize> = BTreeMap::new();
    let mut agglomerations = Vec::new();
    let bump = |rule: u8, n: usize, map: &mut BTreeMap<u8, usize>| {
        if n > 0 {
            *map.entry(rule).or_insert(0) += n;
        }
        n > 0
    };
    let mut rounds = 0;
    let mut deadlock_free = false;
    while rounds < config.max_rounds {
        rounds += 1;
        if rule6(&mut m) {
            bump(6, 1, &mut applications);
            deadlock_free = true;
            break;
        }
        let mut changed = false;
        changed |= bump(1, rule1(&mut m), &mut applications);
        changed |= bump(2, rule2(&mut m), &mut applications);
        changed |= bump(3, rule3(&mut m, config), &mut applications);
        changed |= bump(4, rule4(&mut m), &mut applications);
        changed |= bump(5, rule5(&mut m), &mut applications);
        changed |= bump(10, rule10(&mut m), &mut applications);
        changed |= bump(9, rule9(&mut m), &mut applications);
        changed |= bump(11, rule11(&mut m), &mut applications);
        changed |= bump(7, rule7(&mut m), &mut applications);
        changed |= bump(8, rule8(&mut m), &mut applications);
        changed |= bump(17, rule17(&mut m), &mut applications);
        for p in m.alive_p() {
            if !m.place_alive[p] {
                continue;
            }
            if let Some(rule) = try_agglomeration(&mut m, p, config, &mut agglomerations) {
                bump(rule, 1, &mut applications);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let kept_p = m.alive_p();
    let kept_t = m.alive_t();
    let place_index: BTreeMap<usize, usize> =
        kept_p.iter().enumerate().map(|(new, old)| (*old, new)).collect();
    let initial_marking: Vec<u64> = kept_p.iter().map(|&p| m.m0[p]).collect();
    let remaining_places: Vec<String> = kept_p.iter().map(|&p| m.place_names[p].clone()).collect();
    let remaining_transitions: Vec<String> =
        kept_t.iter().map(|&t| m.trans_names[t].clone()).collect();

    let arcs = kept_t.iter().map(|&t| m.pre[t].len() + m.post[t].len()).sum::<usize>();
    let reduced_net = if kept_p.is_empty() || kept_t.is_empty() || arcs == 0 {
        None
    } else {
        let mut builder = PetriNetBuilder::default();
        for &p in &kept_p {
            builder.add_place(m.place_names[p].clone(), m.place_labels[p].clone());
        }
        let mut tids = Vec::new();
        for &t in &kept_t {
            tids.push(builder.add_transition(m.trans_names[t].clone(), m.trans_labels[t].clone()));
        }
        for (i, &t) in kept_t.iter().enumerate() {
            let tid: TransitionId = tids[i];
            for (p, w) in &m.pre[t] {
                builder.add_arc(
                    ArcEndpoint::Place(PlaceId(place_index[p] as u32)),
                    ArcEndpoint::Transition(tid),
                    *w as u32,
                );
            }
            for (p, w) in &m.post[t] {
                builder.add_arc(
                    ArcEndpoint::Transition(tid),
                    ArcEndpoint::Place(PlaceId(place_index[p] as u32)),
                    *w as u32,
                );
            }
        }
        let tokens = initial_marking.iter().map(|&n| TokenValue::Finite(n)).collect();
        builder.build(tokens).ok()
    };

    Ok(ThierryMiegResult {
        reduced_net,
        remaining_places,
        remaining_transitions,
        initial_marking,
        deadlock_free,
        applications,
        agglomerations,
        rounds,
    })
}
