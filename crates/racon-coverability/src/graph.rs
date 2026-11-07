use std::collections::HashMap;
use std::sync::Arc;

use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{PetriNet, PlaceId, TransitionId};

#[derive(Debug, Clone)]
pub struct CoverabilityNode {
    pub id: NodeId,
    pub marking: Marking,
    pub depth: u32,
    pub is_expanded: bool,
    pub has_omega: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct CoverabilityEdge {
    pub id: u32,
    pub source: NodeId,
    pub target: NodeId,
    pub transition: TransitionId,
    pub is_abstract: bool,
}

#[derive(Debug, Clone)]
pub struct CoverabilityStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub omega_nodes: usize,
    pub pruned_nodes: usize,
    pub construction_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CoverabilityGraph {
    pub net: Arc<PetriNet>,
    pub nodes: Vec<CoverabilityNode>,
    pub edges: Vec<CoverabilityEdge>,
    /// adjacency[node_idx] = vec![target NodeId]
    pub adjacency: Vec<Vec<NodeId>>,
    /// reverse_adjacency[node_idx] = vec![source NodeId]
    pub reverse_adjacency: Vec<Vec<NodeId>>,
    pub edge_adjacency: Vec<Vec<(NodeId, TransitionId)>>,
    /// marking → node index lookup
    pub marking_to_node: HashMap<Marking, usize>,
    pub initial_node: NodeId,
    pub parent: HashMap<NodeId, NodeId>,
    /// Set only when the selected construction gives a complete boundedness proof.
    pub bounded_proof_available: bool,
    pub stats: CoverabilityStats,
}

impl CoverabilityGraph {
    pub fn with_net(net: Arc<PetriNet>) -> Self {
        Self {
            net,
            nodes: Vec::new(),
            edges: Vec::new(),
            adjacency: Vec::new(),
            reverse_adjacency: Vec::new(),
            edge_adjacency: Vec::new(),
            marking_to_node: HashMap::new(),
            initial_node: NodeId(0),
            parent: HashMap::new(),
            bounded_proof_available: false,
            stats: CoverabilityStats {
                total_nodes: 0,
                total_edges: 0,
                omega_nodes: 0,
                pruned_nodes: 0,
                construction_time_ms: 0,
            },
        }
    }

    pub fn add_node(&mut self, marking: Marking, depth: u32) -> NodeId {
        let has_omega = marking
            .tokens
            .iter()
            .any(|t| matches!(t, TokenValue::Omega));
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(CoverabilityNode {
            id,
            marking,
            depth,
            is_expanded: false,
            has_omega,
            is_active: true,
        });
        self.adjacency.push(Vec::new());
        self.reverse_adjacency.push(Vec::new());
        self.edge_adjacency.push(Vec::new());
        id
    }

    pub fn add_edge(&mut self, source: NodeId, target: NodeId, transition: TransitionId) {
        let id = self.edges.len() as u32;
        let si = source.0 as usize;
        let ti = target.0 as usize;
        self.edges.push(CoverabilityEdge {
            id,
            source,
            target,
            transition,
            is_abstract: false,
        });
        self.adjacency[si].push(target);
        self.reverse_adjacency[ti].push(source);
        self.edge_adjacency[si].push((target, transition));
    }

    pub fn active_edges_are_consistent(&self) -> bool {
        self.edges.iter().all(|edge| {
            self.nodes[edge.source.0 as usize].is_active
                && self.nodes[edge.target.0 as usize].is_active
                && self.adjacency[edge.source.0 as usize].contains(&edge.target)
                && self.reverse_adjacency[edge.target.0 as usize].contains(&edge.source)
        })
    }

    /// Build path from a node to the root (ancestors).
    pub fn get_path_to_root(&self, node_id: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut current = node_id;
        while let Some(&parent_id) = self.parent.get(&current) {
            path.push(parent_id);
            current = parent_id;
        }
        path.reverse();
        path
    }

    /// Detect which places are unbounded (have ω in any node).
    pub fn detect_unbounded_places(&self) -> Vec<PlaceId> {
        if self.nodes.is_empty() {
            return Vec::new();
        }
        let place_count = self.nodes[0].marking.tokens.len();
        (0..place_count)
            .filter(|&p| {
                self.nodes.iter().any(|n| {
                    n.is_active && matches!(n.marking.tokens.get(p), Some(TokenValue::Omega))
                })
            })
            .map(|p| PlaceId(p as u32))
            .collect()
    }

    /// Return the tri-state boundedness result for the graph.
    ///
    /// `Some(false)` means that an active ω witness proves unboundedness,
    /// `Some(true)` means that the construction provides a boundedness proof,
    /// and `None` means that the graph is inconclusive. In particular, an
    /// inconclusive result must not be converted to `false`.
    pub fn is_bounded(&self) -> Option<bool> {
        self.bounded_status()
    }

    /// Remove active nodes that are strictly dominated by another active node.
    ///
    /// This is the Finkel minimal-coverability pruning: the remaining nodes are
    /// the minimal coverability set. Edges touching a removed node are dropped
    /// and the adjacency structures are rebuilt so the graph stays consistent.
    pub fn prune_strictly_dominated(&mut self) -> usize {
        let count = self.nodes.len();
        let mut dominated = vec![false; count];
        for victim in 0..count {
            if !self.nodes[victim].is_active {
                continue;
            }
            for dominator in 0..count {
                if dominator == victim || !self.nodes[dominator].is_active {
                    continue;
                }
                if strictly_dominates(&self.nodes[dominator].marking, &self.nodes[victim].marking) {
                    dominated[victim] = true;
                    break;
                }
            }
        }

        let pruned = dominated.iter().filter(|&&is_dominated| is_dominated).count();
        if pruned == 0 {
            return 0;
        }
        for (index, is_dominated) in dominated.into_iter().enumerate() {
            if is_dominated {
                self.nodes[index].is_active = false;
            }
        }

        self.edges.retain(|edge| {
            self.nodes[edge.source.0 as usize].is_active
                && self.nodes[edge.target.0 as usize].is_active
        });
        for (index, edge) in self.edges.iter_mut().enumerate() {
            edge.id = index as u32;
        }
        self.adjacency = vec![Vec::new(); count];
        self.reverse_adjacency = vec![Vec::new(); count];
        self.edge_adjacency = vec![Vec::new(); count];
        let edges = self.edges.clone();
        for edge in &edges {
            self.adjacency[edge.source.0 as usize].push(edge.target);
            self.reverse_adjacency[edge.target.0 as usize].push(edge.source);
            self.edge_adjacency[edge.source.0 as usize].push((edge.target, edge.transition));
        }
        self.marking_to_node
            .retain(|_, &mut index| self.nodes[index].is_active);

        self.stats.pruned_nodes += pruned;
        self.stats.total_nodes = self.nodes.len();
        self.stats.total_edges = self.edges.len();
        pruned
    }

    /// Return a definitive boundedness result when the construction supports
    /// one. Dominance pruning can establish coverability termination without
    /// producing Omega, so a pruned graph is not evidence of boundedness.
    pub fn bounded_status(&self) -> Option<bool> {
        if self.stats.omega_nodes > 0 {
            Some(false)
        } else if self.stats.pruned_nodes > 0 {
            None
        } else if self.bounded_proof_available {
            Some(true)
        } else {
            None
        }
    }
}

fn strictly_dominates(dominator: &Marking, victim: &Marking) -> bool {
    let mut any_greater = false;
    for (a, b) in dominator.tokens.iter().zip(victim.tokens.iter()) {
        if !a.geq(b) {
            return false;
        }
        if a.gt(b) {
            any_greater = true;
        }
    }
    any_greater
}

impl StateSpace for CoverabilityGraph {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        let idx = node.0 as usize;
        self.adjacency
            .get(idx)
            .map(|v| v.as_slice())
            .unwrap_or_default()
    }

    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        let idx = node.0 as usize;
        self.reverse_adjacency
            .get(idx)
            .map(|v| v.as_slice())
            .unwrap_or_default()
    }

    fn outgoing_edges(&self, node: NodeId) -> Vec<(NodeId, TransitionId)> {
        self.edge_adjacency
            .get(node.0 as usize)
            .cloned()
            .unwrap_or_default()
    }

    fn marking(&self, node: NodeId) -> &Marking {
        &self.nodes[node.0 as usize].marking
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn edge_count(&self) -> usize {
        self.edges.len()
    }

    fn initial_node(&self) -> NodeId {
        self.initial_node
    }

    fn is_finite(&self) -> bool {
        false
    }

    fn is_exact(&self) -> bool {
        false
    }
}
