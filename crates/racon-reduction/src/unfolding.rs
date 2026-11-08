//! Complete finite prefix of the unfolding of a 1-safe Petri net with the
//! adequate order of J. Esparza, S. Römer and W. Vogler, *An Improvement of
//! McMillan's Unfolding Algorithm*, TACAS 1996 (`ImpUnfolding.pdf`, R12).
//!
//! * Definition 3.1 / Procedure 3.2: branching process as lists of conditions
//!   and events; *possible extensions* `PE(β)`.
//! * Definition 3.3-3.5 / Algorithm 3.6: adequate order, local configuration
//!   `[e]`, cut-off events; `e` is appended only if `[e]` contains no cut-off.
//! * Definition 4.1 / Theorem 4.2: the **total** order `≺r` (size, then the
//!   sorted label sequence `φ`, then the Foata normal form compared level by
//!   level). With a total order, `e` is a cut-off iff `Fin` already holds
//!   `[e']` with `Mark([e']) = Mark([e])` and `[e'] ≺ [e]`.
//! * McMillan's order `|C1| < |C2|` (`AdequateOrder::McMillan`) is offered for
//!   comparison; it is also adequate, so the prefix is complete but larger.
//!
//! Scope, as in the paper: finite nets, **1-safe**, ordinary arcs (weight 1),
//! every transition with non-empty preset and postset. 1-safeness is checked
//! while unfolding: two co-related conditions with the same place make the
//! net unsafe and the call fails (the prefix would not be valid). Nets with
//! inhibitor arcs or immediate-transition precedence are refused.
//!
//! The prefix represents the reachable markings, not the transition graph:
//! it is not a replacement for exact reachability, liveness or CTL, and it
//! is not a method for unbounded nets (coverability).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use racon_core::marking::TokenValue;
use racon_core::net::{PetriNet, PlaceId, TransitionId};

use crate::error::ReductionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdequateOrder {
    /// `≺r` of Definition 4.1 (total).
    #[default]
    Erv,
    /// McMillan's `|C1| < |C2|`.
    McMillan,
}

#[derive(Debug, Clone, Copy)]
pub struct UnfoldingConfig {
    pub order: AdequateOrder,
    /// Maximum number of events in the prefix.
    pub max_events: usize,
}

impl Default for UnfoldingConfig {
    fn default() -> Self {
        Self { order: AdequateOrder::Erv, max_events: 1_000_000 }
    }
}

#[derive(Debug, Clone)]
pub struct UnfoldingCondition {
    pub place: PlaceId,
    /// Input event; `None` for the initial conditions.
    pub producer: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct UnfoldingEvent {
    pub transition: TransitionId,
    pub preset: Vec<usize>,
    pub postset: Vec<usize>,
    pub is_cutoff: bool,
    /// Length of the longest causal chain ending here.
    pub depth: usize,
    /// `|[e]|`.
    pub local_configuration_size: usize,
}

#[derive(Debug, Clone)]
pub struct Unfolding {
    pub conditions: Vec<UnfoldingCondition>,
    pub events: Vec<UnfoldingEvent>,
    pub initial_conditions: Vec<usize>,
    pub order: AdequateOrder,
    net_pre: Vec<Vec<usize>>,
    initial_marking: Vec<u32>,
}

#[derive(Clone, PartialEq, Eq)]
struct Key {
    size: usize,
    phi: Vec<usize>,
    levels: Vec<Vec<usize>>,
    erv: bool,
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        let by_size = self.size.cmp(&other.size);
        if !self.erv || by_size != Ordering::Equal {
            return by_size;
        }
        self.phi.cmp(&other.phi).then_with(|| {
            for (a, b) in self.levels.iter().zip(&other.levels) {
                let c = a.len().cmp(&b.len()).then_with(|| a.cmp(b));
                if c != Ordering::Equal {
                    return c;
                }
            }
            self.levels.len().cmp(&other.levels.len())
        })
    }
}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Candidate {
    transition: usize,
    preset: Vec<usize>,
    config: BTreeSet<usize>,
    depth: usize,
    key: Key,
}

struct Builder<'a> {
    pre: &'a [Vec<usize>],
    order: AdequateOrder,
    conditions: Vec<UnfoldingCondition>,
    events: Vec<UnfoldingEvent>,
    /// Local configuration of the producer of each condition (empty if initial).
    cond_past: Vec<BTreeSet<usize>>,
    /// Conditions consumed by some event in `cond_past`.
    cond_below: Vec<BTreeSet<usize>>,
    event_config: Vec<BTreeSet<usize>>,
    by_place: Vec<Vec<usize>>,
}

impl Builder<'_> {
    fn conflict(&self, c: usize, d: usize) -> bool {
        let mut consumed: HashMap<usize, usize> = HashMap::new();
        for &e in self.cond_past[c].iter().chain(self.cond_past[d].iter()) {
            for &b in &self.events[e].preset {
                if let Some(&other) = consumed.get(&b) {
                    if other != e {
                        return true;
                    }
                }
                consumed.insert(b, e);
            }
        }
        false
    }

    fn co(&self, c: usize, d: usize) -> bool {
        c != d
            && !self.cond_below[d].contains(&c)
            && !self.cond_below[c].contains(&d)
            && !self.conflict(c, d)
    }

    fn make_key(&self, transition: usize, config: &BTreeSet<usize>, depth: usize) -> Key {
        let mut phi: Vec<usize> = config.iter().map(|&e| self.events[e].transition.0 as usize).collect();
        phi.push(transition);
        phi.sort_unstable();
        let mut levels: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &e in config {
            levels.entry(self.events[e].depth).or_default().push(self.events[e].transition.0 as usize);
        }
        levels.entry(depth).or_default().push(transition);
        let levels = levels
            .into_values()
            .map(|mut level| {
                level.sort_unstable();
                level
            })
            .collect();
        Key { size: config.len() + 1, phi, levels, erv: self.order == AdequateOrder::Erv }
    }

    /// New possible extensions that use at least one condition of `fresh`.
    fn extensions(&self, fresh: &[usize], known: &mut HashSet<(usize, Vec<usize>)>, out: &mut Vec<Candidate>) {
        for &c in fresh {
            let place = self.conditions[c].place.0 as usize;
            for t in 0..self.pre.len() {
                if !self.pre[t].contains(&place) {
                    continue;
                }
                let slot = self.pre[t].iter().position(|&p| p == place).unwrap();
                let mut chosen: Vec<Option<usize>> = vec![None; self.pre[t].len()];
                chosen[slot] = Some(c);
                self.extend_rec(t, 0, &mut chosen, known, out);
            }
        }
    }

    fn extend_rec(
        &self,
        t: usize,
        index: usize,
        chosen: &mut Vec<Option<usize>>,
        known: &mut HashSet<(usize, Vec<usize>)>,
        out: &mut Vec<Candidate>,
    ) {
        if index == chosen.len() {
            let preset: Vec<usize> = chosen.iter().map(|c| c.unwrap()).collect();
            if !known.insert((t, preset.clone())) {
                return;
            }
            let mut config = BTreeSet::new();
            let mut depth = 0;
            for &b in &preset {
                config.extend(self.cond_past[b].iter().copied());
                if let Some(e) = self.conditions[b].producer {
                    depth = depth.max(self.events[e].depth);
                }
            }
            let depth = depth + 1;
            let key = self.make_key(t, &config, depth);
            out.push(Candidate { transition: t, preset, config, depth, key });
            return;
        }
        if chosen[index].is_some() {
            return self.extend_rec(t, index + 1, chosen, known, out);
        }
        let place = self.pre[t][index];
        for &d in &self.by_place[place] {
            if chosen.iter().flatten().all(|&other| self.co(other, d)) {
                chosen[index] = Some(d);
                self.extend_rec(t, index + 1, chosen, known, out);
                chosen[index] = None;
            }
        }
    }
}

/// Builds the complete finite prefix (Algorithm 3.6).
pub fn unfold(net: &PetriNet, config: &UnfoldingConfig) -> Result<Unfolding, ReductionError> {
    const NAME: &str = "unfolding_prefix";
    let not_applicable = |reason: &'static str| ReductionError::NotApplicable { name: NAME, reason };
    if net.is_extended() {
        return Err(not_applicable("nets with inhibitor arcs or immediate-transition precedence are not supported"));
    }
    let places = net.place_count();
    let mut m0 = vec![0u32; places];
    for (p, token) in net.initial_marking().tokens.iter().enumerate() {
        match token {
            TokenValue::Finite(0) => {}
            TokenValue::Finite(1) => m0[p] = 1,
            TokenValue::Finite(_) => {
                return Err(ReductionError::Incompatible { reason: format!("initial marking of '{}' exceeds 1: the net is not 1-safe", net.places()[p].name) })
            }
            _ => return Err(not_applicable("initial marking is not a finite u64 vector")),
        }
    }
    let mut pre: Vec<Vec<usize>> = Vec::new();
    let mut post: Vec<Vec<usize>> = Vec::new();
    for t in net.transitions() {
        let (a, b) = (net.preset(t.id), net.postset(t.id));
        if a.is_empty() || b.is_empty() {
            return Err(not_applicable("every transition needs a non-empty preset and postset"));
        }
        if a.iter().chain(b.iter()).any(|&(_, w)| w != 1) {
            return Err(not_applicable("arc weights must be 1"));
        }
        pre.push(a.iter().map(|&(p, _)| p.0 as usize).collect());
        post.push(b.iter().map(|&(p, _)| p.0 as usize).collect());
    }

    let mut b = Builder {
        pre: &pre,
        order: config.order,
        conditions: Vec::new(),
        events: Vec::new(),
        cond_past: Vec::new(),
        cond_below: Vec::new(),
        event_config: Vec::new(),
        by_place: vec![Vec::new(); places],
    };
    let mut initial_conditions = Vec::new();
    for p in 0..places {
        if m0[p] == 1 {
            let id = b.conditions.len();
            b.conditions.push(UnfoldingCondition { place: PlaceId(p as u32), producer: None });
            b.cond_past.push(BTreeSet::new());
            b.cond_below.push(BTreeSet::new());
            b.by_place[p].push(id);
            initial_conditions.push(id);
        }
    }
    let mut known: HashSet<(usize, Vec<usize>)> = HashSet::new();
    let mut pe: Vec<Candidate> = Vec::new();
    b.extensions(&initial_conditions, &mut known, &mut pe);

    let mut cutoff_events: HashSet<usize> = HashSet::new();
    // Best (smallest) event per marking of a local configuration.
    let mut marks: HashMap<Vec<u32>, (Key, usize)> = HashMap::new();

    while !pe.is_empty() {
        let best = (0..pe.len())
            .min_by(|&i, &j| {
                pe[i].key.cmp(&pe[j].key).then_with(|| (pe[i].transition, &pe[i].preset).cmp(&(pe[j].transition, &pe[j].preset)))
            })
            .unwrap();
        let cand = pe.swap_remove(best);
        if cand.config.iter().any(|e| cutoff_events.contains(e)) {
            continue;
        }
        if b.events.len() >= config.max_events {
            return Err(ReductionError::LimitExceeded { limit: config.max_events });
        }
        // Append e and a condition for every output place of t.
        let e = b.events.len();
        let mut cfg = cand.config.clone();
        cfg.insert(e);
        let mut below: BTreeSet<usize> = BTreeSet::new();
        for &ev in &cfg {
            if ev == e {
                below.extend(cand.preset.iter().copied());
            } else {
                below.extend(b.events[ev].preset.iter().copied());
            }
        }
        b.events.push(UnfoldingEvent {
            transition: TransitionId(cand.transition as u32),
            preset: cand.preset.clone(),
            postset: Vec::new(),
            is_cutoff: false,
            depth: cand.depth,
            local_configuration_size: cfg.len(),
        });
        b.event_config.push(cfg.clone());
        let mut fresh = Vec::new();
        for &p in &post[cand.transition] {
            let id = b.conditions.len();
            b.conditions.push(UnfoldingCondition { place: PlaceId(p as u32), producer: Some(e) });
            b.cond_past.push(cfg.clone());
            b.cond_below.push(below.clone());
            b.events[e].postset.push(id);
            fresh.push(id);
        }
        // 1-safeness: a new condition must not be concurrent with a condition of its place.
        for &c in &fresh {
            let p = b.conditions[c].place.0 as usize;
            if b.by_place[p].iter().any(|&d| b.co(c, d)) {
                return Err(ReductionError::Incompatible { reason: format!("place '{}' can hold two tokens: the net is not 1-safe", net.places()[p].name) });
            }
            b.by_place[p].push(c);
        }
        b.extensions(&fresh, &mut known, &mut pe);

        // Cut-off test with Mark([e]) = M0 + sum of effects over [e].
        let mut mark = m0.clone();
        let mut counts: Vec<i64> = mark.iter().map(|&x| i64::from(x)).collect();
        for &ev in &cfg {
            let t = b.events[ev].transition.0 as usize;
            for &p in &pre[t] {
                counts[p] -= 1;
            }
            for &p in &post[t] {
                counts[p] += 1;
            }
        }
        for (slot, c) in mark.iter_mut().zip(counts) {
            *slot = c as u32;
        }
        let key = cand.key;
        match marks.get(&mark) {
            Some((other, _)) if *other < key => {
                b.events[e].is_cutoff = true;
                cutoff_events.insert(e);
            }
            Some((other, _)) if *other <= key => {}
            _ => {
                marks.insert(mark, (key, e));
            }
        }
    }

    Ok(Unfolding {
        conditions: b.conditions,
        events: b.events,
        initial_conditions,
        order: config.order,
        net_pre: pre,
        initial_marking: m0,
    })
}

impl Unfolding {
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
    pub fn cutoff_count(&self) -> usize {
        self.events.iter().filter(|e| e.is_cutoff).count()
    }

    /// Transitions that label no event: never enabled in any reachable
    /// marking (second bullet of completeness).
    pub fn dead_transitions(&self) -> Vec<TransitionId> {
        let used: BTreeSet<u32> = self.events.iter().map(|e| e.transition.0).collect();
        (0..self.net_pre.len() as u32).filter(|t| !used.contains(t)).map(TransitionId).collect()
    }

    /// Markings of the cuts of the prefix: exactly the reachable markings of
    /// the net. `None` if more than `cap` distinct cuts are found.
    pub fn reachable_markings(&self, cap: usize) -> Option<BTreeSet<Vec<u32>>> {
        let mut consumers: Vec<Vec<usize>> = vec![Vec::new(); self.conditions.len()];
        for (e, ev) in self.events.iter().enumerate() {
            for &c in &ev.preset {
                consumers[c].push(e);
            }
        }
        let start: BTreeSet<usize> = self.initial_conditions.iter().copied().collect();
        let mut seen = BTreeSet::from([start.clone()]);
        let mut queue = VecDeque::from([start]);
        let mut markings = BTreeSet::new();
        while let Some(cut) = queue.pop_front() {
            let mut marking = vec![0u32; self.initial_marking.len()];
            for &c in &cut {
                marking[self.conditions[c].place.0 as usize] += 1;
            }
            markings.insert(marking);
            let enabled: BTreeSet<usize> = cut
                .iter()
                .flat_map(|&c| consumers[c].iter().copied())
                .filter(|&e| self.events[e].preset.iter().all(|c| cut.contains(c)))
                .collect();
            for e in enabled {
                let mut next = cut.clone();
                for c in &self.events[e].preset {
                    next.remove(c);
                }
                next.extend(self.events[e].postset.iter().copied());
                if seen.insert(next.clone()) {
                    if seen.len() > cap {
                        return None;
                    }
                    queue.push_back(next);
                }
            }
        }
        Some(markings)
    }

    /// A reachable marking with no enabled transition, if any (`None` for the
    /// whole answer if the cut enumeration exceeded `cap`).
    pub fn find_deadlock(&self, cap: usize) -> Option<Option<Vec<u32>>> {
        let markings = self.reachable_markings(cap)?;
        Some(markings.into_iter().find(|m| {
            !self.net_pre.iter().any(|pre| pre.iter().all(|&p| m[p] >= 1))
        }))
    }
}
