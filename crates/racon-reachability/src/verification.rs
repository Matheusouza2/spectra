use crate::error::ReachabilityError;
use crate::graph::ReachabilityGraph;

impl ReachabilityGraph {
    /// Verify that all nodes are reachable from the initial node (BFS).
    pub fn verify_consistency(&self) -> Result<(), ReachabilityError> {
        let n = self.nodes.len();
        if n == 0 {
            return Ok(());
        }
        let mut visited = vec![false; n];
        let mut queue = std::collections::VecDeque::new();
        let init_idx = self.initial_node.0 as usize;
        visited[init_idx] = true;
        queue.push_back(init_idx);

        while let Some(u) = queue.pop_front() {
            for &v in &self.adjacency[u] {
                let vi = v.0 as usize;
                if !visited[vi] {
                    visited[vi] = true;
                    queue.push_back(vi);
                }
            }
        }

        if visited.iter().all(|&v| v) {
            Ok(())
        } else {
            let unreachable = visited.iter().enumerate().filter(|(_, &v)| !v).count();
            Err(ReachabilityError::VerificationFailed(format!(
                "{} nodes are not reachable from initial marking",
                unreachable
            )))
        }
    }

    /// Verify that every edge corresponds to a valid transition firing.
    pub fn verify_firing_semantics(&self) -> Result<(), ReachabilityError> {
        for edge in &self.edges {
            let src = &self.nodes[edge.source.0 as usize];
            let tgt = &self.nodes[edge.target.0 as usize];

            if !self.net.is_enabled(edge.transition, &src.marking) {
                return Err(ReachabilityError::VerificationFailed(format!(
                    "Edge {:?}: transition not enabled in source marking",
                    edge.id
                )));
            }

            let expected = self.net.fire(edge.transition, &src.marking);
            if expected != tgt.marking {
                return Err(ReachabilityError::VerificationFailed(format!(
                    "Edge {:?}: firing does not produce target marking",
                    edge.id
                )));
            }
        }
        Ok(())
    }
}
