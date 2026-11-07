use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use racon_core::execution::{AnalysisContext, MemoryBudget};
use racon_core::graph::NodeId;
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::PetriNet;
use rayon::prelude::*;

use crate::error::CoverabilityError;
use crate::graph::{CoverabilityEdge, CoverabilityGraph, CoverabilityNode};

const PARALLEL_BATCH_SIZE: usize = 256;

struct TemporaryMemoryReservation {
    budget: Arc<MemoryBudget>,
    bytes: usize,
}

impl Drop for TemporaryMemoryReservation {
    fn drop(&mut self) {
        self.budget.deallocate(self.bytes);
    }
}

struct ComputedSuccessor {
    marking: Marking,
    transition: racon_core::net::TransitionId,
    _reservation: TemporaryMemoryReservation,
}

fn reserve_temporary_memory(
    context: &AnalysisContext,
    bytes: usize,
) -> Result<TemporaryMemoryReservation, CoverabilityError> {
    context.memory_budget.allocate(bytes).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        CoverabilityError::MemoryLimitExceeded {
            current,
            required: current.saturating_add(bytes),
            limit: context.memory_budget.limit_bytes(),
        }
    })?;
    Ok(TemporaryMemoryReservation {
        budget: context.memory_budget.clone(),
        bytes,
    })
}

/// Strategy for building the coverability graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverabilityStrategy {
    /// Pure Karp-Miller acceleration along paths (introduces ω).
    KarpMiller,
    /// Karp-Miller followed by Finkel minimal-coverability pruning of
    /// strictly dominated nodes.
    FinkelMinimalCoverability,
}

impl Default for CoverabilityStrategy {
    fn default() -> Self {
        CoverabilityStrategy::KarpMiller
    }
}

pub trait CoverabilityEngine {
    fn build_complete(
        &self,
        net: &PetriNet,
        strategy: CoverabilityStrategy,
        context: &AnalysisContext,
    ) -> Result<CoverabilityGraph, CoverabilityError>;
}

pub struct CoverabilityConfig {
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl Default for CoverabilityConfig {
    fn default() -> Self {
        Self {
            max_nodes: 5_000_000,
            max_edges: usize::MAX,
        }
    }
}

pub struct CoverabilityEngineImpl {
    pub config: CoverabilityConfig,
}

impl Default for CoverabilityEngineImpl {
    fn default() -> Self {
        Self {
            config: CoverabilityConfig::default(),
        }
    }
}

impl CoverabilityEngine for CoverabilityEngineImpl {
    fn build_complete(
        &self,
        net: &PetriNet,
        strategy: CoverabilityStrategy,
        context: &AnalysisContext,
    ) -> Result<CoverabilityGraph, CoverabilityError> {
        if net.is_extended() {
            return Err(CoverabilityError::Unsupported(
                crate::error::MONOTONICITY_REQUIRED.to_owned(),
            ));
        }
        let start = Instant::now();
        let mut graph = CoverabilityGraph::with_net(Arc::new(net.clone()));

        let initial_marking = net.initial_marking().clone();
        reserve_node_memory(context, &initial_marking)?;
        let initial_node = graph.add_node(initial_marking, 0);
        graph.initial_node = initial_node;

        let mut work_queue: VecDeque<NodeId> = VecDeque::new();
        work_queue.push_back(initial_node);

        while !work_queue.is_empty() {
            context
                .check_cancellation()
                .map_err(|_| CoverabilityError::Cancelled)?;
            let layer_size = work_queue.len();
            let frontier: Vec<_> = work_queue.drain(..layer_size).collect();
            let net = graph.net.clone();

            // Compute each frontier node against a stable graph snapshot, then
            // merge in BFS/transition order so scheduling cannot change node IDs.
            for batch in frontier.chunks(PARALLEL_BATCH_SIZE) {
                let computed: Vec<Result<_, CoverabilityError>> = batch
                    .par_iter()
                    .map(|&node_id| {
                        context
                            .check_cancellation()
                            .map_err(|_| CoverabilityError::Cancelled)?;
                        let idx = node_id.0 as usize;
                        if !graph.nodes[idx].is_active || graph.nodes[idx].is_expanded {
                            return Ok((node_id, Vec::new()));
                        }

                        let marking = &graph.nodes[idx].marking;
                        let path_to_root = graph.get_path_to_root(node_id);
                        let computed_successors: Vec<Result<_, CoverabilityError>> = net
                            .transitions()
                            .par_iter()
                            .map(|transition| {
                                context
                                    .check_cancellation()
                                    .map_err(|_| CoverabilityError::Cancelled)?;
                                if !net.is_enabled(transition.id, marking) {
                                    return Ok(None);
                                }
                                let bytes =
                                    estimate_successor_scratch_bytes(marking, &net, transition.id);
                                let reservation = reserve_temporary_memory(context, bytes)?;
                                let fired = net.fire(transition.id, marking);
                                let accelerated =
                                    self.accelerate_karp_miller(&fired, &path_to_root, &graph);
                                Ok(Some(ComputedSuccessor {
                                    marking: accelerated,
                                    transition: transition.id,
                                    _reservation: reservation,
                                }))
                            })
                            .collect();
                        let successors = computed_successors
                            .into_iter()
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .flatten()
                            .collect();
                        Ok((node_id, successors))
                    })
                    .collect();

                for result in computed {
                    let (node_id, successors) = result?;
                    let idx = node_id.0 as usize;
                    if !graph.nodes[idx].is_active || graph.nodes[idx].is_expanded {
                        continue;
                    }
                    let depth = graph.nodes[idx].depth;
                    graph.nodes[idx].is_expanded = true;

                    for successor in successors {
                        if let Some(&existing_idx) = graph.marking_to_node.get(&successor.marking) {
                            if graph.edges.len() >= self.config.max_edges {
                                return Err(CoverabilityError::EdgeLimitExceeded {
                                    limit: self.config.max_edges,
                                    current: graph.edges.len(),
                                });
                            }
                            reserve_edge_memory(context)?;
                            graph.add_edge(
                                node_id,
                                NodeId(existing_idx as u32),
                                successor.transition,
                            );
                            continue;
                        }
                        if graph.nodes.len() >= self.config.max_nodes {
                            return Err(CoverabilityError::NodeLimitExceeded {
                                limit: self.config.max_nodes,
                                current: graph.nodes.len(),
                            });
                        }
                        if graph.edges.len() >= self.config.max_edges {
                            return Err(CoverabilityError::EdgeLimitExceeded {
                                limit: self.config.max_edges,
                                current: graph.edges.len(),
                            });
                        }
                        reserve_node_memory(context, &successor.marking)?;
                        reserve_edge_memory(context)?;
                        let new_node = graph.add_node(successor.marking.clone(), depth + 1);
                        graph
                            .marking_to_node
                            .insert(successor.marking.clone(), new_node.0 as usize);
                        graph.parent.insert(new_node, node_id);
                        graph.add_edge(node_id, new_node, successor.transition);
                        work_queue.push_back(new_node);
                        if graph.nodes[new_node.0 as usize].has_omega {
                            graph.stats.omega_nodes += 1;
                        }
                    }
                }
            }
        }

        if matches!(strategy, CoverabilityStrategy::FinkelMinimalCoverability) {
            graph.prune_strictly_dominated();
        }
        graph.bounded_proof_available = graph.stats.pruned_nodes == 0;
        graph.stats.construction_time_ms = start.elapsed().as_millis() as u64;
        Ok(graph)
    }
}

// ── Dominance helpers ─────────────────────────────────────────────────────────

impl CoverabilityEngineImpl {
    /// `m1` strictly dominates `m2` if it dominates and is strictly greater in at least one place.
    fn strictly_dominates(m1: &Marking, m2: &Marking) -> bool {
        let mut any_greater = false;
        for (a, b) in m1.tokens.iter().zip(m2.tokens.iter()) {
            if !a.geq(b) {
                return false;
            }
            if a.gt(b) {
                any_greater = true;
            }
        }
        any_greater
    }
}

fn estimate_marking_bytes(marking: &Marking) -> usize {
    marking.tokens.iter().fold(
        marking
            .tokens
            .len()
            .saturating_mul(std::mem::size_of::<TokenValue>()),
        |bytes, token| {
            let extra = match token {
                TokenValue::Exact(value) => ((value.bits() as usize).saturating_add(7)) / 8,
                _ => 0,
            };
            bytes.saturating_add(extra)
        },
    )
}

fn estimate_successor_scratch_bytes(
    marking: &Marking,
    net: &PetriNet,
    transition: racon_core::net::TransitionId,
) -> usize {
    estimate_marking_bytes(marking)
        .saturating_mul(4)
        .saturating_add(marking.tokens.len().saturating_mul(24))
        .saturating_add(
            net.postset(transition)
                .len()
                .saturating_mul(std::mem::size_of::<u32>()),
        )
        .saturating_add(std::mem::size_of::<ComputedSuccessor>())
}

fn reserve_node_memory(
    context: &AnalysisContext,
    marking: &Marking,
) -> Result<(), CoverabilityError> {
    let marking_bytes = estimate_marking_bytes(marking);
    let estimate = std::mem::size_of::<CoverabilityNode>()
        .saturating_mul(2)
        .saturating_add(std::mem::size_of::<Vec<NodeId>>().saturating_mul(3))
        .saturating_add(marking_bytes.saturating_mul(2))
        .saturating_add(std::mem::size_of::<Marking>() + std::mem::size_of::<usize>() * 2 + 320);
    context.memory_budget.allocate(estimate).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        CoverabilityError::MemoryLimitExceeded {
            current,
            required: current.saturating_add(estimate),
            limit: context.memory_budget.limit_bytes(),
        }
    })
}

fn reserve_edge_memory(context: &AnalysisContext) -> Result<(), CoverabilityError> {
    let estimate = std::mem::size_of::<CoverabilityEdge>()
        .saturating_mul(2)
        .saturating_add(std::mem::size_of::<NodeId>().saturating_mul(4))
        .saturating_add(
            std::mem::size_of::<(NodeId, racon_core::net::TransitionId)>().saturating_mul(2),
        )
        .saturating_add(160);
    context.memory_budget.allocate(estimate).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        CoverabilityError::MemoryLimitExceeded {
            current,
            required: current.saturating_add(estimate),
            limit: context.memory_budget.limit_bytes(),
        }
    })
}

// ── Karp-Miller strategy ──────────────────────────────────────────────────────

impl CoverabilityEngineImpl {
    fn accelerate_karp_miller(
        &self,
        m_inter: &Marking,
        path_to_root: &[NodeId],
        graph: &CoverabilityGraph,
    ) -> Marking {
        let mut accelerated = m_inter.clone();
        for &ancestor_id in path_to_root {
            let ancestor = &graph.nodes[ancestor_id.0 as usize];
            if Self::strictly_dominates(&accelerated, &ancestor.marking) {
                // Collect indices first, then mutate to avoid borrow conflict
                let omega_indices: Vec<usize> = accelerated
                    .tokens
                    .iter()
                    .zip(ancestor.marking.tokens.iter())
                    .enumerate()
                    .filter(|(_, (a_val, b_val))| a_val.gt(b_val))
                    .map(|(i, _)| i)
                    .collect();
                for &i in &omega_indices {
                    accelerated.tokens[i] = TokenValue::Omega;
                }
            }
        }
        accelerated
    }
}
