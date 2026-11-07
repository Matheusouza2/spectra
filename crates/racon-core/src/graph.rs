use crate::marking::Marking;
use crate::net::TransitionId;
use std::fmt;

/// Identifies a node in a state space graph (Reachability or Coverability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const MAX: NodeId = NodeId(u32::MAX);
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// Identifies an edge in a state space graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeId(pub u32);

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "e{}", self.0)
    }
}

/// Unified interface for state spaces.
///
/// Both `ReachabilityGraph` and `CoverabilityGraph` implement this trait,
/// enabling analysis engines (SCC, CTL, Property) to operate on either.
pub trait StateSpace: Send + Sync {
    fn successors(&self, node: NodeId) -> &[NodeId];
    fn predecessors(&self, node: NodeId) -> &[NodeId];
    fn outgoing_edges(&self, node: NodeId) -> Vec<(NodeId, TransitionId)> {
        self.successors(node)
            .iter()
            .copied()
            .map(|target| (target, TransitionId(0)))
            .collect()
    }
    fn marking(&self, node: NodeId) -> &Marking;
    fn node_count(&self) -> usize;
    fn edge_count(&self) -> usize;
    fn initial_node(&self) -> NodeId;
    fn is_finite(&self) -> bool;
    /// Whether the graph represents the exact reachable state space.
    /// Coverability graphs are finite abstractions and therefore override this.
    fn is_exact(&self) -> bool {
        true
    }
}
