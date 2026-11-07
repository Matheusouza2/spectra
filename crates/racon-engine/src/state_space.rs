use racon_core::graph::StateSpace;
use racon_coverability::CoverabilityGraph;
use racon_reachability::ReachabilityGraph;
use racon_reduction::{ReducedReachabilityGraph, ReductionKind};

/// A completed state-space exploration retained by an analysis session.
///
/// Kept as an owned, cloneable handle so behavioral properties can be queried
/// after the asynchronous job finishes (spec §13, §27).
#[derive(Debug, Clone)]
pub enum StateSpaceHandle {
    Reachability(ReachabilityGraph),
    /// Same exact graph as `Reachability`, discovered depth-first.
    DepthFirstReachability(ReachabilityGraph),
    Coverability(CoverabilityGraph),
    /// Reachability graph built under symmetry/stubborn reductions (§33, §34).
    ReducedReachability(ReducedReachabilityGraph),
    /// Coverability tree built with Finkel's minimal-coverability pruning (§35).
    MinimalCoverability(CoverabilityGraph),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateSpaceCounts {
    pub states_discovered: usize,
    pub states_processed: usize,
    pub edges_discovered: usize,
    pub frontier_size: usize,
    pub omega_nodes: usize,
}

impl StateSpaceHandle {
    pub fn as_state_space(&self) -> &dyn StateSpace {
        match self {
            Self::Reachability(graph) | Self::DepthFirstReachability(graph) => graph,
            Self::Coverability(graph) => graph,
            Self::ReducedReachability(graph) => graph,
            Self::MinimalCoverability(graph) => graph,
        }
    }

    pub fn is_exact(&self) -> bool {
        self.as_state_space().is_exact()
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Reachability(_) | Self::DepthFirstReachability(_) => "REACHABILITY",
            Self::Coverability(_) => "COVERABILITY",
            Self::ReducedReachability(_) => "REACHABILITY",
            Self::MinimalCoverability(_) => "COVERABILITY",
        }
    }

    pub fn algorithm(&self) -> &'static str {
        match self {
            Self::Reachability(_) => "BREADTH_FIRST_SEARCH",
            Self::DepthFirstReachability(_) => "DEPTH_FIRST_SEARCH",
            Self::Coverability(_) => "KARP_MILLER",
            Self::ReducedReachability(graph) => reduced_algorithm(graph.reductions()),
            Self::MinimalCoverability(_) => "FINKEL_MINIMAL_COVERABILITY",
        }
    }

    pub fn counts(&self) -> StateSpaceCounts {
        match self {
            Self::Reachability(graph) | Self::DepthFirstReachability(graph) => StateSpaceCounts {
                states_discovered: graph.nodes.len(),
                states_processed: graph.nodes.iter().filter(|node| node.is_expanded).count(),
                edges_discovered: graph.edges.len(),
                frontier_size: graph.nodes.iter().filter(|node| !node.is_expanded).count(),
                omega_nodes: 0,
            },
            Self::Coverability(graph) => StateSpaceCounts {
                states_discovered: graph.nodes.len(),
                states_processed: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && node.is_expanded)
                    .count(),
                edges_discovered: graph.edges.len(),
                frontier_size: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && !node.is_expanded)
                    .count(),
                omega_nodes: graph.stats.omega_nodes,
            },
            Self::ReducedReachability(graph) => {
                let nodes = graph.nodes();
                StateSpaceCounts {
                    states_discovered: nodes.len(),
                    states_processed: nodes.iter().filter(|node| node.is_expanded).count(),
                    edges_discovered: graph.edge_count(),
                    frontier_size: nodes.iter().filter(|node| !node.is_expanded).count(),
                    omega_nodes: 0,
                }
            }
            Self::MinimalCoverability(graph) => StateSpaceCounts {
                states_discovered: graph.nodes.len(),
                states_processed: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && node.is_expanded)
                    .count(),
                edges_discovered: graph.edges.len(),
                frontier_size: graph
                    .nodes
                    .iter()
                    .filter(|node| node.is_active && !node.is_expanded)
                    .count(),
                omega_nodes: graph.stats.omega_nodes,
            },
        }
    }

    pub fn omega_nodes(&self) -> usize {
        match self {
            Self::Reachability(_) | Self::DepthFirstReachability(_) => 0,
            Self::Coverability(graph) => graph.stats.omega_nodes,
            Self::ReducedReachability(_) => 0,
            Self::MinimalCoverability(graph) => graph.stats.omega_nodes,
        }
    }

    /// Whether the construction gives a definitive boundedness answer.
    ///
    /// `None` is a genuine "unknown" and must never be coerced to `false`.
    pub fn bounded_status(&self) -> Option<bool> {
        match self {
            Self::Reachability(graph) | Self::DepthFirstReachability(graph) => {
                graph.complete.then_some(true)
            }
            Self::Coverability(graph) => graph.bounded_status(),
            Self::ReducedReachability(graph) => graph.is_complete().then_some(true),
            Self::MinimalCoverability(graph) => graph.bounded_status(),
        }
    }

    pub fn completed(&self) -> bool {
        match self {
            Self::Reachability(graph) | Self::DepthFirstReachability(graph) => graph.complete,
            Self::Coverability(_) => true,
            Self::ReducedReachability(graph) => graph.is_complete(),
            Self::MinimalCoverability(_) => true,
        }
    }

    /// Whether the exploration applied any state-space reduction (§31-38).
    pub fn reduced(&self) -> bool {
        matches!(
            self,
            Self::ReducedReachability(_) | Self::MinimalCoverability(_)
        )
    }

    pub fn reductions(&self) -> Vec<String> {
        match self {
            Self::ReducedReachability(graph) => graph
                .reductions()
                .iter()
                .map(|kind| kind.as_str().to_owned())
                .collect(),
            Self::MinimalCoverability(_) => {
                vec!["FINKEL_MINIMAL_COVERABILITY".to_owned()]
            }
            _ => Vec::new(),
        }
    }

    /// Variant identifier of the analyzed net (`original`, `symmetry`,
    /// `stubborn`, `symmetry-stubborn`), or `original` for exact graphs.
    pub fn variant(&self) -> &'static str {
        match self {
            Self::ReducedReachability(graph) => graph.variant().as_str(),
            _ => "original",
        }
    }
}

fn reduced_algorithm(reductions: &[ReductionKind]) -> &'static str {
    let symmetry = reductions.contains(&ReductionKind::Symmetry);
    let stubborn = reductions.contains(&ReductionKind::Stubborn);
    match (symmetry, stubborn) {
        (true, true) => "SYMMETRY_STUBBORN_REDUCED_BFS",
        (true, false) => "SYMMETRY_REDUCED_BFS",
        (false, true) => "STUBBORN_REDUCED_BFS",
        (false, false) => "BREADTH_FIRST_SEARCH",
    }
}
