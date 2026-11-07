//! Integration tests for racon-scc (Tarjan SCC + DAG analysis).

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::Marking;
use racon_scc::dag::SCCDAG;
use racon_scc::tarjan::{SCCEngine, SCCError};

// =============================================================================
// Mock StateSpace — lets us define arbitrary graphs for testing
// =============================================================================

struct MockStateSpace {
    successors: Vec<Vec<NodeId>>,
    predecessors: Vec<Vec<NodeId>>,
    markings: Vec<Marking>,
    node_count: usize,
    edge_count: usize,
    initial_node: NodeId,
}

impl MockStateSpace {
    fn new(successors: Vec<Vec<usize>>, initial_node: usize) -> Self {
        let node_count = successors.len();
        let mut preds: Vec<Vec<NodeId>> = vec![vec![]; node_count];
        let mut edge_count = 0;

        for (u, succs) in successors.iter().enumerate() {
            for &v in succs {
                preds[v].push(NodeId(u as u32));
                edge_count += 1;
            }
        }

        let succs_native: Vec<Vec<NodeId>> = successors
            .iter()
            .map(|s| s.iter().map(|&i| NodeId(i as u32)).collect())
            .collect();

        let markings = (0..node_count).map(|_| Marking::new(vec![])).collect();

        Self {
            successors: succs_native,
            predecessors: preds,
            markings,
            node_count,
            edge_count,
            initial_node: NodeId(initial_node as u32),
        }
    }
}

impl StateSpace for MockStateSpace {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        &self.successors[node.0 as usize]
    }
    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        &self.predecessors[node.0 as usize]
    }
    fn marking(&self, node: NodeId) -> &Marking {
        &self.markings[node.0 as usize]
    }
    fn node_count(&self) -> usize {
        self.node_count
    }
    fn edge_count(&self) -> usize {
        self.edge_count
    }
    fn initial_node(&self) -> NodeId {
        self.initial_node
    }
    fn is_finite(&self) -> bool {
        true
    }
}

fn make_context() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

// =============================================================================
// SCCEngine Tests
// =============================================================================

#[test]
fn test_three_node_cycle() {
    // 0 → 1 → 2 → 0
    let ss = MockStateSpace::new(vec![vec![1], vec![2], vec![0]], 0);
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should compute SCCs");

    assert_eq!(result.sccs.len(), 1, "All 3 nodes in one SCC");
    assert_eq!(result.sccs[0].nodes.len(), 3);
    assert!(!result.sccs[0].is_trivial, "Cycle is non-trivial");
    assert_eq!(result.total_nodes, 3);
}

#[test]
fn test_two_isolated_nodes() {
    // 0  1  (no edges)
    let ss = MockStateSpace::new(vec![vec![], vec![]], 0);
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should compute SCCs");

    assert_eq!(result.sccs.len(), 2, "Two SCCs");
    for scc in &result.sccs {
        assert_eq!(scc.nodes.len(), 1);
        assert!(scc.is_trivial, "Isolated nodes are trivial SCCs");
    }
}

#[test]
fn test_diamond_graph() {
    // 0 → 1, 0 → 2, 1 → 3, 2 → 3
    let ss = MockStateSpace::new(vec![vec![1, 2], vec![3], vec![3], vec![]], 0);
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should compute SCCs");

    assert_eq!(result.sccs.len(), 4, "Each node in its own SCC");
    for scc in &result.sccs {
        assert!(scc.is_trivial, "Diamond has no cycles");
    }
}

#[test]
fn test_self_loop_node() {
    // 0 → 0 (self-loop)
    let ss = MockStateSpace::new(vec![vec![0]], 0);
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should compute SCCs");

    assert_eq!(result.sccs.len(), 1, "Single SCC");
    assert!(!result.sccs[0].is_trivial, "Self-loop is non-trivial");
}

#[test]
fn test_two_sccs_connected() {
    // 0 ⇄ 1, 0 → 2, 2 ⇄ 3  ⇒  SCC1 = {0,1} → SCC2 = {2,3}
    let ss = MockStateSpace::new(
        vec![
            vec![1, 2], // 0 → 1, 0 → 2
            vec![0],    // 1 → 0
            vec![3],    // 2 → 3
            vec![2],    // 3 → 2
        ],
        0,
    );
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should compute SCCs");

    assert_eq!(result.sccs.len(), 2, "Two SCCs");
    // Sort by ID to check sizes
    let mut sizes: Vec<usize> = result.sccs.iter().map(|s| s.nodes.len()).collect();
    sizes.sort();
    assert_eq!(sizes, vec![2, 2], "Each SCC has 2 nodes");
}

#[test]
fn test_empty_graph() {
    let ss = MockStateSpace::new(vec![], 0);
    let engine = SCCEngine;
    let result = engine
        .compute_sccs(&ss, &make_context())
        .expect("Should handle empty graph");

    assert_eq!(result.sccs.len(), 0, "No SCCs in empty graph");
    assert_eq!(result.total_nodes, 0);
}

// =============================================================================
// SCCDAG Tests
// =============================================================================

#[test]
fn test_dag_terminal_sccs_cycle() {
    let ss = MockStateSpace::new(vec![vec![1], vec![2], vec![0]], 0);
    let engine = SCCEngine;
    let result = engine.compute_sccs(&ss, &make_context()).unwrap();
    let dag = SCCDAG::analyze(&result, &ss);

    assert_eq!(dag.terminal_sccs.len(), 1, "One terminal SCC for cycle");
    assert!(!dag.has_deadlocks, "No deadlock (non-trivial terminal SCC)");
    assert!(dag.has_livelocks, "Livelock in non-trivial terminal SCC");
}

#[test]
fn test_dag_terminal_sccs_isolated() {
    let ss = MockStateSpace::new(vec![vec![], vec![]], 0);
    let engine = SCCEngine;
    let result = engine.compute_sccs(&ss, &make_context()).unwrap();
    let dag = SCCDAG::analyze(&result, &ss);

    assert_eq!(dag.terminal_sccs.len(), 2, "Both nodes are terminal");
    assert!(dag.has_deadlocks, "Trivial terminal SCCs are deadlocks");
    assert!(!dag.has_livelocks, "No livelocks (only trivial)");
}

#[test]
fn test_dag_reversibility() {
    // 0 → 1 → 2 → 0 (cycle starting at 0) → reversible
    let ss = MockStateSpace::new(vec![vec![1], vec![2], vec![0]], 0);
    let engine = SCCEngine;
    let result = engine.compute_sccs(&ss, &make_context()).unwrap();
    let dag = SCCDAG::analyze(&result, &ss);

    assert!(dag.is_reversible, "Cycle should be reversible");
    assert!(!dag.home_states.is_empty(), "Home states should exist");
}

#[test]
fn test_dag_not_reversible() {
    // 0 → 1 → 2 (no back-edge) → not reversible
    let ss = MockStateSpace::new(vec![vec![1], vec![2], vec![]], 0);
    let engine = SCCEngine;
    let result = engine.compute_sccs(&ss, &make_context()).unwrap();
    let dag = SCCDAG::analyze(&result, &ss);

    assert!(!dag.is_reversible, "Acyclic path should not be reversible");
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_cancellation() {
    let ss = MockStateSpace::new(vec![vec![], vec![]], 0);
    let ctx = make_context();
    ctx.cancellation.cancel();
    let engine = SCCEngine;
    let result = engine.compute_sccs(&ss, &ctx);

    match result {
        Err(SCCError::Cancelled) => {} // expected
        _ => panic!("Should return Cancelled error"),
    }
}

#[test]
fn test_scc_error_display() {
    let err = SCCError::Cancelled;
    assert_eq!(err.to_string(), "SCC computation cancelled");
    let err2 = SCCError::MemoryLimitExceeded;
    assert_eq!(err2.to_string(), "SCC computation exceeded memory budget");
}
