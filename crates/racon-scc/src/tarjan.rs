use racon_core::execution::AnalysisContext;
use racon_core::graph::{NodeId, StateSpace};
use std::collections::HashMap;

use crate::dag::{SCCId, SCCResult, SCC};

/// Errors from SCC computation.
#[derive(Debug, Clone)]
pub enum SCCError {
    Cancelled,
    MemoryLimitExceeded,
    NonExactStateSpace,
}

impl std::fmt::Display for SCCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "SCC computation cancelled"),
            Self::MemoryLimitExceeded => write!(f, "SCC computation exceeded memory budget"),
            Self::NonExactStateSpace => {
                write!(f, "SCC computation requires an exact reachability graph")
            }
        }
    }
}

impl std::error::Error for SCCError {}

/// Computes SCCs via Tarjan's algorithm on any `StateSpace`.
pub struct SCCEngine;

impl SCCEngine {
    /// Run Tarjan's algorithm on the given state space.
    ///
    /// Returns the SCC decomposition including the mapping from each node
    /// to its SCC, the list of SCCs, and total node count.
    pub fn compute_sccs(
        &self,
        state_space: &dyn StateSpace,
        context: &AnalysisContext,
    ) -> Result<SCCResult, SCCError> {
        if !state_space.is_exact() {
            return Err(SCCError::NonExactStateSpace);
        }
        let num_nodes = state_space.node_count();
        let mut indices: HashMap<NodeId, usize> = HashMap::with_capacity(num_nodes);
        let mut lowlinks: HashMap<NodeId, usize> = HashMap::with_capacity(num_nodes);
        let mut on_stack: HashMap<NodeId, bool> = HashMap::with_capacity(num_nodes);
        let mut stack: Vec<NodeId> = Vec::with_capacity(num_nodes);
        let mut current_index = 0usize;
        let mut sccs: Vec<SCC> = Vec::new();
        let mut node_to_scc: HashMap<NodeId, SCCId> = HashMap::with_capacity(num_nodes);
        let mut scc_id_counter = 0u32;

        for i in 0..num_nodes {
            context
                .check_cancellation()
                .map_err(|_| SCCError::Cancelled)?;

            let node = NodeId(i as u32);
            if !indices.contains_key(&node) {
                Self::strongconnect(
                    node,
                    &mut current_index,
                    &mut indices,
                    &mut lowlinks,
                    &mut on_stack,
                    &mut stack,
                    &mut sccs,
                    &mut node_to_scc,
                    &mut scc_id_counter,
                    state_space,
                );
            }
        }

        Ok(SCCResult {
            node_to_scc,
            sccs,
            total_nodes: num_nodes,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn strongconnect(
        v: NodeId,
        current_index: &mut usize,
        indices: &mut HashMap<NodeId, usize>,
        lowlinks: &mut HashMap<NodeId, usize>,
        on_stack: &mut HashMap<NodeId, bool>,
        stack: &mut Vec<NodeId>,
        sccs: &mut Vec<SCC>,
        node_to_scc: &mut HashMap<NodeId, SCCId>,
        scc_id_counter: &mut u32,
        state_space: &dyn StateSpace,
    ) {
        indices.insert(v, *current_index);
        lowlinks.insert(v, *current_index);
        *current_index += 1;
        stack.push(v);
        on_stack.insert(v, true);

        for &w in state_space.successors(v) {
            if !indices.contains_key(&w) {
                Self::strongconnect(
                    w,
                    current_index,
                    indices,
                    lowlinks,
                    on_stack,
                    stack,
                    sccs,
                    node_to_scc,
                    scc_id_counter,
                    state_space,
                );
                let w_lowlink = *lowlinks.get(&w).unwrap();
                let v_lowlink = lowlinks.get_mut(&v).unwrap();
                *v_lowlink = (*v_lowlink).min(w_lowlink);
            } else if *on_stack.get(&w).unwrap_or(&false) {
                let w_index = *indices.get(&w).unwrap();
                let v_lowlink = lowlinks.get_mut(&v).unwrap();
                *v_lowlink = (*v_lowlink).min(w_index);
            }
        }

        if lowlinks.get(&v) == indices.get(&v) {
            // v is the root of an SCC — pop the stack
            let scc_id = SCCId(*scc_id_counter);
            *scc_id_counter += 1;
            let mut nodes = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.insert(w, false);
                node_to_scc.insert(w, scc_id);
                nodes.push(w);
                if w == v {
                    break;
                }
            }

            let is_trivial =
                nodes.len() == 1 && !state_space.successors(nodes[0]).contains(&nodes[0]);

            sccs.push(SCC {
                id: scc_id,
                nodes,
                is_trivial,
                is_terminal: false, /* determined in DAG analysis */
                has_self_loop: false,
            });
        }
    }
}
