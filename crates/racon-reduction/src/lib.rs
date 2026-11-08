//! RACoN reduction engines.
//!
//! Structural simplification plus the protocol reductions: symmetry quotienting,
//! stubborn sets and Finkel minimal coverability. Every reduction declares its
//! preconditions and the properties it preserves before it is applied.

pub mod bdd;
pub mod bounded_search;
pub mod capability;
pub mod catalog;
pub mod dead_transitions;
pub mod decision_diagrams;
pub mod error;
pub mod explorer;
pub mod goal_search;
pub mod planner;
pub mod polyhedral;
pub mod query_rules;
pub mod rules;
pub mod stubborn;
pub mod symmetry;
pub mod thierry_mieg;
pub mod unfolding;
pub mod variant;

pub use bounded_search::{
    decide_boundedness_stubborn, decide_boundedness_stubborn_omega, BoundSearchConfig,
    BoundSearchReport, BoundTarget, BoundVerdict, OmegaSearchReport, OmegaVerdict,
};
pub use capability::{capability, ReductionCapability, ReductionKind, CAPABILITIES};
pub use dead_transitions::{
    decide_dead_transition, DeadSearchConfig, DeadSearchReport, DeadVerdict,
};
pub use error::ReductionError;
pub use explorer::{
    ReducedExplorationConfig, ReducedReachabilityEdge, ReducedReachabilityGraph,
    ReducedReachabilityNode,
};
pub use goal_search::{
    search_goal, GoalOp, GoalSearchConfig, GoalSearchOutcome, GoalSearchReport, LinearConstraint,
    ReachGoal,
};
pub use planner::{plan, PlannedReduction, ReductionPlan};
pub use query_rules::{
    goal_places, reduce_for_query, QueryKind, QueryReduction, RuleCount,
};
pub use stubborn::stubborn_set;
pub use symmetry::{
    compute_symmetries, transposition_generator_pairs, Symmetry, SymmetryGenerator,
    SymmetryGeneratorKind, SymmetryOptions, SymmetryReport,
};
pub use thierry_mieg::{
    reduce as reduce_thierry_mieg, Agglomeration, ReductionGoal, ThierryMiegConfig,
    ThierryMiegResult,
};
pub use variant::VariantId;

use racon_core::net::PetriNet;

/// Result of applying structural reduction rules to a net.
#[derive(Debug, Clone)]
pub struct ReductionResult {
    pub reduced_net: PetriNet,
    pub report: ReductionReport,
}

/// Summary of what the reducer did.
#[derive(Debug, Clone, Default)]
pub struct ReductionReport {
    pub rules_applied: Vec<AppliedRule>,
    pub total_places_removed: usize,
    pub total_transitions_removed: usize,
    pub total_arcs_removed: usize,
    pub iterations: usize,
    /// Structural reducer never claims behavioral equivalence without a
    /// dedicated proof/certificate for the original net.
    pub behavioral_properties_certified: bool,
    pub preservation_scope: &'static str,
}

/// One application of a reduction rule.
#[derive(Debug, Clone)]
pub struct AppliedRule {
    pub name: &'static str,
    pub count: usize,
}

/// Applies structural reduction rules to a Petri net.
pub struct StructuralReducer;

impl StructuralReducer {
    /// Reduce the net by applying all rules in a fixed-point loop.
    /// Stops when no rule makes progress.
    pub fn reduce(net: &PetriNet) -> ReductionResult {
        if net.is_extended() {
            // The rewrite rules rebuild the net from ordinary arcs only and
            // would silently drop inhibitor arcs and transition kinds.
            return ReductionResult {
                reduced_net: net.clone(),
                report: ReductionReport {
                    behavioral_properties_certified: false,
                    preservation_scope: "unsupported_extended_net_unchanged",
                    ..ReductionReport::default()
                },
            };
        }
        let mut current = net.clone();
        let mut report = ReductionReport::default();
        report.preservation_scope = "structural_simplification_only";

        loop {
            let places_before = current.places().len();
            let trans_before = current.transitions().len();
            let arcs_before = current.arcs().len();

            let (new, n_sink) = rules::eliminate_sink_places(&current);
            current = new;
            let n_source = {
                let (new, n) = rules::eliminate_source_places(&current);
                current = new;
                n
            };
            let n_self = {
                let (new, n) = rules::remove_self_loops(&current);
                current = new;
                n
            };
            let n_dup = {
                let (new, n) = rules::remove_duplicates(&current);
                current = new;
                n
            };

            let total_removed = n_sink + n_source + n_self + n_dup;
            if total_removed == 0 {
                break;
            }

            let places_removed = places_before - current.places().len();
            let trans_removed = trans_before - current.transitions().len();
            let arcs_removed = arcs_before - current.arcs().len();

            if n_sink > 0 {
                report.rules_applied.push(AppliedRule {
                    name: "eliminate_sink_places",
                    count: n_sink,
                });
            }
            if n_source > 0 {
                report.rules_applied.push(AppliedRule {
                    name: "eliminate_source_places",
                    count: n_source,
                });
            }
            if n_self > 0 {
                report.rules_applied.push(AppliedRule {
                    name: "remove_self_loops",
                    count: n_self,
                });
            }
            if n_dup > 0 {
                report.rules_applied.push(AppliedRule {
                    name: "remove_duplicates",
                    count: n_dup,
                });
            }
            report.total_places_removed += places_removed;
            report.total_transitions_removed += trans_removed;
            report.total_arcs_removed += arcs_removed;
            report.iterations += 1;
        }

        ReductionResult {
            reduced_net: current,
            report,
        }
    }
}
