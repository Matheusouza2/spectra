use racon_core::net::{ArcEndpoint, PetriNet, PlaceId, TransitionId};
use std::collections::{HashMap, HashSet, VecDeque};

/// Structural properties of a Petri net derived from its topology.
#[derive(Debug, Clone, Default)]
pub struct StructuralProperties {
    // Topological
    pub is_ordinary: bool,
    pub is_homogeneous: bool,
    pub is_pure: bool,
    pub is_connected: bool,

    // Source/sink checks
    pub has_transitions_without_preset: bool,
    pub has_transitions_without_postset: bool,
    pub has_places_without_preset: bool,
    pub has_places_without_postset: bool,

    // Structural subclasses
    pub is_state_machine: bool,
    pub is_marked_graph: bool,
    pub is_free_choice: bool,
    pub is_extended_free_choice: bool,
    pub is_static_conflict_free: bool,
    pub is_structurally_concurrent: bool,
    pub is_extended_simple: bool,
    pub is_non_blocking: bool,
    pub state_machine_coverable: Option<bool>,
    pub state_machine_decomposable: Option<bool>,
    pub state_machine_allocatable: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::StructuralAnalyzer;
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    fn cycle() -> racon_core::net::PetriNet {
        let mut b = PetriNetBuilder::default();
        let p0 = b.add_place("p0".into(), None);
        let p1 = b.add_place("p1".into(), None);
        let t0 = b.add_transition("t0".into(), None);
        let t1 = b.add_transition("t1".into(), None);
        b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
        b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap()
    }

    #[test]
    fn cycle_is_state_machine_coverable_decomposable_and_allocatable() {
        let net = cycle();
        assert_eq!(
            StructuralAnalyzer::state_machine_coverable(&net),
            Some(true)
        );
        assert_eq!(
            StructuralAnalyzer::state_machine_decomposable(&net),
            Some(true)
        );
        assert_eq!(
            StructuralAnalyzer::state_machine_allocatable(&net),
            Some(true)
        );
    }
}

/// Analyzes the topological structure of a Petri net.
pub struct StructuralAnalyzer;

impl StructuralAnalyzer {
    /// Compute all structural properties of a net.
    pub fn analyze(net: &PetriNet) -> StructuralProperties {
        Self::analyze_with_subclasses(net, true)
    }

    /// Compute inexpensive structural properties without enumerating all
    /// state-machine components and predecessor selections.
    pub fn analyze_basic(net: &PetriNet) -> StructuralProperties {
        Self::analyze_with_subclasses(net, false)
    }

    fn analyze_with_subclasses(net: &PetriNet, include_subclasses: bool) -> StructuralProperties {
        let mut p = StructuralProperties::default();

        // Precompute place presets/postsets from arcs
        let (place_preset, place_postset) = Self::place_adjacencies(net);

        // --- Ordinary: all arc weights == 1 ---
        p.is_ordinary = net.arcs().iter().all(|a| a.weight == 1);

        // --- Homogeneous: for each place, all outgoing arcs have the same weight ---
        p.is_homogeneous = net.places().iter().all(|pl| {
            let weights: HashSet<u32> = place_postset
                .get(&pl.id)
                .map(|v| v.iter().map(|(_, w)| *w).collect())
                .unwrap_or_default();
            weights.len() <= 1
        });

        // --- Pure: no transition has a place in both its pre- and post-set ---
        p.is_pure = net.transitions().iter().all(|t| {
            let pre: HashSet<PlaceId> = net.preset(t.id).iter().map(|(pl, _)| *pl).collect();
            let post: HashSet<PlaceId> = net.postset(t.id).iter().map(|(pl, _)| *pl).collect();
            pre.is_disjoint(&post)
        });

        // --- Source / sink ---
        p.has_transitions_without_preset = net
            .transitions()
            .iter()
            .any(|t| net.preset(t.id).is_empty());
        p.has_transitions_without_postset = net
            .transitions()
            .iter()
            .any(|t| net.postset(t.id).is_empty());
        p.has_places_without_preset = net
            .places()
            .iter()
            .any(|pl| place_preset.get(&pl.id).map_or(true, |v| v.is_empty()));
        p.has_places_without_postset = net
            .places()
            .iter()
            .any(|pl| place_postset.get(&pl.id).map_or(true, |v| v.is_empty()));

        // --- State Machine: each transition has exactly one input and output ---
        p.is_state_machine = net
            .transitions()
            .iter()
            .all(|t| net.preset(t.id).len() == 1 && net.postset(t.id).len() == 1);

        // --- Marked Graph / Synchronisationsgraph: each place has exactly one
        // incoming and one outgoing transition.
        p.is_marked_graph = net.places().iter().all(|pl| {
            place_preset.get(&pl.id).map_or(0, |v| v.len()) == 1
                && place_postset.get(&pl.id).map_or(0, |v| v.len()) == 1
        });

        // --- Free Choice: shared input places must be the entire unit preset ---
        p.is_free_choice = {
            let mut fc = true;
            for pl in net.places() {
                let outgoing = place_postset.get(&pl.id).map_or(&[][..], Vec::as_slice);
                if outgoing.len() > 1 {
                    for &(tid, _) in outgoing {
                        let preset = net.preset(tid);
                        if preset.len() != 1 || preset[0].0 != pl.id {
                            fc = false;
                            break;
                        }
                    }
                }
                if !fc {
                    break;
                }
            }
            fc
        };

        // --- Extended Free Choice: shared input places have equal presets ---
        p.is_extended_free_choice = net.places().iter().all(|pl| {
            let outgoing = place_postset.get(&pl.id).map_or(&[][..], Vec::as_slice);
            let Some(&(first_tid, _)) = outgoing.first() else {
                return true;
            };
            let first: HashSet<PlaceId> = net.preset(first_tid).iter().map(|(p, _)| *p).collect();
            outgoing.iter().all(|&(tid, _)| {
                let current: HashSet<PlaceId> = net.preset(tid).iter().map(|(p, _)| *p).collect();
                current == first
            })
        });

        // --- Static Conflict Free: each place has ≤1 output transition ---
        p.is_static_conflict_free = net
            .places()
            .iter()
            .all(|pl| place_postset.get(&pl.id).map_or(0, |v| v.len()) <= 1);

        p.is_structurally_concurrent = net.transitions().iter().enumerate().all(|(i, a)| {
            net.transitions().iter().skip(i + 1).all(|b| {
                let ap: HashSet<_> = net.preset(a.id).iter().map(|(p, _)| *p).collect();
                let bp: HashSet<_> = net.preset(b.id).iter().map(|(p, _)| *p).collect();
                ap.is_disjoint(&bp)
            })
        });

        p.is_extended_simple = net.places().iter().enumerate().all(|(i, a)| {
            net.places().iter().skip(i + 1).all(|b| {
                let ap: HashSet<_> = place_postset
                    .get(&a.id)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .map(|(t, _)| *t)
                    .collect();
                let bp: HashSet<_> = place_postset
                    .get(&b.id)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .map(|(t, _)| *t)
                    .collect();
                ap.is_disjoint(&bp) || ap.is_subset(&bp) || bp.is_subset(&ap)
            })
        });

        p.is_non_blocking = net.places().iter().all(|place| {
            let outgoing: Vec<u32> = place_postset
                .get(&place.id)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(|(_, w)| *w)
                .collect();
            let incoming: Vec<u32> = place_preset
                .get(&place.id)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(|(_, w)| *w)
                .collect();
            outgoing.iter().copied().min().unwrap_or(0)
                >= incoming.iter().copied().min().unwrap_or(0)
        });

        if include_subclasses {
            p.state_machine_coverable = Self::state_machine_coverable(net);
            p.state_machine_decomposable = Self::state_machine_decomposable(net);
            p.state_machine_allocatable = Self::state_machine_allocatable(net);
        }

        // --- Connectivity (undirected BFS) ---
        p.is_connected = Self::check_connected(net);

        p
    }

    /// Build place-to-transition adjacency: for each place, which transitions
    /// consume from it (preset) or produce to it (postset).
    fn place_adjacencies(
        net: &PetriNet,
    ) -> (
        HashMap<PlaceId, Vec<(TransitionId, u32)>>,
        HashMap<PlaceId, Vec<(TransitionId, u32)>>,
    ) {
        let mut preset: HashMap<PlaceId, Vec<(TransitionId, u32)>> = HashMap::new();
        let mut postset: HashMap<PlaceId, Vec<(TransitionId, u32)>> = HashMap::new();

        for arc in net.arcs() {
            match (arc.source, arc.target) {
                (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                    postset.entry(p).or_default().push((t, arc.weight));
                }
                (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                    preset.entry(p).or_default().push((t, arc.weight));
                }
                _ => { /* invalid arc, skip */ }
            }
        }

        (preset, postset)
    }

    fn check_connected(net: &PetriNet) -> bool {
        let total = net.places().len() + net.transitions().len();
        if total == 0 {
            return true;
        }

        let n_places = net.places().len();
        let mut adj: Vec<Vec<usize>> = vec![vec![]; total];

        for arc in net.arcs() {
            if let Some((src, tgt)) = Self::node_indices(arc, n_places) {
                adj[src].push(tgt);
                adj[tgt].push(src);
            }
        }

        let mut visited = vec![false; total];
        let mut queue = VecDeque::new();
        queue.push_back(0usize);
        visited[0] = true;

        while let Some(v) = queue.pop_front() {
            for &u in &adj[v] {
                if !visited[u] {
                    visited[u] = true;
                    queue.push_back(u);
                }
            }
        }

        visited.iter().all(|&v| v)
    }

    /// Exact chapter-16 decision procedures. The exhaustive component search
    /// is deliberately bounded; returning None is preferable to reporting a
    /// false negative for a large net.
    pub fn state_machine_coverable(net: &PetriNet) -> Option<bool> {
        Self::coverability_decision(net, false)
    }

    pub fn state_machine_decomposable(net: &PetriNet) -> Option<bool> {
        Self::coverability_decision(net, true)
    }

    fn coverability_decision(net: &PetriNet, strongly_connected: bool) -> Option<bool> {
        let candidates = Self::state_machine_components(net, strongly_connected)?;
        let mut places = HashSet::new();
        let mut transitions = HashSet::new();
        let mut arcs = HashSet::new();
        for (q, ts) in candidates {
            for (index, arc) in net.arcs().iter().enumerate() {
                let included = match (arc.source, arc.target) {
                    (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                        q.contains(&p) && ts.contains(&t)
                    }
                    (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                        q.contains(&p) && ts.contains(&t)
                    }
                    _ => false,
                };
                if included {
                    arcs.insert(index);
                }
            }
            places.extend(q);
            transitions.extend(ts);
        }
        Some(
            places.len() == net.place_count()
                && transitions.len() == net.transition_count()
                && arcs.len() == net.arcs().len(),
        )
    }

    /// Chapter 16.3: every predecessor-place selection must select an SZZM
    /// component. Each transition chooses exactly one of its input places.
    pub fn state_machine_allocatable(net: &PetriNet) -> Option<bool> {
        let components = Self::state_machine_components(net, true)?;
        if components.is_empty() {
            return Some(false);
        }
        let choices: Vec<Vec<PlaceId>> = net
            .transitions()
            .iter()
            .map(|t| net.preset(t.id).iter().map(|(p, _)| *p).collect())
            .collect();
        if choices.iter().any(Vec::is_empty) {
            return Some(false);
        }
        let combinations = choices
            .iter()
            .try_fold(1usize, |total, c| total.checked_mul(c.len()))?;
        if combinations > 1_000_000 {
            return None;
        }
        let transition_ids: Vec<TransitionId> = net.transitions().iter().map(|t| t.id).collect();
        let mut selection = vec![PlaceId(0); choices.len()];
        Some(Self::all_predecessor_selections(
            0,
            &choices,
            &transition_ids,
            &mut selection,
            &components,
        ))
    }

    fn all_predecessor_selections(
        index: usize,
        choices: &[Vec<PlaceId>],
        transition_ids: &[TransitionId],
        selection: &mut [PlaceId],
        components: &[(HashSet<PlaceId>, HashSet<TransitionId>)],
    ) -> bool {
        if index == choices.len() {
            return components.iter().any(|(places, transitions)| {
                transitions.iter().all(|t| {
                    let index = transition_ids
                        .iter()
                        .position(|candidate| candidate == t)
                        .unwrap();
                    places.contains(&selection[index])
                })
            });
        }
        choices[index].iter().copied().any(|place| {
            selection[index] = place;
            Self::all_predecessor_selections(
                index + 1,
                choices,
                transition_ids,
                selection,
                components,
            )
        })
    }

    fn state_machine_components(
        net: &PetriNet,
        strongly_connected: bool,
    ) -> Option<Vec<(HashSet<PlaceId>, HashSet<TransitionId>)>> {
        let n = net.place_count();
        if n > 20 {
            return None;
        }
        let mut result = Vec::new();
        for mask in 1u64..(1u64 << n) {
            let places: HashSet<_> = (0..n)
                .filter(|p| mask & (1u64 << p) != 0)
                .map(|p| PlaceId(p as u32))
                .collect();
            let transitions: HashSet<_> = net
                .transitions()
                .iter()
                .filter(|t| {
                    net.preset(t.id).iter().any(|(p, _)| places.contains(p))
                        || net.postset(t.id).iter().any(|(p, _)| places.contains(p))
                })
                .map(|t| t.id)
                .collect();
            if transitions.is_empty()
                || !transitions.iter().all(|t| {
                    let pre: HashSet<_> = net.preset(*t).iter().map(|(p, _)| *p).collect();
                    let post: HashSet<_> = net.postset(*t).iter().map(|(p, _)| *p).collect();
                    pre.is_subset(&places)
                        && post.is_subset(&places)
                        && pre.len() == 1
                        && post.len() == 1
                })
            {
                continue;
            }
            if strongly_connected && !Self::component_strongly_connected(net, &places, &transitions)
            {
                continue;
            }
            result.push((places, transitions));
        }
        Some(result)
    }

    fn component_strongly_connected(
        net: &PetriNet,
        places: &HashSet<PlaceId>,
        transitions: &HashSet<TransitionId>,
    ) -> bool {
        let Some(&start) = places.iter().next() else {
            return false;
        };
        let walk = |reverse: bool| {
            let mut seen = HashSet::from([start]);
            let mut queue = vec![start];
            while let Some(place) = queue.pop() {
                for t in transitions {
                    let matches = if reverse {
                        net.postset(*t).iter().any(|(p, _)| *p == place)
                    } else {
                        net.preset(*t).iter().any(|(p, _)| *p == place)
                    };
                    if matches {
                        let next = if reverse {
                            net.preset(*t)[0].0
                        } else {
                            net.postset(*t)[0].0
                        };
                        if seen.insert(next) {
                            queue.push(next);
                        }
                    }
                }
            }
            seen.len() == places.len()
        };
        walk(false) && walk(true)
    }

    fn node_indices(arc: &racon_core::net::Arc, n_places: usize) -> Option<(usize, usize)> {
        match (arc.source, arc.target) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                Some((p.0 as usize, n_places + t.0 as usize))
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                Some((n_places + t.0 as usize, p.0 as usize))
            }
            _ => None,
        }
    }
}
