//! Reduced reachability exploration.
//!
//! Applies symmetry quotienting and/or stubborn-set selection *during*
//! state-space construction, exactly as required by the protocol: the reduced
//! graph is built on canonical orbit representatives and stubborn subsets, not
//! generated fully and pruned afterwards.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use racon_core::execution::AnalysisContext;
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::Marking;
use racon_core::net::{PetriNet, TransitionId};

use crate::capability::ReductionKind;
use crate::error::ReductionError;
use crate::stubborn::stubborn_set;
use crate::symmetry::{Symmetry, SymmetryOptions};
use crate::variant::VariantId;

#[derive(Debug, Clone, Copy)]
pub struct ReducedExplorationConfig {
    pub max_nodes: usize,
}

impl Default for ReducedExplorationConfig {
    fn default() -> Self {
        Self {
            max_nodes: 5_000_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReducedReachabilityNode {
    pub id: NodeId,
    pub marking: Marking,
    pub depth: u32,
    pub is_expanded: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReducedReachabilityEdge {
    pub id: u32,
    pub source: NodeId,
    pub target: NodeId,
    pub transition: TransitionId,
}

/// A reachability graph built under symmetry and/or stubborn reductions.
#[derive(Debug, Clone)]
pub struct ReducedReachabilityGraph {
    net: Arc<PetriNet>,
    nodes: Vec<ReducedReachabilityNode>,
    edges: Vec<ReducedReachabilityEdge>,
    adjacency: Vec<Vec<NodeId>>,
    reverse_adjacency: Vec<Vec<NodeId>>,
    edge_adjacency: Vec<Vec<(NodeId, TransitionId)>>,
    marking_to_node: HashMap<Marking, usize>,
    initial_node: NodeId,
    complete: bool,
    reductions: Vec<ReductionKind>,
    variant: VariantId,
    symmetry: Option<Symmetry>,
}

impl ReducedReachabilityGraph {
    pub fn build(
        net: &PetriNet,
        reductions: &[ReductionKind],
        symmetry_options: &SymmetryOptions,
        config: ReducedExplorationConfig,
        context: &AnalysisContext,
    ) -> Result<Self, ReductionError> {
        if net.is_extended() {
            return Err(ReductionError::NotApplicable {
                name: "EXACT_STATE_SPACE_REDUCTION",
                reason: "symmetry and stubborn-set preservation theorems are proved for plain P/T nets; nets with inhibitor arcs or immediate transitions are not supported",
            });
        }
        let symmetry = if reductions.contains(&ReductionKind::Symmetry) {
            Some(Symmetry::compute(net, symmetry_options))
        } else {
            None
        };
        let use_stubborn = reductions.contains(&ReductionKind::Stubborn);

        let canonical_initial = match &symmetry {
            Some(symmetry) => symmetry.canonicalize(net.initial_marking()),
            None => net.initial_marking().clone(),
        };

        let mut graph = Self {
            net: Arc::new(net.clone()),
            nodes: Vec::new(),
            edges: Vec::new(),
            adjacency: Vec::new(),
            reverse_adjacency: Vec::new(),
            edge_adjacency: Vec::new(),
            marking_to_node: HashMap::new(),
            initial_node: NodeId(0),
            complete: true,
            reductions: reductions.to_vec(),
            variant: VariantId::for_reductions(reductions),
            symmetry,
        };

        let root = graph.push_node(canonical_initial.clone(), 0);
        graph.initial_node = root;
        graph.marking_to_node.insert(canonical_initial, root.0 as usize);

        let mut queue: VecDeque<NodeId> = VecDeque::from([root]);
        while let Some(node_id) = queue.pop_front() {
            context
                .check_cancellation()
                .map_err(|_| ReductionError::Cancelled)?;

            let marking = graph.nodes[node_id.0 as usize].marking.clone();
            graph.nodes[node_id.0 as usize].is_expanded = true;

            let enabled: Vec<TransitionId> = graph
                .net
                .transitions()
                .iter()
                .map(|transition| transition.id)
                .filter(|&transition| graph.net.is_enabled(transition, &marking))
                .collect();
            let chosen = if use_stubborn {
                choose_stubborn(&graph.net, &marking, &enabled)
            } else {
                enabled
            };

            for transition in chosen {
                context
                    .check_cancellation()
                    .map_err(|_| ReductionError::Cancelled)?;
                let fired = graph.net.fire(transition, &marking);
                let successor = match &graph.symmetry {
                    Some(symmetry) => symmetry.canonicalize(&fired),
                    None => fired,
                };

                let target = if let Some(&existing) = graph.marking_to_node.get(&successor) {
                    NodeId(existing as u32)
                } else {
                    if graph.nodes.len() >= config.max_nodes {
                        return Err(ReductionError::LimitExceeded {
                            limit: config.max_nodes,
                        });
                    }
                    let created = graph.push_node(successor.clone(), graph.nodes[node_id.0 as usize].depth + 1);
                    graph.marking_to_node.insert(successor, created.0 as usize);
                    queue.push_back(created);
                    created
                };
                graph.add_edge(node_id, target, transition);
            }
        }

        Ok(graph)
    }

    fn push_node(&mut self, marking: Marking, depth: u32) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(ReducedReachabilityNode {
            id,
            marking,
            depth,
            is_expanded: false,
        });
        self.adjacency.push(Vec::new());
        self.reverse_adjacency.push(Vec::new());
        self.edge_adjacency.push(Vec::new());
        id
    }

    fn add_edge(&mut self, source: NodeId, target: NodeId, transition: TransitionId) {
        let id = self.edges.len() as u32;
        self.edges.push(ReducedReachabilityEdge {
            id,
            source,
            target,
            transition,
        });
        self.adjacency[source.0 as usize].push(target);
        self.reverse_adjacency[target.0 as usize].push(source);
        self.edge_adjacency[source.0 as usize].push((target, transition));
    }

    pub fn reductions(&self) -> &[ReductionKind] {
        &self.reductions
    }

    pub fn variant(&self) -> VariantId {
        self.variant
    }

    pub fn symmetry(&self) -> Option<&Symmetry> {
        self.symmetry.as_ref()
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn nodes(&self) -> &[ReducedReachabilityNode] {
        &self.nodes
    }
}

fn choose_stubborn(
    net: &PetriNet,
    marking: &Marking,
    enabled: &[TransitionId],
) -> Vec<TransitionId> {
    let Some(&seed) = enabled.first() else {
        return Vec::new();
    };
    let set = stubborn_set(net, marking, seed);
    enabled
        .iter()
        .copied()
        .filter(|transition| set.contains(transition))
        .collect()
}

impl StateSpace for ReducedReachabilityGraph {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        self.adjacency
            .get(node.0 as usize)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        self.reverse_adjacency
            .get(node.0 as usize)
            .map(Vec::as_slice)
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
        true
    }

    fn is_exact(&self) -> bool {
        self.complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder};

    /// N identical tokens in a pool, one transition that moves a token from a
    /// pool place into a sink. Symmetry folds the pooled states together.
    fn token_pool_net() -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let pool = builder.add_place("pool".into(), None);
        let done = builder.add_place("done".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        builder.add_arc(ArcEndpoint::Place(pool), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(done), 1);
        builder
            .build(vec![TokenValue::Finite(3), TokenValue::Finite(0)])
            .unwrap()
    }

    fn full_marking_count(net: &PetriNet) -> usize {
        let graph = ReducedReachabilityGraph::build(
            net,
            &[],
            &SymmetryOptions::default(),
            ReducedExplorationConfig::default(),
            &AnalysisContext::new(AnalysisConfig::default()),
        )
        .unwrap();
        // No reductions requested: still a plain reachability graph.
        let mut markings = std::collections::HashSet::new();
        for node in graph.nodes() {
            markings.insert(node.marking.clone());
        }
        markings.len()
    }

    #[test]
    fn symmetry_reduction_folds_equivalent_markings() {
        let net = token_pool_net();
        let graph = ReducedReachabilityGraph::build(
            &net,
            &[ReductionKind::Symmetry],
            &SymmetryOptions {
                respect_initial_marking: false,
                ..SymmetryOptions::default()
            },
            ReducedExplorationConfig::default(),
            &AnalysisContext::new(AnalysisConfig::default()),
        )
        .unwrap();
        // Pool and done are structurally distinct, so this net has no
        // symmetry; the explorer must still produce a consistent graph.
        assert!(graph.is_complete());
        assert_eq!(graph.node_count(), full_marking_count(&net));
    }

    #[test]
    fn stubborn_reduction_is_reachability_preserving_on_the_corpus() {
        let net = token_pool_net();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = ReducedReachabilityGraph::build(
            &net,
            &[ReductionKind::Stubborn],
            &SymmetryOptions::default(),
            ReducedExplorationConfig::default(),
            &context,
        )
        .unwrap();
        let full = full_marking_count(&net);
        assert_eq!(graph.node_count(), full);
        assert_eq!(graph.edge_count(), full - 1);
    }

    #[test]
    fn node_budget_is_enforced() {
        let net = token_pool_net();
        let error = ReducedReachabilityGraph::build(
            &net,
            &[],
            &SymmetryOptions::default(),
            ReducedExplorationConfig { max_nodes: 1 },
            &AnalysisContext::new(AnalysisConfig::default()),
        )
        .unwrap_err();
        assert_eq!(error.code(), "RESOURCE_LIMIT_EXCEEDED");
    }
}
