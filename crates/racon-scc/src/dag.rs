use racon_core::graph::{NodeId, StateSpace};
use std::collections::HashMap;

/// Identifies a strongly connected component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SCCId(pub u32);

/// A single strongly connected component.
#[derive(Debug, Clone)]
pub struct SCC {
    pub id: SCCId,
    pub nodes: Vec<NodeId>,
    pub is_trivial: bool,
    pub is_terminal: bool,
    pub has_self_loop: bool,
}

/// Result of Tarjan SCC decomposition.
#[derive(Debug, Clone)]
pub struct SCCResult {
    pub node_to_scc: HashMap<NodeId, SCCId>,
    pub sccs: Vec<SCC>,
    pub total_nodes: usize,
}

/// High-level analysis of the SCC condensation DAG.
#[derive(Debug, Clone)]
pub struct SCCDAGAnalysis {
    pub terminal_sccs: Vec<SCCId>,
    pub is_reversible: bool,
    pub has_deadlocks: bool,
    pub has_livelocks: bool,
    pub home_states: Vec<NodeId>,
}

/// DAG-level analysis over the SCC decomposition of a state space.
pub struct SCCDAG;

impl SCCDAG {
    /// Analyze the condensation DAG: find terminal SCCs, check reversibility,
    /// detect deadlocks and livelocks.
    pub fn analyze(scc_result: &SCCResult, state_space: &dyn StateSpace) -> SCCDAGAnalysis {
        let mut scc_has_outgoing: HashMap<SCCId, bool> = HashMap::new();

        for scc in &scc_result.sccs {
            scc_has_outgoing.insert(scc.id, false);
        }

        // Build outgoing edges between SCCs
        for scc in &scc_result.sccs {
            for &u in &scc.nodes {
                for &v in state_space.successors(u) {
                    let scc_v = scc_result.node_to_scc[&v];
                    if scc_v != scc.id {
                        scc_has_outgoing.insert(scc.id, true);
                    }
                }
            }
        }

        let mut terminal_sccs = Vec::new();
        let mut has_deadlocks = false;
        let mut has_livelocks = false;

        for scc in &scc_result.sccs {
            let is_terminal = !scc_has_outgoing.get(&scc.id).unwrap_or(&false);
            if is_terminal {
                terminal_sccs.push(scc.id);
                if scc.is_trivial {
                    has_deadlocks = true;
                } else {
                    has_livelocks = true;
                }
            }
        }

        let initial_scc = scc_result.node_to_scc[&state_space.initial_node()];
        let is_reversible = terminal_sccs.len() == 1 && terminal_sccs[0] == initial_scc;

        let home_states = if is_reversible {
            scc_result
                .sccs
                .iter()
                .find(|s| s.id == initial_scc)
                .map(|s| s.nodes.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        SCCDAGAnalysis {
            terminal_sccs,
            is_reversible,
            has_deadlocks,
            has_livelocks,
            home_states,
        }
    }
}
