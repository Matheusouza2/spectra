use std::collections::HashMap;
use std::sync::Arc;

use racon_core::graph::{EdgeId, NodeId, StateSpace};
use racon_core::marking::Marking;
use racon_core::net::{PetriNet, TransitionId};

#[derive(Debug, Clone)]
pub struct ReachabilityNode {
    pub id: NodeId,
    pub marking: Marking,
    pub depth: u32,
    pub level: u32,
    pub is_expanded: bool,
}

#[derive(Debug, Clone)]
pub struct ReachabilityEdge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub transition: TransitionId,
}

#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub max_depth: u32,
    pub max_level: u32,
    pub construction_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ReachabilityGraph {
    pub net: Arc<PetriNet>,
    pub nodes: Vec<ReachabilityNode>,
    pub edges: Vec<ReachabilityEdge>,
    /// adjacency[node_idx] = vec![target NodeId]
    pub adjacency: Vec<Vec<NodeId>>,
    /// reverse_adjacency[node_idx] = vec![source NodeId]
    pub reverse_adjacency: Vec<Vec<NodeId>>,
    pub edge_adjacency: Vec<Vec<(NodeId, TransitionId)>>,
    /// marking → node index lookup
    pub marking_to_node: HashMap<Marking, usize>,
    pub initial_node: NodeId,
    pub current_level: u32,
    pub level_index: Vec<Vec<NodeId>>,
    pub complete: bool,
    pub stats: GraphStats,
}

impl ReachabilityGraph {
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
            current_level: 0,
            level_index: Vec::new(),
            complete: false,
            stats: GraphStats {
                total_nodes: 0,
                total_edges: 0,
                max_depth: 0,
                max_level: 0,
                construction_time_ms: 0,
            },
        }
    }

    pub fn add_node(&mut self, marking: Marking, depth: u32, level: u32) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(ReachabilityNode {
            id,
            marking,
            depth,
            level,
            is_expanded: false,
        });
        self.adjacency.push(Vec::new());
        self.reverse_adjacency.push(Vec::new());
        self.edge_adjacency.push(Vec::new());
        id
    }

    pub fn add_edge(&mut self, source: NodeId, target: NodeId, transition: TransitionId) -> EdgeId {
        let id = EdgeId(self.edges.len() as u32);
        let si = source.0 as usize;
        let ti = target.0 as usize;
        self.edges.push(ReachabilityEdge {
            id,
            source,
            target,
            transition,
        });
        self.adjacency[si].push(target);
        self.reverse_adjacency[ti].push(source);
        self.edge_adjacency[si].push((target, transition));
        id
    }

    pub fn get_node_by_marking(&self, marking: &Marking) -> Option<usize> {
        self.marking_to_node.get(marking).copied()
    }

    /// BFS path from `from` to `to` (node indices). Returns node path if found.
    pub fn bfs_path(&self, from: usize, to: usize) -> Option<Vec<usize>> {
        if from == to {
            return Some(vec![from]);
        }
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut parent = vec![None; n];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(from);
        visited[from] = true;

        while let Some(u) = queue.pop_front() {
            for &v in &self.adjacency[u] {
                let vi = v.0 as usize;
                if !visited[vi] {
                    visited[vi] = true;
                    parent[vi] = Some(u);
                    if vi == to {
                        let mut path = vec![to];
                        let mut cur = to;
                        while let Some(p) = parent[cur] {
                            path.push(p);
                            cur = p;
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(vi);
                }
            }
        }
        None
    }
}

impl StateSpace for ReachabilityGraph {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        let idx = node.0 as usize;
        if idx < self.adjacency.len() {
            &self.adjacency[idx]
        } else {
            &[]
        }
    }

    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        let idx = node.0 as usize;
        if idx < self.reverse_adjacency.len() {
            &self.reverse_adjacency[idx]
        } else {
            &[]
        }
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
        true
    }

    fn is_exact(&self) -> bool {
        self.complete
    }
}
