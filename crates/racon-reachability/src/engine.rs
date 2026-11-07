use std::sync::Arc;
use std::time::Instant;

use racon_core::execution::{AnalysisContext, MemoryBudget};
use racon_core::graph::NodeId;
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::PetriNet;
use rayon::prelude::*;

use crate::error::ReachabilityError;
use crate::graph::{ReachabilityEdge, ReachabilityGraph, ReachabilityNode};

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
) -> Result<TemporaryMemoryReservation, ReachabilityError> {
    context.memory_budget.allocate(bytes).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        ReachabilityError::MemoryLimitExceeded {
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
        .saturating_mul(2)
        .saturating_add(marking.tokens.len().saturating_mul(16))
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
) -> Result<(), ReachabilityError> {
    let marking_bytes = estimate_marking_bytes(marking);
    let estimate = std::mem::size_of::<ReachabilityNode>()
        .saturating_mul(2)
        .saturating_add(std::mem::size_of::<Vec<NodeId>>().saturating_mul(3))
        .saturating_add(marking_bytes.saturating_mul(2))
        .saturating_add(std::mem::size_of::<Marking>() + std::mem::size_of::<usize>() + 256);
    context.memory_budget.allocate(estimate).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        ReachabilityError::MemoryLimitExceeded {
            current,
            required: current.saturating_add(estimate),
            limit: context.memory_budget.limit_bytes(),
        }
    })
}

fn reserve_edge_memory(context: &AnalysisContext) -> Result<(), ReachabilityError> {
    let estimate = std::mem::size_of::<ReachabilityEdge>()
        .saturating_mul(2)
        .saturating_add(std::mem::size_of::<NodeId>().saturating_mul(4))
        .saturating_add(
            std::mem::size_of::<(NodeId, racon_core::net::TransitionId)>().saturating_mul(2),
        )
        .saturating_add(128);
    context.memory_budget.allocate(estimate).map_err(|_| {
        let current = context.memory_budget.current_bytes();
        ReachabilityError::MemoryLimitExceeded {
            current,
            required: current.saturating_add(estimate),
            limit: context.memory_budget.limit_bytes(),
        }
    })
}

pub trait ReachabilityEngine {
    /// Build the complete reachability graph from a Petri net.
    fn build_complete(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<ReachabilityGraph, ReachabilityError>;

    /// Expand the graph by a given number of levels.
    fn expand_levels(
        &self,
        graph: &mut ReachabilityGraph,
        levels: u32,
        context: &AnalysisContext,
    ) -> Result<ExpansionResult, ReachabilityError>;
}

pub struct ReachabilityConfig {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub levels_per_expansion: u32,
}

impl Default for ReachabilityConfig {
    fn default() -> Self {
        Self {
            max_nodes: 10_000_000,
            max_edges: 50_000_000,
            levels_per_expansion: 5,
        }
    }
}

pub struct ReachabilityEngineImpl {
    pub config: ReachabilityConfig,
}

impl Default for ReachabilityEngineImpl {
    fn default() -> Self {
        Self {
            config: ReachabilityConfig::default(),
        }
    }
}

impl ReachabilityEngineImpl {
    /// Depth-first construction of the complete reachability graph.
    ///
    /// Produces the same set of markings and the same labelled edges as
    /// [`ReachabilityEngine::build_complete`] (every transition in `Fireable(M)`
    /// is an edge), but node ids follow DFS discovery order and `depth`/`level`
    /// record the depth in the DFS tree, which is **not** the shortest-path
    /// distance. Paths read from this graph are therefore not minimal.
    pub fn build_complete_dfs(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<ReachabilityGraph, ReachabilityError> {
        let start = Instant::now();
        let mut graph = ReachabilityGraph::with_net(Arc::new(net.clone()));

        let initial = net.initial_marking().clone();
        reserve_node_memory(context, &initial)?;
        let root = graph.add_node(initial.clone(), 0, 0);
        graph.initial_node = root;
        graph.marking_to_node.insert(initial, 0);

        // Explicit stack of (node, successors still to visit).
        let mut stack: Vec<(NodeId, std::vec::IntoIter<racon_core::net::TransitionId>)> = vec![(
            root,
            net.fireable_transitions(&graph.nodes[0].marking).into_iter(),
        )];
        while let Some((node, pending)) = stack.last_mut() {
            context
                .check_cancellation()
                .map_err(|_| ReachabilityError::Cancelled)?;
            let node = *node;
            let Some(transition) = pending.next() else {
                graph.nodes[node.0 as usize].is_expanded = true;
                stack.pop();
                continue;
            };
            let source_marking = graph.nodes[node.0 as usize].marking.clone();
            let depth = graph.nodes[node.0 as usize].depth;
            let successor = net.fire(transition, &source_marking);
            let (target, is_new) = match graph.get_node_by_marking(&successor) {
                Some(existing) => (NodeId(existing as u32), false),
                None => {
                    if graph.nodes.len() >= self.config.max_nodes {
                        return Err(ReachabilityError::NodeLimitExceeded {
                            limit: self.config.max_nodes,
                            current: graph.nodes.len(),
                        });
                    }
                    reserve_node_memory(context, &successor)?;
                    let id = graph.add_node(successor.clone(), depth + 1, depth + 1);
                    graph.marking_to_node.insert(successor, id.0 as usize);
                    graph.current_level = graph.current_level.max(depth + 1);
                    (id, true)
                }
            };
            if graph.edges.len() >= self.config.max_edges {
                return Err(ReachabilityError::EdgeLimitExceeded {
                    limit: self.config.max_edges,
                    current: graph.edges.len(),
                });
            }
            reserve_edge_memory(context)?;
            graph.add_edge(node, target, transition);
            if is_new {
                let next = net
                    .fireable_transitions(&graph.nodes[target.0 as usize].marking)
                    .into_iter();
                stack.push((target, next));
            }
        }

        graph.complete = true;
        graph.stats.construction_time_ms = start.elapsed().as_millis() as u64;
        Ok(graph)
    }
}

#[derive(Debug, Clone)]
pub struct ExpansionResult {
    pub new_nodes: usize,
    pub new_edges: usize,
    pub previous_level: u32,
    pub current_level: u32,
    pub is_complete: bool,
    pub elapsed_ms: u64,
}

impl ReachabilityEngine for ReachabilityEngineImpl {
    fn build_complete(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<ReachabilityGraph, ReachabilityError> {
        let start = Instant::now();
        let mut graph = ReachabilityGraph::with_net(Arc::new(net.clone()));

        // Add initial marking
        let initial_marking = net.initial_marking().clone();
        reserve_node_memory(context, &initial_marking)?;
        let initial_node = graph.add_node(initial_marking.clone(), 0, 0);
        graph.initial_node = initial_node;
        graph.marking_to_node.insert(initial_marking, 0);
        graph.level_index.push(vec![initial_node]);

        loop {
            context
                .check_cancellation()
                .map_err(|_| ReachabilityError::Cancelled)?;

            let result =
                self.expand_levels(&mut graph, self.config.levels_per_expansion, context)?;

            if result.is_complete {
                break;
            }
        }

        graph.complete = true;
        graph.stats.construction_time_ms = start.elapsed().as_millis() as u64;
        Ok(graph)
    }

    fn expand_levels(
        &self,
        graph: &mut ReachabilityGraph,
        levels: u32,
        context: &AnalysisContext,
    ) -> Result<ExpansionResult, ReachabilityError> {
        let start = Instant::now();
        let start_nodes = graph.nodes.len();
        let start_edges = graph.edges.len();
        let start_level = graph.current_level;
        let target_level = start_level + levels;

        while graph.current_level < target_level {
            context
                .check_cancellation()
                .map_err(|_| ReachabilityError::Cancelled)?;

            let lvl = graph.current_level as usize;
            let nodes_at_level: Vec<NodeId> =
                graph.level_index.get(lvl).cloned().unwrap_or_default();

            if nodes_at_level.is_empty() {
                return Ok(ExpansionResult {
                    new_nodes: graph.nodes.len() - start_nodes,
                    new_edges: graph.edges.len() - start_edges,
                    previous_level: start_level,
                    current_level: graph.current_level,
                    is_complete: true,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                });
            }

            let mut next_level = Vec::new();
            let net = graph.net.clone();
            let level = graph.current_level;

            // Successor calculations are independent within a BFS layer. Work in
            // bounded chunks, then integrate in input order so node IDs and graph
            // contents stay deterministic across thread schedules.
            for batch in nodes_at_level.chunks(PARALLEL_BATCH_SIZE) {
                let computed: Vec<Result<_, ReachabilityError>> = batch
                    .par_iter()
                    .map(|node_id| {
                        context
                            .check_cancellation()
                            .map_err(|_| ReachabilityError::Cancelled)?;
                        let idx = node_id.0 as usize;
                        if graph.nodes[idx].is_expanded {
                            return Ok((*node_id, graph.nodes[idx].depth, Vec::new()));
                        }

                        let marking = &graph.nodes[idx].marking;
                        let depth = graph.nodes[idx].depth;
                        let computed_successors: Vec<Result<_, ReachabilityError>> = net
                            .transitions()
                            .par_iter()
                            .map(|transition| {
                                context
                                    .check_cancellation()
                                    .map_err(|_| ReachabilityError::Cancelled)?;
                                if !net.is_enabled(transition.id, marking) {
                                    return Ok(None);
                                }
                                let bytes =
                                    estimate_successor_scratch_bytes(marking, &net, transition.id);
                                let reservation = reserve_temporary_memory(context, bytes)?;
                                let successor = net.fire(transition.id, marking);
                                Ok(Some(ComputedSuccessor {
                                    marking: successor,
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
                        Ok((*node_id, depth, successors))
                    })
                    .collect();

                for result in computed {
                    let (node_id, depth, successors) = result?;
                    let source_idx = node_id.0 as usize;
                    for successor in successors {
                        let target =
                            if let Some(existing) = graph.get_node_by_marking(&successor.marking) {
                                NodeId(existing as u32)
                            } else {
                                if graph.nodes.len() >= self.config.max_nodes {
                                    return Err(ReachabilityError::NodeLimitExceeded {
                                        limit: self.config.max_nodes,
                                        current: graph.nodes.len(),
                                    });
                                }
                                reserve_node_memory(context, &successor.marking)?;
                                let id =
                                    graph.add_node(successor.marking.clone(), depth + 1, level + 1);
                                graph
                                    .marking_to_node
                                    .insert(successor.marking.clone(), id.0 as usize);
                                next_level.push(id);
                                id
                            };

                        if graph.edges.len() >= self.config.max_edges {
                            return Err(ReachabilityError::EdgeLimitExceeded {
                                limit: self.config.max_edges,
                                current: graph.edges.len(),
                            });
                        }
                        reserve_edge_memory(context)?;
                        graph.add_edge(node_id, target, successor.transition);
                    }
                    graph.nodes[source_idx].is_expanded = true;
                }
            }

            graph.current_level += 1;
            if !next_level.is_empty() {
                graph.level_index.push(next_level);
            }
        }

        Ok(ExpansionResult {
            new_nodes: graph.nodes.len() - start_nodes,
            new_edges: graph.edges.len() - start_edges,
            previous_level: start_level,
            current_level: graph.current_level,
            is_complete: false,
            elapsed_ms: start.elapsed().as_millis() as u64,
        })
    }
}
