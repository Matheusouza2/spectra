use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TransitionId};
use std::collections::HashMap;

/// Build a new PetriNet keeping only the specified elements.
///
/// `keep_places` and `keep_transitions` are sets of old IDs to keep.
/// Arcs referencing removed endpoints are automatically excluded.
/// Optionally `keep_arc_indices` can further filter arcs by index.
/// Returns (new_net, removed_places, removed_transitions).
pub(crate) fn build_filtered_net(
    net: &PetriNet,
    keep_places: &[PlaceId],
    keep_transitions: &[TransitionId],
    keep_arc_indices: Option<&[usize]>,
) -> (PetriNet, usize, usize) {
    let old_places = net.places();
    let old_transitions = net.transitions();
    let old_arcs = net.arcs();

    let removed_places = old_places.len() - keep_places.len();
    let removed_transitions = old_transitions.len() - keep_transitions.len();

    // Build ID maps from old → new
    let mut place_map: HashMap<PlaceId, PlaceId> = HashMap::new();
    for (new_idx, old_id) in keep_places.iter().enumerate() {
        place_map.insert(*old_id, PlaceId(new_idx as u32));
    }
    let mut trans_map: HashMap<TransitionId, TransitionId> = HashMap::new();
    for (new_idx, old_id) in keep_transitions.iter().enumerate() {
        trans_map.insert(*old_id, TransitionId(new_idx as u32));
    }

    let mut builder = PetriNetBuilder::default();

    // Add places in keep order (preserving original order)
    for old_id in keep_places {
        let p = &old_places[old_id.0 as usize];
        builder.add_place(p.name.clone(), p.label.clone());
    }
    // Add transitions in keep order
    for old_id in keep_transitions {
        let t = &old_transitions[old_id.0 as usize];
        builder.add_transition(t.name.clone(), t.label.clone());
    }
    // Collect arcs to add
    let arcs_to_add: Vec<&racon_core::net::Arc> = if let Some(arc_indices) = keep_arc_indices {
        arc_indices.iter().map(|&i| &old_arcs[i]).collect()
    } else {
        old_arcs.iter().collect()
    };

    for arc in arcs_to_add {
        match (arc.source, arc.target) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                if let (Some(&new_p), Some(&new_t)) = (place_map.get(&p), trans_map.get(&t)) {
                    builder.add_arc(
                        ArcEndpoint::Place(new_p),
                        ArcEndpoint::Transition(new_t),
                        arc.weight,
                    );
                }
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                if let (Some(&new_t), Some(&new_p)) = (trans_map.get(&t), place_map.get(&p)) {
                    builder.add_arc(
                        ArcEndpoint::Transition(new_t),
                        ArcEndpoint::Place(new_p),
                        arc.weight,
                    );
                }
            }
            _ => {} // should not happen in a valid net, skip
        }
    }

    // Build with the same initial markings for kept places (in new order)
    let new_initial_tokens: Vec<TokenValue> = {
        let old_marking = net.initial_marking();
        keep_places
            .iter()
            .map(|old_id| old_marking.tokens[old_id.0 as usize].clone())
            .collect()
    };
    let new_net = builder
        .build(new_initial_tokens)
        .expect("build_filtered_net: filtered net should be valid (caller ensures non-empty)");

    (new_net, removed_places, removed_transitions)
}

// ── Structural reduction rules ──

/// Remove places that have no outgoing arcs (sink places).
///
/// A sink place is a place that no transition ever consumes tokens from.
/// Such places are dead for forward analysis and can be removed without
/// affecting reachability or coverability properties.
pub fn eliminate_sink_places(net: &PetriNet) -> (PetriNet, usize) {
    let place_count = net.places().len();
    if place_count == 0 {
        return (net.clone(), 0);
    }

    // For each place, compute whether it has any outgoing arcs
    let mut has_outgoing = vec![false; place_count];
    for arc in net.arcs() {
        if let ArcEndpoint::Place(p) = arc.source {
            if (p.0 as usize) < place_count {
                has_outgoing[p.0 as usize] = true;
            }
        }
    }

    let keep_places: Vec<PlaceId> = net
        .places()
        .iter()
        .filter(|p| has_outgoing[p.id.0 as usize])
        .map(|p| p.id)
        .collect();

    // Guard: don't remove every place
    if keep_places.is_empty() || keep_places.len() == place_count {
        return (net.clone(), 0);
    }

    let keep_transitions: Vec<TransitionId> = net.transitions().iter().map(|t| t.id).collect();

    let (new_net, removed_places, _) =
        build_filtered_net(net, &keep_places, &keep_transitions, None);
    (new_net, removed_places)
}

/// Remove places that have no incoming arcs and zero initial marking (source places).
///
/// A source place can never accumulate tokens (no arcs bring tokens to it)
/// and starts empty. Such places are irrelevant for reachability analysis.
pub fn eliminate_source_places(net: &PetriNet) -> (PetriNet, usize) {
    let place_count = net.places().len();
    if place_count == 0 {
        return (net.clone(), 0);
    }

    // For each place, compute whether it has any incoming arcs
    let mut has_incoming = vec![false; place_count];
    for arc in net.arcs() {
        if let ArcEndpoint::Place(p) = arc.target {
            if (p.0 as usize) < place_count {
                has_incoming[p.0 as usize] = true;
            }
        }
    }

    let marking = net.initial_marking();
    let keep_places: Vec<PlaceId> = net
        .places()
        .iter()
        .filter(|p| {
            let idx = p.id.0 as usize;
            // Keep if it has incoming arcs OR has non-zero initial marking
            has_incoming[idx] || marking.tokens[idx] != TokenValue::Finite(0)
        })
        .map(|p| p.id)
        .collect();

    // Guard: don't remove every place
    if keep_places.is_empty() || keep_places.len() == place_count {
        return (net.clone(), 0);
    }

    let keep_transitions: Vec<TransitionId> = net.transitions().iter().map(|t| t.id).collect();

    let (new_net, removed_places, _) =
        build_filtered_net(net, &keep_places, &keep_transitions, None);
    (new_net, removed_places)
}

/// Remove transitions whose preset and postset are identical (self-loops).
///
/// A self-loop transition reads from and writes to the same places with
/// the same weights. These transitions are equivalent to no-ops and can
/// be removed without changing the state space.
pub fn remove_self_loops(net: &PetriNet) -> (PetriNet, usize) {
    let mut removed = 0usize;
    let keep_transitions: Vec<TransitionId> = net
        .transitions()
        .iter()
        .filter(|t| {
            let pre = net.preset(t.id);
            let post = net.postset(t.id);
            if pre.len() != post.len() {
                return true; // different cardinality → keep
            }
            // Sort by (place_id, weight) for order-independent comparison
            // PlaceId does not implement Ord, so use sort_by_key on the inner u32
            let mut pre_sorted: Vec<_> = pre.to_vec();
            let mut post_sorted: Vec<_> = post.to_vec();
            pre_sorted.sort_by_key(|&(p, w)| (p.0, w));
            post_sorted.sort_by_key(|&(p, w)| (p.0, w));
            let is_self_loop = pre_sorted == post_sorted;
            if is_self_loop {
                removed += 1;
            }
            !is_self_loop
        })
        .map(|t| t.id)
        .collect();

    if removed == 0 {
        return (net.clone(), 0);
    }

    // Guard: don't remove every transition
    if keep_transitions.is_empty() {
        return (net.clone(), 0);
    }

    let keep_places: Vec<PlaceId> = net.places().iter().map(|p| p.id).collect();
    let (new_net, _, removed_trans) =
        build_filtered_net(net, &keep_places, &keep_transitions, None);
    (new_net, removed_trans)
}

/// Remove duplicate places and duplicate transitions.
///
/// Two places are duplicates if they have identical presets and postsets
/// (same incoming/outgoing transitions with same weights). Two transitions
/// are duplicates if they have identical presets and postsets. Only the
/// first occurrence of each structure is kept.
pub fn remove_duplicates(net: &PetriNet) -> (PetriNet, usize) {
    let place_count = net.places().len();
    let trans_count = net.transitions().len();

    // Pre-sort transition presets/postsets once for O(n²) duplicate detection
    let sorted_t_presets: Vec<Vec<(PlaceId, u32)>> = (0..trans_count)
        .map(|j| {
            let mut v = net.preset(TransitionId(j as u32)).to_vec();
            v.sort_by_key(|&(p, w)| (p.0, w));
            v
        })
        .collect();
    let sorted_t_postsets: Vec<Vec<(PlaceId, u32)>> = (0..trans_count)
        .map(|j| {
            let mut v = net.postset(TransitionId(j as u32)).to_vec();
            v.sort_by_key(|&(p, w)| (p.0, w));
            v
        })
        .collect();

    // Build place-preset and place-postset from arcs (no built-in methods exist)
    let mut place_preset: Vec<Vec<(TransitionId, u32)>> = vec![Vec::new(); place_count];
    let mut place_postset: Vec<Vec<(TransitionId, u32)>> = vec![Vec::new(); place_count];

    for arc in net.arcs() {
        match (arc.source, arc.target, arc.weight) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t), w) => {
                place_postset[p.0 as usize].push((t, w));
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p), w) => {
                place_preset[p.0 as usize].push((t, w));
            }
            _ => {}
        }
    }

    // Normalize by sorting for order-independent comparison
    for list in place_preset.iter_mut() {
        list.sort_by_key(|&(t, w)| (t.0, w));
    }
    for list in place_postset.iter_mut() {
        list.sort_by_key(|&(t, w)| (t.0, w));
    }

    // Detect duplicate places
    let mut keep_places = Vec::new();
    for i in 0..place_count {
        let pid = PlaceId(i as u32);
        let is_dup = keep_places.iter().any(|&other: &PlaceId| {
            let oi = other.0 as usize;
            place_preset[i] == place_preset[oi] && place_postset[i] == place_postset[oi]
        });
        if !is_dup {
            keep_places.push(pid);
        }
    }

    // Detect duplicate transitions
    let mut keep_transitions = Vec::new();
    for j in 0..trans_count {
        let tid = TransitionId(j as u32);
        let is_dup = keep_transitions.iter().any(|&other: &TransitionId| {
            let oi = other.0 as usize;
            sorted_t_presets[j] == sorted_t_presets[oi]
                && sorted_t_postsets[j] == sorted_t_postsets[oi]
        });
        if !is_dup {
            keep_transitions.push(tid);
        }
    }

    let removed = (place_count - keep_places.len()) + (trans_count - keep_transitions.len());
    if removed == 0 {
        return (net.clone(), 0);
    }

    // Guard: don't remove everything (must keep at least one of each)
    if keep_places.is_empty() || keep_transitions.is_empty() {
        return (net.clone(), 0);
    }

    let (new_net, _, _) = build_filtered_net(net, &keep_places, &keep_transitions, None);
    (new_net, removed)
}
