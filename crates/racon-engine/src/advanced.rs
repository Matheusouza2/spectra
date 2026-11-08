//! Backend methods that have no exploration-session lifecycle: symbolic state
//! spaces (MDD saturation, BDD), unfolding prefix, structural / polyhedral
//! reductions and the CovProc / Monotone-Pruning coverability sets.
//!
//! One entry point, [`run_advanced`], is shared by the HTTP API, the Tauri
//! shell and (through the HTTP server) the JNI binding, so every front end
//! sees the same result shape: `{ algorithm, reference_ids, experimental,
//! summary, does_not_preserve, elapsed_ms }`.

use std::time::Instant;

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::net::PetriNet;
use racon_coverability::{
    CoverabilityError, CovProc, CovProcConfig, MinimalCoverabilitySet, MonotonePruning,
    MonotonePruningConfig, OmegaValue,
};
use racon_reduction::bdd::{self, BddConfig};
use racon_reduction::catalog::algorithm as catalog_algorithm;
use racon_reduction::decision_diagrams::{self as mdd, DdConfig};
use racon_reduction::polyhedral;
use racon_reduction::thierry_mieg::{self, ReductionGoal, ThierryMiegConfig};
use racon_reduction::unfolding::{unfold, AdequateOrder, UnfoldingConfig};
use racon_reduction::ReductionError;
use serde::Deserialize;
use serde_json::{json, Value};

/// Identifiers accepted by [`run_advanced`].
pub const ADVANCED_ALGORITHMS: &[&str] = &[
    "symbolic_mdd",
    "symbolic_bdd",
    "unfolding_prefix",
    "structural_rules",
    "polyhedral_abstraction",
    "coverability_grvb",
    "coverability_mp",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AdvancedParams {
    /// Resource budget (MDD/BDD nodes, unfolding events, tree nodes, pairs).
    pub limit: Option<usize>,
    /// `structural_rules`: `"deadlock"` (default) or `"safety"`.
    pub goal: Option<String>,
    /// `structural_rules` with goal `safety`: names of the places observed.
    pub support: Vec<String>,
    /// `unfolding_prefix`: `"erv"` (default) or `"mcmillan"`.
    pub order: Option<String>,
    /// Markings listed in the result at most (default 64).
    pub list_cap: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedError {
    UnknownAlgorithm(String),
    BadRequest(String),
    /// The method's preconditions do not hold for this net.
    Unsupported(String),
    /// A budget was exhausted: the outcome is UNKNOWN, never a verdict.
    LimitExceeded(String),
    Internal(String),
}

impl AdvancedError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownAlgorithm(_) => "UNKNOWN_ALGORITHM",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Unsupported(_) => "UNSUPPORTED",
            Self::LimitExceeded(_) => "RESOURCE_LIMIT_EXCEEDED",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            Self::UnknownAlgorithm(m)
            | Self::BadRequest(m)
            | Self::Unsupported(m)
            | Self::LimitExceeded(m)
            | Self::Internal(m) => m,
        }
    }
}

impl std::fmt::Display for AdvancedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}
impl std::error::Error for AdvancedError {}

impl From<ReductionError> for AdvancedError {
    fn from(error: ReductionError) -> Self {
        match error {
            ReductionError::LimitExceeded { .. } => Self::LimitExceeded(format!("UNKNOWN: {error}")),
            ReductionError::Cancelled => Self::Internal(error.to_string()),
            other => Self::Unsupported(other.to_string()),
        }
    }
}

impl From<CoverabilityError> for AdvancedError {
    fn from(error: CoverabilityError) -> Self {
        match error {
            CoverabilityError::Unsupported(m) => Self::Unsupported(m),
            other => Self::LimitExceeded(format!("UNKNOWN: {other}")),
        }
    }
}

fn marking_json(m: &[u64]) -> Value {
    json!(m)
}

fn set_json(net: &PetriNet, set: &MinimalCoverabilitySet) -> Value {
    let names: Vec<&str> = net.places().iter().map(|p| p.name.as_str()).collect();
    json!({
        "place_names": names,
        "bounded": set.is_bounded(),
        "unbounded_places": set.unbounded_places().into_iter().map(|i| names[i]).collect::<Vec<_>>(),
        "elements": set.elements().iter().map(|m| m.iter().map(|v| match v {
            OmegaValue::Finite(n) => json!(n),
            OmegaValue::Omega => json!("omega"),
        }).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "tree_vertices": set.tree_vertices,
        "accelerations": set.accelerations,
        "iterations": set.iterations,
    })
}

fn place_names(net: &PetriNet) -> Vec<String> {
    net.places().iter().map(|p| p.name.clone()).collect()
}

/// Runs one of [`ADVANCED_ALGORITHMS`] on `net`. Blocking and possibly long:
/// callers on an async runtime must use a blocking task.
pub fn run_advanced(net: &PetriNet, algorithm: &str, params: &AdvancedParams) -> Result<Value, AdvancedError> {
    let entry = ADVANCED_ALGORITHMS
        .iter()
        .find(|id| **id == algorithm)
        .map(|id| catalog_algorithm(catalog_id(id)).expect("catalogued"))
        .ok_or_else(|| AdvancedError::UnknownAlgorithm(format!("'{algorithm}' is not an advanced algorithm")))?;
    let started = Instant::now();
    let cap = params.list_cap.unwrap_or(64);
    let summary = match algorithm {
        "symbolic_mdd" => {
            let config = params.limit.map_or_else(DdConfig::default, |max_nodes| DdConfig { max_nodes });
            let space = mdd::generate(net, &config)?;
            let states = space.states(cap);
            json!({
                "encoding": "MDD (um nível por lugar) + saturação",
                "place_names": place_names(net),
                "state_count": space.state_count().map(|n| n.to_string()),
                "diagram_nodes": space.node_count(),
                "allocated_nodes": space.allocated_nodes,
                "place_bounds": space.place_bounds(),
                "states_listed": states.is_some(),
                "states": states.map(|s| s.iter().map(|m| marking_json(m)).collect::<Vec<_>>()).unwrap_or_default(),
            })
        }
        "symbolic_bdd" => {
            let config = params.limit.map_or_else(BddConfig::default, |max_nodes| BddConfig { max_nodes });
            let mut space = bdd::generate(net, &config)?;
            let states = space.states(cap);
            json!({
                "encoding": "BDD ordenado reduzido (uma variável por lugar), rede 1-safe",
                "place_names": place_names(net),
                "state_count": space.state_count().map(|n| n.to_string()),
                "diagram_nodes": space.node_count(),
                "allocated_nodes": space.allocated_nodes(),
                "iterations": space.iterations,
                "place_bounds": space.place_bounds(),
                "states_listed": states.is_some(),
                "states": states.map(|s| s.iter().map(|m| marking_json(m)).collect::<Vec<_>>()).unwrap_or_default(),
            })
        }
        "unfolding_prefix" => {
            let order = match params.order.as_deref() {
                None | Some("erv") => AdequateOrder::Erv,
                Some("mcmillan") => AdequateOrder::McMillan,
                Some(other) => return Err(AdvancedError::BadRequest(format!("unknown order '{other}'"))),
            };
            let config = UnfoldingConfig {
                order,
                max_events: params.limit.unwrap_or(UnfoldingConfig::default().max_events),
            };
            let prefix = unfold(net, &config)?;
            let transitions = net.transitions();
            let markings = prefix.reachable_markings(100_000);
            let deadlock = prefix.find_deadlock(100_000);
            json!({
                "order": if order == AdequateOrder::Erv { "erv" } else { "mcmillan" },
                "conditions": prefix.conditions.len(),
                "events": prefix.event_count(),
                "cutoff_events": prefix.cutoff_count(),
                "dead_transitions": prefix.dead_transitions().iter()
                    .map(|t| transitions[t.0 as usize].name.clone()).collect::<Vec<_>>(),
                "reachable_markings": markings.as_ref().map(|m| m.len()),
                "has_deadlock": deadlock.as_ref().map(Option::is_some),
                "deadlock_marking": deadlock.flatten(),
                "place_names": place_names(net),
            })
        }
        "structural_rules" => {
            let goal = match params.goal.as_deref() {
                None | Some("deadlock") => ReductionGoal::Deadlock,
                Some("safety") => {
                    let mut support = Vec::new();
                    for name in &params.support {
                        let place = net.places().iter().find(|p| &p.name == name).ok_or_else(|| {
                            AdvancedError::BadRequest(format!("support place '{name}' does not exist"))
                        })?;
                        support.push(place.id);
                    }
                    if support.is_empty() {
                        return Err(AdvancedError::BadRequest("goal 'safety' needs at least one support place".into()));
                    }
                    ReductionGoal::Safety { support }
                }
                Some(other) => return Err(AdvancedError::BadRequest(format!("unknown goal '{other}'"))),
            };
            let result = thierry_mieg::reduce(net, &goal, &ThierryMiegConfig::default())?;
            json!({
                "goal": params.goal.clone().unwrap_or_else(|| "deadlock".into()),
                "original_places": net.place_count(),
                "original_transitions": net.transitions().len(),
                "remaining_places": result.remaining_places,
                "remaining_transitions": result.remaining_transitions,
                "initial_marking": result.initial_marking,
                "empty_net": result.reduced_net.is_none(),
                "deadlock_free": result.deadlock_free,
                "total_applications": result.total_applications(),
                "applications_per_rule": result.applications.iter()
                    .map(|(rule, n)| (rule.to_string(), json!(n))).collect::<serde_json::Map<_, _>>(),
                "agglomerations": result.agglomerations.iter().map(|a| json!({
                    "rule": a.rule, "place": a.place, "new_transitions": a.new_transitions,
                })).collect::<Vec<_>>(),
                "rounds": result.rounds,
            })
        }
        "polyhedral_abstraction" => {
            let reduction = polyhedral::reduce(net)?;
            json!({
                "original_places": reduction.original_place_count(),
                "remaining_places": reduction.remaining_places(),
                "remaining_transitions": reduction.remaining_transitions(),
                "initial_marking": reduction.initial_marking(),
                "equations": reduction.equation_strings(),
                "applications_per_rule": reduction.applications.iter()
                    .map(|(rule, n)| (rule.to_string(), json!(n))).collect::<serde_json::Map<_, _>>(),
                "empty_net": reduction.reduced_net().is_none(),
            })
        }
        "coverability_grvb" => {
            let defaults = CovProcConfig::default();
            let config = CovProcConfig { max_pairs: params.limit.unwrap_or(defaults.max_pairs), ..defaults };
            let context = AnalysisContext::new(AnalysisConfig::default());
            set_json(net, &CovProc::new(config).compute(net, &context)?)
        }
        "coverability_mp" => {
            let defaults = MonotonePruningConfig::default();
            let config = MonotonePruningConfig { max_nodes: params.limit.unwrap_or(defaults.max_nodes), ..defaults };
            let context = AnalysisContext::new(AnalysisConfig::default());
            set_json(net, &MonotonePruning::new(config).compute(net, &context)?)
        }
        _ => unreachable!("checked against ADVANCED_ALGORITHMS"),
    };
    Ok(json!({
        "algorithm": algorithm,
        "name": entry.name,
        "reference_ids": entry.reference_ids,
        "experimental": true,
        "summary": summary,
        "when_not_to_use": entry.when_not_to_use,
        "does_not_preserve": entry.does_not_preserve,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    }))
}

/// Catalogue entry that documents a given advanced method.
fn catalog_id(id: &str) -> &'static str {
    match id {
        "symbolic_mdd" | "symbolic_bdd" => "decision_diagrams",
        "unfolding_prefix" => "unfolding_prefix",
        "structural_rules" => "structural_rules",
        "polyhedral_abstraction" => "polyhedral_abstraction",
        "coverability_grvb" => "coverability_grvb",
        _ => "coverability_mp",
    }
}
