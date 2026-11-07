// GSPN graph construction — CTMC from PetriNet + GspnSpec
//
// Algorithm:
// 1. BFS from initial marking, classifying each marking as Tangible, Vanishing, or Deadlock
// 2. Vanishing markings are resolved through immediate transitions (priority + weight)
// 3. Q matrix built from tangible→tangible transitions (rates summed over all paths)
//
// PetriNet is immutable (Arc<PetriNet>) — all successor markings produced via net.fire().

use crate::{GspnError, GspnSpec, TransitionType};
use racon_core::net::{PetriNet, TransitionId};
use racon_core::Marking;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A tangible state in the GSPN — marking with no enabled immediate transitions.
#[derive(Debug, Clone)]
pub struct TangibleState {
    pub marking: Marking,
    pub index: usize,
}

/// CTMC generator matrix Q built from PetriNet + GspnSpec.
///
/// Stores the CTMC as adjacency lists: for each tangible state i,
/// `transitions[i]` = `vec![(j, rate)]` for i != j.
/// `diagonal[i]` = `-sum_j transitions[i].rate` (computed on construction).
#[derive(Debug, Clone)]
pub struct QMatrix {
    pub n: usize,
    /// transitions[i] = list of (j, rate) for i != j
    pub transitions: Vec<Vec<(usize, f64)>>,
    /// Q[i][i] = -sum_{j != i} Q[i][j]
    pub diagonal: Vec<f64>,
}

/// The GSPN graph: PetriNet + GspnSpec → CTMC (Q matrix).
#[derive(Debug, Clone)]
pub struct GspnGraph {
    pub net: Arc<PetriNet>,
    pub spec: GspnSpec,
    pub tangible_states: Vec<TangibleState>,
    /// Marking hash → tangible index
    pub marking_to_tangible: HashMap<Marking, usize>,
    pub q_matrix: QMatrix,
}

// ---------------------------------------------------------------------------
// State classification
// ---------------------------------------------------------------------------

/// Classification of a marking during GSPN state-space exploration.
#[derive(Debug, PartialEq)]
enum StateClass {
    /// At least one immediate transition enabled → resolve probabilistically.
    Vanishing,
    /// Only timed transitions enabled → tangible CTMC state.
    Tangible { successors: Vec<(Marking, f64)> },
    /// No transitions enabled at all.
    Deadlock,
}

/// Classify a marking: which timed transitions are enabled (tangible) or
/// which immediate transitions are enabled (vanishing).
fn classify_marking(net: &PetriNet, spec: &GspnSpec, marking: &Marking) -> StateClass {
    let n_trans = net.transition_count();
    let mut enabled_timed: Vec<(TransitionId, f64)> = Vec::new();
    let mut has_immediate = false;

    for i in 0..n_trans {
        let tid = TransitionId(i as u32);
        if net.is_enabled(tid, marking) {
            match spec.transition_types.get(&tid) {
                Some(TransitionType::Timed { rate }) => {
                    enabled_timed.push((tid, *rate));
                }
                Some(TransitionType::Immediate { .. }) => {
                    has_immediate = true;
                }
                None => {
                    // P/T transition without GSPN type — ignore in GSPN context
                }
            }
        }
    }

    if has_immediate {
        return StateClass::Vanishing;
    }

    if enabled_timed.is_empty() {
        return StateClass::Deadlock;
    }

    let successors: Vec<(Marking, f64)> = enabled_timed
        .into_iter()
        .map(|(tid, rate)| (net.fire(tid, marking), rate))
        .collect();

    StateClass::Tangible { successors }
}

// ---------------------------------------------------------------------------
// Vanishing resolution
// ---------------------------------------------------------------------------

/// Given a marking reachable from a tangible state, follow all immediate
/// transitions until reaching either a tangible state or a deadlock.
///
/// Returns `(tangible_markings, probabilities)` where probabilities sum to 1
/// (if at least one path leads to a tangible state).
///
/// Uses a cache under `visited_vanishing` to avoid redundant computation and
/// detect cycles in vanishing subgraphs (returns empty for cycle paths).
fn resolve_vanishing(
    net: &PetriNet,
    spec: &GspnSpec,
    marking: &Marking,
    visited_vanishing: &mut HashMap<Marking, Vec<(Marking, f64)>>,
) -> Vec<(Marking, f64)> {
    // Cache hit — may be empty if a cycle was detected
    if let Some(result) = visited_vanishing.get(marking) {
        return result.clone();
    }

    // Insert empty sentinel to detect cycles during recursion
    visited_vanishing.insert(marking.clone(), Vec::new());

    let n_trans = net.transition_count();
    let mut enabled_imm: Vec<(TransitionId, f64, u32)> = Vec::new();

    for i in 0..n_trans {
        let tid = TransitionId(i as u32);
        if net.is_enabled(tid, marking) {
            if let Some(TransitionType::Immediate { weight, priority }) =
                spec.transition_types.get(&tid)
            {
                enabled_imm.push((tid, *weight, *priority));
            }
        }
    }

    if enabled_imm.is_empty() {
        // This is actually a tangible state or deadlock — return as-is
        let result = vec![(marking.clone(), 1.0)];
        visited_vanishing.insert(marking.clone(), result.clone());
        return result;
    }

    // Find highest priority
    let max_priority = enabled_imm
        .iter()
        .map(|(_, _, p)| p)
        .max()
        .copied()
        .unwrap_or(0);

    let total_weight: f64 = enabled_imm
        .iter()
        .filter(|(_, _, p)| *p == max_priority)
        .map(|(_, w, _)| w)
        .sum();

    let mut results: Vec<(Marking, f64)> = Vec::new();

    for (tid, weight, priority) in &enabled_imm {
        if *priority != max_priority {
            continue;
        }
        let prob = weight / total_weight;
        let next = net.fire(*tid, marking);
        let sub_results = resolve_vanishing(net, spec, &next, visited_vanishing);
        for (sub_marking, sub_prob) in &sub_results {
            results.push((sub_marking.clone(), prob * sub_prob));
        }
    }

    // Merge duplicate target markings (sum their probabilities)
    let mut merged: HashMap<Marking, f64> = HashMap::new();
    for (m, p) in &results {
        *merged.entry(m.clone()).or_insert(0.0) += *p;
    }
    let result: Vec<(Marking, f64)> = merged.into_iter().collect();

    visited_vanishing.insert(marking.clone(), result.clone());
    result
}

// ---------------------------------------------------------------------------
// GspnGraph construction
// ---------------------------------------------------------------------------

impl GspnGraph {
    /// Build the CTMC by expanding the state space with GSPN semantics.
    ///
    /// # Errors
    /// Returns `GspnError::NoTimedTransitions` if the spec has no timed transitions.
    /// Returns `GspnError::AllVanishingMarkings` if no tangible states are reachable.
    pub fn build(net: &Arc<PetriNet>, spec: &GspnSpec) -> Result<Self, GspnError> {
        // Validate: at least one timed transition
        let has_timed = spec
            .transition_types
            .values()
            .any(|tt| matches!(tt, TransitionType::Timed { .. }));
        if !has_timed {
            return Err(GspnError::NoTimedTransitions);
        }

        // BFS state exploration
        let init = net.initial_marking().clone();
        let mut visited: HashMap<Marking, usize> = HashMap::new();
        let mut marking_to_idx: Vec<Marking> = Vec::new();
        let mut is_tangible: Vec<bool> = Vec::new();
        let mut frontier: Vec<Marking> = Vec::new();

        visited.insert(init.clone(), 0);
        marking_to_idx.push(init.clone());
        is_tangible.push(false);
        frontier.push(init);

        // Tangible state tracking
        let mut tangible_states: Vec<TangibleState> = Vec::new();
        let mut marking_to_tangible: HashMap<Marking, usize> = HashMap::new();

        // Q entries: (src_tangible_idx, target_marking, rate)
        let mut q_entries: Vec<(usize, Marking, f64)> = Vec::new();

        // Cache for vanishing resolution
        let mut vanishing_cache: HashMap<Marking, Vec<(Marking, f64)>> = HashMap::new();

        while let Some(marking) = frontier.pop() {
            let state_idx = visited[&marking];

            // Skip if already classified as tangible
            if is_tangible[state_idx] {
                continue;
            }

            match classify_marking(net, spec, &marking) {
                StateClass::Vanishing => {
                    // Resolve vanishing path to tangible outcomes
                    let tangible_results =
                        resolve_vanishing(net, spec, &marking, &mut vanishing_cache);

                    for (tangible_marking, _prob) in &tangible_results {
                        // Register tangible state if not already known
                        if !marking_to_tangible.contains_key(tangible_marking) {
                            let tangible_idx = tangible_states.len();
                            marking_to_tangible.insert((*tangible_marking).clone(), tangible_idx);
                            tangible_states.push(TangibleState {
                                marking: tangible_marking.clone(),
                                index: tangible_idx,
                            });
                        }
                        // Ensure tangible_marking is in visited for frontier expansion
                        if !visited.contains_key(tangible_marking) {
                            let new_idx = visited.len();
                            visited.insert((*tangible_marking).clone(), new_idx);
                            marking_to_idx.push(tangible_marking.clone());
                            is_tangible.push(true);
                            frontier.push(tangible_marking.clone());
                        } else {
                            let idx = visited[tangible_marking];
                            if !is_tangible[idx] {
                                is_tangible[idx] = true;
                                frontier.push(tangible_marking.clone());
                            }
                        }
                    }
                }
                StateClass::Tangible { successors } => {
                    is_tangible[state_idx] = true;

                    // Register this marking as tangible
                    if !marking_to_tangible.contains_key(&marking) {
                        let tangible_idx = tangible_states.len();
                        marking_to_tangible.insert(marking.clone(), tangible_idx);
                        tangible_states.push(TangibleState {
                            marking: marking.clone(),
                            index: tangible_idx,
                        });
                    }
                    let tangible_idx = marking_to_tangible[&marking];

                    for (next_marking, rate) in &successors {
                        // Register successor in visited for BFS
                        if !visited.contains_key(next_marking) {
                            let new_idx = visited.len();
                            visited.insert((*next_marking).clone(), new_idx);
                            marking_to_idx.push(next_marking.clone());
                            is_tangible.push(false);
                            frontier.push(next_marking.clone());
                        }
                        // Record outgoing Q entry (target may be vanishing)
                        q_entries.push((tangible_idx, next_marking.clone(), *rate));
                    }
                }
                StateClass::Deadlock => {
                    // Deadlock is a tangible state with no successors
                    is_tangible[state_idx] = true;
                    if !marking_to_tangible.contains_key(&marking) {
                        let tangible_idx = tangible_states.len();
                        marking_to_tangible.insert(marking.clone(), tangible_idx);
                        tangible_states.push(TangibleState {
                            marking: marking.clone(),
                            index: tangible_idx,
                        });
                    }
                }
            }
        }

        // Build Q matrix: resolve vanishing targets in q_entries
        let n = tangible_states.len();
        let mut transitions: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let mut row_sums = vec![0.0_f64; n];

        for (src_idx, target_marking, rate) in &q_entries {
            if let Some(&tangible_idx) = marking_to_tangible.get(target_marking) {
                // Direct tangible → tangible
                transitions[*src_idx].push((tangible_idx, *rate));
                row_sums[*src_idx] += *rate;
            } else {
                // target_marking is vanishing — resolve through immediate transitions
                let resolved = resolve_vanishing(net, spec, target_marking, &mut vanishing_cache);
                for (resolved_marking, prob) in &resolved {
                    if let Some(&tangible_idx) = marking_to_tangible.get(resolved_marking) {
                        let effective_rate = *rate * prob;
                        transitions[*src_idx].push((tangible_idx, effective_rate));
                        row_sums[*src_idx] += effective_rate;
                    }
                }
            }
        }

        // Merge duplicate transitions (same source → same target via different paths)
        for i in 0..n {
            let mut merged: HashMap<usize, f64> = HashMap::new();
            for (j, rate) in &transitions[i] {
                *merged.entry(*j).or_insert(0.0) += *rate;
            }
            transitions[i] = merged.into_iter().collect();

            // Recompute row sum from merged values
            row_sums[i] = transitions[i].iter().map(|(_, r)| r).sum();
        }

        // Diagonal = -row_sum
        let diagonal: Vec<f64> = row_sums.iter().map(|s| -s).collect();

        let q_matrix = QMatrix {
            n,
            transitions,
            diagonal,
        };

        if tangible_states.is_empty() {
            return Err(GspnError::AllVanishingMarkings);
        }

        Ok(GspnGraph {
            net: net.clone(),
            spec: spec.clone(),
            tangible_states,
            marking_to_tangible,
            q_matrix,
        })
    }

    /// Number of tangible states in the CTMC.
    pub fn n_tangible(&self) -> usize {
        self.tangible_states.len()
    }

    /// Get CTMC transition rate from tangible state `i` to tangible state `j`.
    pub fn get_rate(&self, i: usize, j: usize) -> f64 {
        self.q_matrix.transitions[i]
            .iter()
            .find(|(dst, _)| *dst == j)
            .map(|(_, rate)| *rate)
            .unwrap_or(0.0)
    }
}
