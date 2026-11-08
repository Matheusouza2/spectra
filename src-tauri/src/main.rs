#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod graph_cache;

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::graph::NodeId;
use racon_core::marking::TokenValue;
use racon_core::net::{ArcEndpoint, PetriNet, PlaceId, TransitionId};
use racon_core::Marking;
use racon_core::StateSpace;
use racon_core::validation::validate_petri_net;
use racon_coverability::{CoverabilityConfig, CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy};
use racon_invariants::{
    compute_minimal_place_inequality_invariants, derive_structural_properties,
    state_equation_test, structural_boundedness_certificate, InequalityRelation, InvariantEngine,
    InvariantEngineImpl, p_invariant_basis, t_invariant_basis, StateEquationVerdict,
};
use racon_parsers::{MercuryParser, MercuryParserImpl, MercuryParserConfig, PntParser, PntParserImpl, PntParserConfig};
use racon_reachability::{ReachabilityConfig, ReachabilityEngine, ReachabilityEngineImpl, ReachabilityGraph};
use racon_ctl::{AtomicPredicate, CTLConfig, CTLEngine, CTLEngineImpl, CTLFormula, ComparisonOp};
use racon_ctl_dsl::{parse_ctl_query, DslParseError};
use racon_structural::topology::StructuralAnalyzer;
use racon_structural::expert::ExpertSystem;
use racon_property::{BadStateDefinition, PropertyEngine, PropertyEngineImpl, StatePredicate};
use racon_evidence::{build_all_evidence, MathematicalEvidence};
use serde::{Deserialize, Serialize};
use num_bigint::{BigInt, BigUint};
use std::collections::HashSet;
use std::sync::Arc;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Manager};

// =============================================================================
// Response types (JSON protocol — unchanged from previous version)
// =============================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenDto {
    Finite { value: String },
    Omega,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphNode {
    pub id: usize,
    pub marking: Vec<TokenDto>,
    /// True when at least one place is represented by ω in a coverability node.
    pub is_omega: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphEdge {
    pub source: usize,
    pub target: usize,
    pub transition: usize,
    #[serde(default)]
    pub is_abstract: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ReachabilityGraphResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub display_truncated: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ReachabilityGraphPageResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub offset: usize,
    pub limit: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub complete: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CoverageTreeResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub is_bounded: Option<bool>,
    pub max_states_exceeded: bool,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub display_truncated: bool,
}

#[derive(Serialize, Deserialize)]
pub struct LivenessEntry {
    pub transition: usize,
    pub name: String,
    pub level: String,
}

#[derive(Serialize, Deserialize)]
pub struct PlaceLivenessEntry {
    pub place: usize,
    pub name: String,
    pub live: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AnalysisInfo {
    pub num_states: Option<usize>,
    pub duration_ms: f64,
}

/// Exploration algorithm chosen by the user (ids of the algorithm catalogue).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphAlgorithm {
    /// Legacy behaviour: BFS for reachability, Karp-Miller for coverability.
    #[default]
    Default,
    ReachabilityBfs,
    ReachabilityDfs,
    CoverabilityKarpMiller,
    CoverabilityFinkelLegacy,
    CoverabilityMincov,
}

impl GraphAlgorithm {
    fn catalog_id(self, objective: racon_reduction::catalog::Objective) -> &'static str {
        use racon_reduction::catalog::Objective;
        match (self, objective) {
            (Self::ReachabilityBfs, _) => "reachability_bfs",
            (Self::ReachabilityDfs, _) => "reachability_dfs",
            (Self::CoverabilityKarpMiller, _) => "coverability_karp_miller",
            (Self::CoverabilityFinkelLegacy, _) => "coverability_finkel_legacy",
            (Self::CoverabilityMincov, _) => "coverability_mincov",
            (Self::Default, Objective::Reachability) => "reachability_bfs",
            (Self::Default, _) => "coverability_karp_miller",
        }
    }
}

/// Re-validates the user's choice against the capability matrix right before
/// running it (spec §2.3, §6: the core revalidates, the UI is not trusted).
fn ensure_algorithm_allowed(
    net: &PetriNet,
    objective: racon_reduction::catalog::Objective,
    algorithm: GraphAlgorithm,
    stubborn: bool,
    symmetry: bool,
) -> Result<(), String> {
    use racon_reduction::catalog::{validate, ConfigRequest, NetClass};
    let report = validate(&ConfigRequest {
        objective,
        mode: "manual".into(),
        exploration: Some(algorithm.catalog_id(objective).to_owned()),
        reductions: {
            let mut selected = Vec::new();
            if stubborn {
                selected.push("partial_order_stubborn".to_owned());
            }
            if symmetry {
                selected.push("symmetry_reduction".to_owned());
            }
            selected
        },
        net_class: if net.is_extended() { NetClass::ExtendedPt } else { NetClass::PtWeighted },
    });
    if report.is_valid() {
        Ok(())
    } else {
        Err(format!(
            "UNSUPPORTED: {}",
            report.issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join(" ")
        ))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphGenerationConfig {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub memory_limit_mb: usize,
    #[serde(default)]
    pub algorithm: GraphAlgorithm,
    /// Apply the (experimental) stubborn-set reduction during exploration.
    #[serde(default)]
    pub stubborn: bool,
    /// Explore one canonical marking per symmetry orbit (Junttila 2003).
    #[serde(default)]
    pub symmetry: bool,
    /// Apply the query-dependent structural rules A-I (Bønneland et al. 2019)
    /// before searching a Bad State.
    #[serde(default)]
    pub structural_rules: bool,
}

impl Default for GraphGenerationConfig {
    fn default() -> Self {
        Self {
            max_nodes: 1_000_000,
            max_edges: 5_000_000,
            memory_limit_mb: 1_024,
            algorithm: GraphAlgorithm::Default,
            stubborn: false,
            symmetry: false,
            structural_rules: false,
        }
    }
}

fn validate_graph_config(config: GraphGenerationConfig) -> Result<GraphGenerationConfig, String> {
    if !(1..=10_000_000).contains(&config.max_nodes) {
        return Err("O limite de estados deve estar entre 1 e 10.000.000.".into());
    }
    if !(1..=50_000_000).contains(&config.max_edges) {
        return Err("O limite de arestas deve estar entre 1 e 50.000.000.".into());
    }
    if !(64..=8_192).contains(&config.memory_limit_mb) {
        return Err("O orçamento de memória deve estar entre 64 e 8.192 MiB.".into());
    }
    Ok(config)
}

fn analysis_context_for_graph(config: GraphGenerationConfig) -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig {
        max_memory_mb: config.memory_limit_mb,
        ..AnalysisConfig::default()
    })
}

#[derive(Serialize)]
pub struct FairnessResult {
    pub fair: bool,
    pub state_fair: bool,
}

#[derive(Serialize, Deserialize)]
pub struct StructuralResult {
    pub pure: bool,
    pub ordinary: bool,
    pub homogeneous: bool,
    pub connected: bool,
    pub strongly_connected: bool,
    pub marked_graph: bool,
    pub free_choice: bool,
    pub extended_free_choice: bool,
    pub state_machine: bool,
    pub static_conflict_free: bool,
    pub transitions_without_preset: bool,
    pub transitions_without_postset: bool,
    pub places_without_preset: bool,
    pub places_without_postset: bool,
    pub structurally_concurrent: bool,
    pub extended_simple: bool,
    pub non_blocking: bool,
    pub state_machine_coverable: Option<bool>,
    pub state_machine_decomposable: Option<bool>,
    pub state_machine_allocatable: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct BadStateTermInput {
    pub place: usize,
    pub coefficient: i64,
}

#[derive(Serialize, Deserialize)]
pub struct BadStateInput {
    pub id: String,
    pub description: String,
    pub terms: Vec<BadStateTermInput>,
    pub operator: String,
    pub constant: i64,
}

#[derive(Serialize)]
pub struct BadStateCheckEntry {
    pub id: String,
    pub status: String,
    pub certificate: Option<String>,
}

#[derive(Serialize)]
pub struct BadStateCheckResponse {
    pub safe: Option<bool>,
    pub results: Vec<BadStateCheckEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct InvariantResult {
    /// Decimal strings preserve arbitrary-precision algebra at the IPC boundary.
    pub minimal_p_semiflows: Vec<Vec<String>>,
    pub minimal_t_semiflows: Vec<Vec<String>>,
    pub structurally_bounded: bool,
    pub conservative: bool,
    pub p_invariant_coverage: bool,
    pub t_invariant_coverage: bool,
    /// One-sided invariants: `sub` = non-increasing (`<= 0`), `sur` = non-decreasing (`>= 0`).
    /// Exact invariants (zero residual) are not repeated here.
    #[serde(default)]
    pub p_sub_invariants: Vec<InequalityInvariantDto>,
    #[serde(default)]
    pub p_sur_invariants: Vec<InequalityInvariantDto>,
    #[serde(default)]
    pub t_sub_invariants: Vec<InequalityInvariantDto>,
    #[serde(default)]
    pub t_sur_invariants: Vec<InequalityInvariantDto>,
    /// Basis of ALL invariants (rational kernel), including vectors with negative entries.
    #[serde(default)]
    pub p_basis: KernelBasisDto,
    #[serde(default)]
    pub t_basis: KernelBasisDto,
}

#[derive(Serialize, Deserialize, Default)]
pub struct KernelBasisDto {
    pub vectors: Vec<Vec<String>>,
    pub rank: usize,
    /// `n - rank`: how many independent invariants exist.
    pub dimension: usize,
}

fn kernel_basis_dto(basis: racon_invariants::KernelBasis) -> KernelBasisDto {
    KernelBasisDto {
        vectors: basis.vectors.iter().map(|v| v.iter().map(ToString::to_string).collect()).collect(),
        rank: basis.rank,
        dimension: basis.dimension,
    }
}

#[derive(Serialize, Deserialize)]
pub struct InequalityInvariantDto {
    pub coefficients: Vec<String>,
    /// `y^T C` for place invariants, `C x` for transition invariants.
    pub residual: Vec<String>,
    /// `y^T M0` for place invariants when M0 is finite; always `None` for transitions.
    pub initial_value: Option<String>,
}

#[derive(Serialize)]
pub struct RequiredInitialProperties {
    pub conservative: Option<bool>,
    pub structurally_bounded: Option<bool>,
    pub bounded: Option<bool>,
    pub live: Option<bool>,
    pub live_and_safe: Option<bool>,
    /// Size of the exact reachability graph when it was fully built.
    pub state_count: Option<usize>,
    pub warning: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AnalysisResult {
    pub initial_marking: Vec<TokenDto>,
    pub bad_state_safe: Option<bool>,
    pub bounded: Option<bool>,
    pub safe: Option<bool>,
    pub live: Option<bool>,
    pub place_live: Option<bool>,
    pub exact_graph_complete: bool,
    pub analysis_status: String,
    pub analysis_message: Option<String>,
    pub unbounded_places: Vec<String>,
    pub deadlock_free: Option<bool>,
    pub deadlock_nodes: Vec<String>,
    pub dead_transitions: Option<Vec<String>>,
    pub reversible: Option<bool>,
    pub non_reversible_nodes: Vec<String>,
    pub dynamic_conflict_free: Option<bool>,
    pub persistent: Option<bool>,
    pub gfc: Option<bool>,
    pub deadlock_trap: Option<bool>,
    pub liveness: Vec<LivenessEntry>,
    pub place_liveness: Vec<PlaceLivenessEntry>,
    pub dead_places: Vec<String>,
    pub has_self_loops: bool,
    pub self_loops: Vec<String>,
    pub structural: Option<StructuralResult>,
    pub invariant: Option<InvariantResult>,
    pub analysis_info: AnalysisInfo,
    pub net_places: usize,
    pub net_transitions: usize,
    pub total_arcs: usize,
    pub total_tokens: TokenDto,
    pub evidence: Vec<MathematicalEvidence>,
}

#[derive(Serialize, Deserialize)]
pub struct NetPlaceNode {
    pub id: usize,
    pub name: String,
    pub tokens: TokenDto,
}

#[derive(Serialize, Deserialize)]
pub struct NetTransitionNode {
    pub id: usize,
    pub name: String,
    /// `true` for immediate transitions (precedence over normal ones).
    #[serde(default)]
    pub immediate: bool,
}

#[derive(Serialize, Deserialize)]
pub struct NetArc {
    pub source: String,
    pub target: String,
    pub weight: i32,
    /// `"ordinary"` or `"inhibitor"` (test arc, never consumes tokens).
    #[serde(default = "default_arc_kind")]
    pub kind: String,
    /// Inhibitor threshold `k` (requires `M(place) < k`); absent for ordinary arcs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u64>,
}

fn default_arc_kind() -> String {
    "ordinary".to_string()
}

#[derive(Serialize, Deserialize)]
pub struct NetStructureResponse {
    pub places: Vec<NetPlaceNode>,
    pub transitions: Vec<NetTransitionNode>,
    pub arcs: Vec<NetArc>,
    #[serde(default)]
    pub extended: bool,
    #[serde(default)]
    pub semantics_version: String,
}

#[derive(Serialize, Deserialize)]
pub struct CTLCheckResult {
    pub satisfied: bool,
    pub count: usize,
    pub total_states: usize,
    pub time: f64,
    pub formula: String,
    pub exact_state_space: bool,
    pub witness: Option<CTLPathResult>,
    pub counterexample: Option<CTLPathResult>,
    /// The states where the formula holds (id and marking), capped at `CTL_LISTED_STATES`.
    #[serde(default)]
    pub states: Vec<CTLStateEntry>,
    #[serde(default)]
    pub states_truncated: bool,
    #[serde(default)]
    pub place_names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CTLStateEntry {
    pub id: usize,
    pub marking: Vec<TokenDto>,
}

/// More satisfying states than this are counted but not listed.
const CTL_LISTED_STATES: usize = 1_000;

#[derive(Serialize, Deserialize)]
pub struct CTLParseErrorResult {
    pub kind: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub expected: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CTLValidationResult {
    pub valid: bool,
    pub formula: String,
    pub preview: Option<String>,
    pub error: Option<CTLParseErrorResult>,
    pub suggestions: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CTLPathResult {
    pub description: String,
    pub nodes: Vec<usize>,
    pub transitions: Vec<usize>,
    pub violation_point: Option<usize>,
    pub is_infinite: bool,
    pub cycle_start: Option<usize>,
    /// Markings and transition names along the path, so the UI can draw it.
    #[serde(default)]
    pub steps: Vec<CheckPathStep>,
    #[serde(default)]
    pub place_names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct NetInfo {
    pub num_places: usize,
    pub num_transitions: usize,
    pub place_names: Vec<String>,
    pub transition_names: Vec<String>,
    pub initial_marking: Vec<TokenDto>,
}

#[derive(Serialize, Deserialize)]
pub struct FireResult {
    pub marking: Vec<TokenDto>,
    pub enabled: Vec<usize>,
    pub deadlock: bool,
}

// ── Marking Reachability Check ──────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct CheckPathStep {
    pub marking: Vec<TokenDto>,
    pub transition: Option<usize>,
    pub transition_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MarkingCheckResult {
    pub reachable: bool,
    pub path: Vec<CheckPathStep>,
    pub bounded: Option<bool>,
    pub state_count: usize,
}

/// Algebraic (graph-free) test of a target marking: it can only rule reachability out.
#[derive(Serialize, Deserialize)]
pub struct MarkingAlgebraicResult {
    /// "unreachable" (proved) or "possibly_reachable" (necessary conditions hold).
    pub verdict: String,
    /// "p_invariant" or "state_equation" when `verdict == "unreachable"`.
    pub method: Option<String>,
    /// P-invariant `y` whose value differs between M0 and the target.
    pub invariant: Option<Vec<String>>,
    pub invariant_initial: Option<String>,
    pub invariant_target: Option<String>,
    /// Farkas vector `y`: `y^T C >= 0` yet `y^T (M - M0) < 0`.
    pub farkas: Option<Vec<String>>,
    /// Integer firing counts solving `M = M0 + C x`, when one was found.
    pub parikh: Option<Vec<String>>,
    pub place_names: Vec<String>,
    pub transition_names: Vec<String>,
}

/// Coverability of a marking: does some reachable marking have at least these tokens?
#[derive(Serialize, Deserialize)]
pub struct MarkingCoverabilityResult {
    pub covered: bool,
    /// Shortest path in the coverability graph from M0 to a covering node.
    pub path: Vec<CheckPathStep>,
    /// The path passes through ω (unbounded growth): it shows how places grow,
    /// not a literal firing sequence.
    pub path_has_omega: bool,
    pub covering_marking: Option<Vec<TokenDto>>,
    /// `Some(false)` when the coverability graph contains ω.
    pub bounded: Option<bool>,
    pub node_count: usize,
}

/// Classification of a user-supplied place- or transition-vector.
#[derive(Serialize, Deserialize)]
pub struct VectorTestResult {
    /// "place" or "transition".
    pub kind: String,
    /// `y^T C` over the transitions (place vector) or `C x` over the places (transition vector).
    pub residual: Vec<String>,
    pub semipositive: bool,
    pub positive: bool,
    /// Residual is zero: a P-invariant / T-invariant.
    pub invariant: bool,
    /// Residual `<= 0` (sub-invariant); an exact invariant also satisfies it.
    pub sub: bool,
    /// Residual `>= 0` (sur-invariant); an exact invariant also satisfies it.
    pub sur: bool,
    pub support: Vec<String>,
    /// `y . M0` for place vectors.
    pub initial_value: Option<String>,
    pub names: Vec<String>,
    pub residual_names: Vec<String>,
}

/// Two transitions that are in dynamic conflict in some reachable marking.
#[derive(Serialize, Deserialize)]
pub struct ConflictPair {
    pub first: String,
    pub second: String,
    /// In how many reachable markings this pair is in conflict.
    pub states: usize,
    /// First such marking found, as `M<id>`, with the shortest firing sequence to it.
    pub example_state: String,
    pub example_path: Vec<CheckPathStep>,
}

#[derive(Serialize, Deserialize)]
pub struct ConflictSummary {
    pub pair_count: usize,
    /// Most frequent pairs first, capped at 60.
    pub pairs: Vec<ConflictPair>,
}

#[derive(Serialize, Deserialize)]
pub struct DistanceTarget {
    /// Number of firings of the shortest path; `None` when `to` cannot be reached from `from`.
    pub distance: Option<usize>,
    pub path: Vec<CheckPathStep>,
}

/// INA "compute distances" on the reachability graph (distance = number of firings).
#[derive(Serialize, Deserialize)]
pub struct DistanceResult {
    pub state_count: usize,
    /// Distance from the source marking to every state: `histogram[d]` states lie at distance `d`.
    pub histogram: Vec<usize>,
    /// Largest finite distance from the source.
    pub eccentricity: usize,
    /// States not reachable from the source.
    pub unreachable: usize,
    /// Largest finite distance over all ordered pairs; `None` for graphs too large to scan.
    pub diameter: Option<usize>,
    /// Every state reaches every other; `None` when the scan was skipped.
    pub strongly_connected: Option<bool>,
    pub target: Option<DistanceTarget>,
}

/// `low <= M(place) <= upp`; `upp = None` is unbounded (INA's `oo`).
#[derive(Serialize, Deserialize)]
pub struct PredicateBound {
    pub place: String,
    pub low: u64,
    pub upp: Option<u64>,
}

/// A conjunction of place bounds; a predicate is a disjunction of these (INA's `.pdc`).
#[derive(Serialize, Deserialize)]
pub struct PredicateConjunction {
    /// What produced it: a transition (enabledness) or a state.
    pub label: String,
    pub bounds: Vec<PredicateBound>,
}

#[derive(Serialize, Deserialize)]
pub struct PredicateResult {
    pub conjunctions: Vec<PredicateConjunction>,
    /// The same predicate in INA's predicate-file format (`.pdc`).
    pub pdc: String,
}

#[derive(Serialize, Deserialize)]
pub struct TransitionCount {
    pub name: String,
    pub count: usize,
}

/// One elementary circuit of the reachability graph, in INA's terms.
#[derive(Serialize, Deserialize)]
pub struct CircuitDto {
    /// Number of firings (= number of distinct states on the circuit).
    pub length: usize,
    /// States on the circuit in firing order, as `M<id>`; the circuit returns to the first.
    pub states: Vec<String>,
    /// The closed path: first marking again at the end, each step with the transition fired.
    pub steps: Vec<CheckPathStep>,
    /// Firing counts: a semipositive T-invariant, since a closed walk has zero net effect.
    pub parikh: Vec<TransitionCount>,
    /// Transitions that never fire on the circuit (INA's "livelocked transitions").
    pub not_fired: Vec<String>,
    pub fair: bool,
    pub state_fair: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CircuitsResult {
    pub state_count: usize,
    pub circuits: Vec<CircuitDto>,
    /// Circuits found (counted) by the search, which can exceed `circuits.len()`.
    pub total_found: usize,
    /// `counts[l]` = circuits of length `l` seen (only for the "all" mode).
    pub length_counts: Vec<usize>,
    /// Length of the shortest / longest circuit among those found.
    pub shortest: Option<usize>,
    pub longest: Option<usize>,
    /// The search covered everything it was asked to. False when the time limit
    /// stopped it, or in "first only" mode.
    pub exhaustive: bool,
    /// The list was cut at the requested number of circuits.
    pub list_truncated: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CircuitsRequest {
    /// "shortest", "longest" or "all".
    pub mode: String,
    /// Restrict to circuits through this marking.
    pub through: Option<Vec<i32>>,
    /// Longest circuit to consider ("all" mode; also bounds "longest").
    pub max_length: Option<usize>,
    /// Stop at the first circuit of the selected kind.
    pub first_only: bool,
    /// Circuits to list (at most 1000).
    pub limit: usize,
    /// Search time limit.
    pub time_limit_ms: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SccComponent {
    pub id: usize,
    pub size: usize,
    /// No outgoing edge to another component (a bottom / terminal component).
    pub terminal: bool,
    pub trivial: bool,
    pub contains_initial: bool,
    pub has_self_loop: bool,
    /// Up to 24 member states, as `M<id>`.
    pub states: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SccSummary {
    pub count: usize,
    pub terminal_count: usize,
    pub trivial_count: usize,
    pub largest: usize,
    /// States reachable from every state (all home states), as a count.
    pub home_state_count: usize,
    pub components: Vec<SccComponent>,
}

/// Properties that need the complete, unreduced reachability graph.
#[derive(Serialize, Deserialize)]
pub struct BehavioralPropertiesResult {
    pub state_count: usize,
    pub deadlock_free: bool,
    pub deadlock_nodes: Vec<String>,
    pub dead_transitions: Vec<String>,
    pub reversible: bool,
    pub live: bool,
    pub place_live: bool,
    pub safe: bool,
    pub live_and_safe: bool,
    pub persistent: bool,
    pub gfc: bool,
    pub dynamic_conflict_free: bool,
    pub liveness: Vec<LivenessEntry>,
    pub place_liveness: Vec<PlaceLivenessEntry>,
    pub scc: SccSummary,
    pub conflicts: ConflictSummary,
}

#[derive(Serialize, Deserialize)]
pub struct IncidenceMatrixResponse {
    pub places: Vec<String>,
    pub transitions: Vec<String>,
    /// Decimal strings preserve arbitrary-precision incidence values at the IPC boundary.
    pub matrix: Vec<Vec<String>>,
}

// =============================================================================
// Conversion helpers — workspace Marking <-> i32 protocol used by frontend
// =============================================================================

fn marking_from_i32(v: &[i32]) -> Marking {
    Marking::new(
        v.iter()
            .map(|&x| {
                if x == i32::MAX {
                    TokenValue::Omega
                } else {
                    // The legacy frontend protocol cannot represent negative
                    // markings; normalize them to zero instead of wrapping
                    // into a huge u64 value.
                    TokenValue::Finite(x.max(0) as u64)
                }
            })
            .collect(),
    )
}

fn token_to_dto(token: &TokenValue) -> TokenDto {
    match token {
        TokenValue::Finite(value) => TokenDto::Finite {
            value: value.to_string(),
        },
        TokenValue::Exact(value) => TokenDto::Finite {
            value: value.to_string(),
        },
        TokenValue::Omega => TokenDto::Omega,
    }
}

fn marking_to_dto_vec(m: &Marking) -> Vec<TokenDto> {
    m.tokens
        .iter()
        .map(token_to_dto)
        .collect()
}

fn total_tokens_dto(marking: &Marking) -> TokenDto {
    let mut total = BigUint::default();
    for token in &marking.tokens {
        match token {
            TokenValue::Finite(value) => total += BigUint::from(*value),
            TokenValue::Exact(value) => total += value,
            TokenValue::Omega => return TokenDto::Omega,
        }
    }
    TokenDto::Finite {
        value: total.to_string(),
    }
}

// =============================================================================
// Parse helpers
// =============================================================================

fn parse_pnt_file(file_path: &str) -> Result<PetriNet, String> {
    let content = fs::read_to_string(file_path).map_err(|e| e.to_string())?;
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser.parse_string(&content).map_err(|e| e.to_string())?;
    validate_petri_net(&net).map_err(|e| format!("Formal Petri-net validation failed: {e}"))?;
    Ok(net)
}

/// The saved reachability graph of the net in `file_path`. Nothing is explored here:
/// the graph exists only after it was generated from the Graphs tab.
fn cached_reachability(file_path: &str, net: &PetriNet) -> Result<Arc<ReachabilityGraph>, String> {
    graph_cache::load_reachability(file_path, &Arc::new(net.clone()))
}

fn cached_coverability(file_path: &str, net: &PetriNet) -> Result<Arc<racon_coverability::CoverabilityGraph>, String> {
    graph_cache::load_coverability(file_path, &Arc::new(net.clone()))
}

#[tauri::command]
fn get_graph_cache_status(file_path: String) -> graph_cache::GraphCacheStatus {
    graph_cache::status(&file_path)
}

fn marking_distance(left: &Marking, right: &Marking) -> u128 {
    left.tokens
        .iter()
        .zip(right.tokens.iter())
        .map(|(a, b)| match (a, b) {
            (TokenValue::Finite(a), TokenValue::Finite(b)) => u128::from(a.abs_diff(*b)),
            (TokenValue::Omega, TokenValue::Omega) => 0,
            _ => u128::MAX / 2,
        })
        .sum()
}

fn build_check_path(
    graph: &racon_reachability::ReachabilityGraph,
    net: &PetriNet,
    path_indices: &[usize],
) -> Vec<CheckPathStep> {
    path_indices
        .iter()
        .enumerate()
        .map(|(i, &node_idx)| {
            let transition = if i > 0 {
                let previous = path_indices[i - 1];
                let source = NodeId(previous as u32);
                let target = NodeId(node_idx as u32);
                graph.edges.iter().find_map(|edge| {
                    (edge.source == source && edge.target == target)
                        .then_some(edge.transition.0 as usize)
                })
            } else {
                None
            };
            let transition_name = transition.and_then(|id| {
                net.transitions()
                    .iter()
                    .find(|candidate| candidate.id.0 as usize == id)
                    .map(|candidate| candidate.name.clone())
            });
            CheckPathStep {
                marking: marking_to_dto_vec(&graph.nodes[node_idx].marking),
                transition,
                transition_name,
            }
        })
        .collect()
}

// =============================================================================
// Strong connectivity (Kosaraju on the Petri net's directed bipartite graph)
// =============================================================================

fn is_strongly_connected(net: &PetriNet) -> bool {
    let np = net.place_count();
    let nt = net.transition_count();
    let total = np + nt;
    if total <= 1 {
        return true;
    }

    let mut adj: Vec<Vec<usize>> = vec![vec![]; total];
    for arc in net.arcs() {
        match (arc.source, arc.target) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                adj[p.0 as usize].push(np + t.0 as usize);
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                adj[np + t.0 as usize].push(p.0 as usize);
            }
            _ => {}
        }
    }

    // First DFS: build order
    let mut visited = vec![false; total];
    let mut order = Vec::new();

    fn dfs1(v: usize, adj: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
        visited[v] = true;
        for &u in &adj[v] {
            if !visited[u] {
                dfs1(u, adj, visited, order);
            }
        }
        order.push(v);
    }

    for v in 0..total {
        if !visited[v] {
            dfs1(v, &adj, &mut visited, &mut order);
        }
    }

    // Reverse graph
    let mut radj: Vec<Vec<usize>> = vec![vec![]; total];
    for v in 0..total {
        for &u in &adj[v] {
            radj[u].push(v);
        }
    }

    // Second DFS on reversed graph
    let mut comp = vec![0usize; total];
    let mut comp_count = 0;

    fn dfs2(v: usize, radj: &[Vec<usize>], comp: &mut [usize], comp_id: usize) {
        comp[v] = comp_id;
        for &u in &radj[v] {
            if comp[u] == 0 {
                dfs2(u, radj, comp, comp_id);
            }
        }
    }

    for &v in order.iter().rev() {
        if comp[v] == 0 {
            comp_count += 1;
            dfs2(v, &radj, &mut comp, comp_count);
        }
    }

    comp_count == 1
}

// =============================================================================
// Structural result conversion
// =============================================================================

fn structural_to_result(net: &PetriNet) -> StructuralResult {
    let props = StructuralAnalyzer::analyze(net);
    structural_result_from_properties(net, &props)
}

fn structural_result_from_properties(
    net: &PetriNet,
    props: &racon_structural::topology::StructuralProperties,
) -> StructuralResult {
    StructuralResult {
        pure: props.is_pure,
        ordinary: props.is_ordinary,
        homogeneous: props.is_homogeneous,
        connected: props.is_connected,
        strongly_connected: is_strongly_connected(net),
        marked_graph: props.is_marked_graph,
        free_choice: props.is_free_choice,
        extended_free_choice: props.is_extended_free_choice,
        state_machine: props.is_state_machine,
        static_conflict_free: props.is_static_conflict_free,
        // The structural analyzer stores the violation form (`has_*`).
        // The UI properties are positive: true means no source/sink exists.
        transitions_without_preset: !props.has_transitions_without_preset,
        transitions_without_postset: !props.has_transitions_without_postset,
        places_without_preset: !props.has_places_without_preset,
        places_without_postset: !props.has_places_without_postset,
        structurally_concurrent: props.is_structurally_concurrent,
        extended_simple: props.is_extended_simple,
        non_blocking: props.is_non_blocking,
        state_machine_coverable: props.state_machine_coverable,
        state_machine_decomposable: props.state_machine_decomposable,
        state_machine_allocatable: props.state_machine_allocatable,
    }
}

fn compute_invariant_analysis(net: &PetriNet) -> Result<racon_invariants::InvariantAnalysisResult, String> {
    let ctx = racon_core::execution::AnalysisContext::new(
        racon_core::execution::AnalysisConfig::default(),
    );
    let engine = InvariantEngineImpl::default();

    let p_semiflows = engine
        .compute_p_semiflows(net, &ctx)
        .map_err(|e| format!("Invariant computation error: {:?}", e))?;
    let t_semiflows = engine
        .compute_t_semiflows(net, &ctx)
        .map_err(|e| format!("Invariant computation error: {:?}", e))?;

    let (is_conservative, is_cpi, is_cti) =
        derive_structural_properties(net, &p_semiflows, &t_semiflows);

    // Murata's structural-boundedness certificate is C^T y <= 0 with y > 0.
    // CPI/conservativity remain valid sufficient certificates as well.
    Ok(racon_invariants::InvariantAnalysisResult {
        minimal_p_semiflows: p_semiflows.iter().map(|inv| inv.coefficients.clone()).collect(),
        minimal_t_semiflows: t_semiflows.iter().map(|inv| inv.coefficients.clone()).collect(),
        p_semiflows,
        t_semiflows,
        is_conservative,
        is_cpi,
        is_cti,
        structural_bounds: std::collections::HashMap::new(),
        structural_bounded_by_inequality: structural_boundedness_certificate(net),
    })
}

fn invariant_to_result(result: &racon_invariants::InvariantAnalysisResult) -> InvariantResult {
    InvariantResult {
        minimal_p_semiflows: result.minimal_p_semiflows.iter().map(|v| v.iter().map(ToString::to_string).collect()).collect(),
        minimal_t_semiflows: result.minimal_t_semiflows.iter().map(|v| v.iter().map(ToString::to_string).collect()).collect(),
        structurally_bounded: result.is_cpi || result.is_conservative || result.structural_bounded_by_inequality.is_some(),
        conservative: result.is_conservative,
        p_invariant_coverage: result.is_cpi,
        t_invariant_coverage: result.is_cti,
        p_sub_invariants: Vec::new(),
        p_sur_invariants: Vec::new(),
        t_sub_invariants: Vec::new(),
        t_sur_invariants: Vec::new(),
        p_basis: KernelBasisDto::default(),
        t_basis: KernelBasisDto::default(),
    }
}

/// Proper (non-exact) one-sided invariants. `transpose` solves `C x <= 0 / >= 0`
/// for transitions by running the place algorithm on `C^T`.
fn inequality_invariants(
    net: &PetriNet,
    relation: InequalityRelation,
    transpose: bool,
) -> Result<Vec<InequalityInvariantDto>, String> {
    let context = AnalysisContext::new(AnalysisConfig::default());
    let c: Vec<Vec<BigInt>> = net.incidence_matrix().data().to_vec();
    let matrix = if transpose {
        let cols = c.first().map_or(0, Vec::len);
        (0..cols).map(|t| c.iter().map(|row| row[t].clone()).collect()).collect()
    } else {
        c
    };
    let marking = net.initial_marking();
    let found = compute_minimal_place_inequality_invariants(&matrix, relation, &context)
        .map_err(|e| format!("Inequality invariant computation error: {:?}", e))?;
    Ok(found
        .into_iter()
        .filter(|inv| !inv.is_exact)
        .map(|inv| {
            let initial_value = if transpose {
                None
            } else {
                inv.coefficients
                    .iter()
                    .enumerate()
                    .try_fold(BigInt::from(0), |acc, (i, y)| {
                        marking
                            .get(i)
                            .and_then(TokenValue::as_biguint)
                            .map(|m| acc + y * BigInt::from(m))
                    })
                    .map(|v| v.to_string())
            };
            InequalityInvariantDto {
                coefficients: inv.coefficients.iter().map(ToString::to_string).collect(),
                residual: inv.residual.iter().map(ToString::to_string).collect(),
                initial_value,
            }
        })
        .collect())
}

fn compute_invariant_result(net: &PetriNet) -> Result<InvariantResult, String> {
    let mut result = compute_invariant_analysis(net).map(|r| invariant_to_result(&r))?;
    result.p_sub_invariants = inequality_invariants(net, InequalityRelation::LessOrEqual, false)?;
    result.p_sur_invariants = inequality_invariants(net, InequalityRelation::GreaterOrEqual, false)?;
    result.t_sub_invariants = inequality_invariants(net, InequalityRelation::LessOrEqual, true)?;
    result.t_sur_invariants = inequality_invariants(net, InequalityRelation::GreaterOrEqual, true)?;
    result.p_basis = kernel_basis_dto(p_invariant_basis(net));
    result.t_basis = kernel_basis_dto(t_invariant_basis(net));
    Ok(result)
}

#[tauri::command]
async fn get_deadlock_trap_property(file_path: String) -> Result<Option<bool>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        Ok(ExpertSystem::deadlock_trap_property(&net))
    })
    .await
    .map_err(|error| format!("Falha na tarefa Deadlock-Trap: {error}"))?
}

// =============================================================================
// Self-loop detection
// =============================================================================

fn find_self_loops(net: &PetriNet) -> (bool, Vec<String>) {
    let mut loops = Vec::new();
    for t in net.transitions() {
        let pre: HashSet<PlaceId> = net.preset(t.id).iter().map(|(p, _)| *p).collect();
        let post: HashSet<PlaceId> = net.postset(t.id).iter().map(|(p, _)| *p).collect();
        if !pre.is_disjoint(&post) {
            loops.push(t.name.clone());
        }
    }
    let has = !loops.is_empty();
    (has, loops)
}

// =============================================================================
// Tauri commands
// =============================================================================

fn log_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_log_dir().map_err(|e| format!("Could not resolve log directory: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create log directory: {e}"))?;
    Ok(dir.join("racon.log"))
}

#[derive(Serialize, Deserialize)]
struct LogEntry {
    timestamp: String,
    level: String,
    source: String,
    event: String,
    message: String,
}

#[tauri::command]
fn write_log_entry(app: AppHandle, entry: LogEntry) -> Result<(), String> {
    let path = log_file_path(&app)?;
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path)
        .map_err(|e| format!("Could not open application log: {e}"))?;
    serde_json::to_writer(&mut file, &entry).map_err(|e| format!("Could not serialize log entry: {e}"))?;
    file.write_all(b"\n").map_err(|e| format!("Could not write application log: {e}"))
}

#[tauri::command]
fn read_log_entries(app: AppHandle) -> Result<Vec<LogEntry>, String> {
    let path = log_file_path(&app)?;
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read application log: {error}")),
    };
    Ok(content.lines().rev().take(2000).filter_map(|line| serde_json::from_str(line).ok()).collect())
}

// ── Marking Reachability (simplified BFS) ───────────────────────────────────

#[tauri::command]
async fn check_marking_reachability(
    file_path: String,
    marking: Vec<i32>,
    config: GraphGenerationConfig,
) -> Result<MarkingCheckResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        check_marking_reachability_impl(file_path, marking, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de verificação de marcação: {error}"))?
}

fn check_marking_reachability_impl(
    file_path: String,
    marking: Vec<i32>,
    config: GraphGenerationConfig,
) -> Result<MarkingCheckResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let target = marking_from_i32(&marking);

    if target.len() != net.place_count() {
        return Err(format!(
            "Marking length {} does not match number of places {}",
            marking.len(),
            net.place_count()
        ));
    }

    let ctx = analysis_context_for_graph(config);
    if config.stubborn {
        ensure_algorithm_allowed(&net, racon_reduction::catalog::Objective::Reachability, config.algorithm, true, false)?;
        return check_marking_reachability_stubborn(&net, &target, config, &ctx);
    }
    let graph = cached_reachability(&file_path, &net)?;

    let state_count = graph.node_count();

    // Find target marking in graph
    let target_idx = graph.get_node_by_marking(&target);

    let Some(target_idx) = target_idx else {
        let closest_idx = graph
            .nodes
            .iter()
            .enumerate()
            .min_by_key(|(_, node)| marking_distance(&node.marking, &target))
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        let path = graph
            .bfs_path(0, closest_idx)
            .map(|indices| build_check_path(&graph, &net, &indices))
            .unwrap_or_default();
        return Ok(MarkingCheckResult {
            reachable: false,
            path,
            bounded: Some(true),
            state_count,
        });
    };

    // BFS path from initial (0) to target
    let path_indices = graph.bfs_path(0, target_idx);

    let Some(path_indices) = path_indices else {
        return Ok(MarkingCheckResult {
            reachable: true,
            path: vec![],
            bounded: Some(true),
            state_count,
        });
    };

    let path = build_check_path(&graph, &net, &path_indices);

    Ok(MarkingCheckResult {
        reachable: true,
        path,
        bounded: Some(true),
        state_count,
    })
}

/// Goal-directed stubborn-set search (Schmidt 1999, Thm 3). Preserves
/// reachability of the target marking without building the full graph; the
/// witness path is a firing sequence of the original net.
fn check_marking_reachability_stubborn(
    net: &PetriNet,
    target: &Marking,
    config: GraphGenerationConfig,
    ctx: &AnalysisContext,
) -> Result<MarkingCheckResult, String> {
    use racon_reduction::{search_goal, GoalSearchConfig, GoalSearchOutcome, ReachGoal};
    let goal = ReachGoal::marking(target).ok_or("Marcação alvo inválida para a busca stubborn.")?;
    let report = search_goal(
        net,
        &goal,
        GoalSearchConfig { max_nodes: config.max_nodes, use_stubborn: true },
        ctx,
    )
    .map_err(|e| format!("Stubborn reachability failed: {e}"))?;
    match report.outcome {
        GoalSearchOutcome::Reachable { path, .. } => {
            let mut marking = net.initial_marking().clone();
            let mut steps = vec![CheckPathStep { marking: marking_to_dto_vec(&marking), transition: None, transition_name: None }];
            for transition in path {
                marking = net.fire(transition, &marking);
                steps.push(CheckPathStep {
                    marking: marking_to_dto_vec(&marking),
                    transition: Some(transition.0 as usize),
                    transition_name: net.transitions().iter().find(|t| t.id == transition).map(|t| t.name.clone()),
                });
            }
            Ok(MarkingCheckResult { reachable: true, path: steps, bounded: None, state_count: report.visited })
        }
        GoalSearchOutcome::Unreachable => Ok(MarkingCheckResult { reachable: false, path: vec![], bounded: None, state_count: report.visited }),
    }
}

#[tauri::command]
async fn compute_behavioral_properties(
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<BehavioralPropertiesResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || compute_behavioral_properties_impl(file_path, config))
        .await
        .map_err(|error| format!("Falha na tarefa de propriedades comportamentais: {error}"))?
}

fn compute_behavioral_properties_impl(
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<BehavioralPropertiesResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let ctx = analysis_context_for_graph(config);
    // Always the full graph: persistence, GFC and dynamic conflicts are not
    // preserved by stubborn-set or other reductions.
    let graph = cached_reachability(&file_path, &net)?;
    if !graph.is_exact() {
        return Err("Exact reachability graph required: the state space is not exact".into());
    }

    let structural = StructuralAnalyzer::analyze_basic(&net);
    let invariants = compute_invariant_analysis(&net)?;
    let behavior = ExpertSystem::derive(&net, &structural, &invariants, Some(&graph));

    let transition_name = |id: TransitionId| {
        net.transitions()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| format!("t{}", id.0))
    };
    let level_label = |level: racon_structural::expert::LivenessLevel| match level {
        racon_structural::expert::LivenessLevel::L0 => "L0",
        racon_structural::expert::LivenessLevel::L1 => "L1",
        racon_structural::expert::LivenessLevel::L2 => "L2",
        racon_structural::expert::LivenessLevel::L3 => "L3",
        racon_structural::expert::LivenessLevel::L4 => "L4",
        racon_structural::expert::LivenessLevel::Unknown => "?",
    };

    let place_liveness: Vec<PlaceLivenessEntry> = net
        .places()
        .iter()
        .map(|place| PlaceLivenessEntry {
            place: place.id.0 as usize,
            name: place.name.clone(),
            live: ExpertSystem::is_place_live(&net, place.id, &graph),
        })
        .collect();
    let live = ExpertSystem::is_live(&net, &graph);
    let safe = (0..graph.node_count()).all(|index| {
        graph
            .marking(NodeId(index as u32))
            .tokens
            .iter()
            .all(|token| matches!(token, TokenValue::Finite(value) if *value <= 1))
    });

    let scc_result = racon_scc::tarjan::SCCEngine
        .compute_sccs(&graph, &ctx)
        .map_err(|e| format!("SCC computation failed: {e}"))?;
    let dag = racon_scc::dag::SCCDAG::analyze(&scc_result, &graph);
    let mut components: Vec<SccComponent> = scc_result
        .sccs
        .iter()
        .map(|scc| SccComponent {
            id: scc.id.0 as usize,
            size: scc.nodes.len(),
            terminal: dag.terminal_sccs.contains(&scc.id),
            trivial: scc.is_trivial,
            contains_initial: scc.nodes.iter().any(|n| *n == graph.initial_node),
            has_self_loop: scc.has_self_loop,
            states: scc.nodes.iter().take(24).map(|n| format!("M{}", n.0)).collect(),
        })
        .collect();
    components.sort_by(|a, b| b.size.cmp(&a.size).then(a.id.cmp(&b.id)));
    let scc = SccSummary {
        count: components.len(),
        terminal_count: components.iter().filter(|c| c.terminal).count(),
        trivial_count: components.iter().filter(|c| c.trivial).count(),
        largest: components.first().map_or(0, |c| c.size),
        // With exactly one terminal component, every state in it is reachable from
        // every state of the net (a home state), whether or not M0 belongs to it.
        home_state_count: match dag.terminal_sccs.as_slice() {
            [only] => scc_result.sccs.iter().find(|c| c.id == *only).map_or(0, |c| c.nodes.len()),
            _ => 0,
        },
        components,
    };

    // Dynamic conflicts: pairs of fireable transitions that cannot both fire (weighted
    // demand for extended nets: firing one disables the other), grouped by pair.
    let mut conflict_map: std::collections::BTreeMap<(usize, usize), (usize, usize)> = std::collections::BTreeMap::new();
    for index in 0..graph.node_count() {
        let marking = graph.marking(NodeId(index as u32));
        let fireable = net.fireable_transitions(marking);
        for (position, &first) in fireable.iter().enumerate() {
            for &second in fireable.iter().skip(position + 1) {
                if net.is_dynamic_conflict(first, second, marking) {
                    let key = (first.0.min(second.0) as usize, first.0.max(second.0) as usize);
                    let entry = conflict_map.entry(key).or_insert((0, index));
                    entry.0 += 1;
                }
            }
        }
    }
    let pair_count = conflict_map.len();
    let mut ranked: Vec<((usize, usize), (usize, usize))> = conflict_map.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    let conflicts = ConflictSummary {
        pair_count,
        pairs: ranked
            .into_iter()
            .take(60)
            .map(|((a, b), (states, example))| ConflictPair {
                first: transition_name(TransitionId(a as u32)),
                second: transition_name(TransitionId(b as u32)),
                states,
                example_state: format!("M{example}"),
                example_path: graph
                    .bfs_path(0, example)
                    .map(|path| build_check_path(&graph, &net, &path))
                    .unwrap_or_default(),
            })
            .collect(),
    };

    Ok(BehavioralPropertiesResult {
        state_count: graph.node_count(),
        deadlock_free: behavior.is_deadlock_free.unwrap_or(false),
        deadlock_nodes: ExpertSystem::deadlock_nodes(&graph).iter().map(|n| format!("M{}", n.0)).collect(),
        dead_transitions: behavior.dead_transitions.iter().map(|&id| transition_name(id)).collect(),
        reversible: behavior.is_reversible.unwrap_or(false),
        live,
        place_live: place_liveness.iter().all(|p| p.live),
        safe,
        live_and_safe: live && safe,
        persistent: behavior.is_persistent.unwrap_or(false),
        gfc: behavior.is_gfc.unwrap_or(false),
        dynamic_conflict_free: ExpertSystem::is_dynamic_conflict_free(&net, &graph),
        liveness: net
            .transitions()
            .iter()
            .map(|t| LivenessEntry {
                transition: t.id.0 as usize,
                name: t.name.clone(),
                level: level_label(ExpertSystem::liveness_level(&net, t.id, &graph)).into(),
            })
            .collect(),
        place_liveness,
        scc,
        conflicts,
    })
}

/// INA "non-reachability test": P-invariants, then the state equation. Needs no
/// reachability graph, so it also works for unbounded nets.
#[tauri::command]
async fn test_marking_algebraic(file_path: String, marking: Vec<i32>) -> Result<MarkingAlgebraicResult, String> {
    tauri::async_runtime::spawn_blocking(move || test_marking_algebraic_impl(file_path, marking))
        .await
        .map_err(|error| format!("Falha na tarefa de teste algébrico: {error}"))?
}

fn test_marking_algebraic_impl(file_path: String, marking: Vec<i32>) -> Result<MarkingAlgebraicResult, String> {
    let net = parse_pnt_file(&file_path)?;
    if marking.len() != net.place_count() {
        return Err(format!("Marking length {} does not match number of places {}", marking.len(), net.place_count()));
    }
    if marking.iter().any(|v| *v < 0) {
        return Err("Token counts must not be negative".into());
    }
    let target: Vec<BigInt> = marking.iter().map(|v| BigInt::from(*v)).collect();
    let place_names: Vec<String> = net.places().iter().map(|p| p.name.clone()).collect();
    let transition_names: Vec<String> = net.transitions().iter().map(|t| t.name.clone()).collect();
    let base = |verdict: &str| MarkingAlgebraicResult {
        verdict: verdict.into(),
        method: None,
        invariant: None,
        invariant_initial: None,
        invariant_target: None,
        farkas: None,
        parikh: None,
        place_names: place_names.clone(),
        transition_names: transition_names.clone(),
    };

    // 1) P-invariants: y . M must be the same in every reachable marking.
    let ctx = AnalysisContext::new(AnalysisConfig::default());
    let flows = InvariantEngineImpl::default()
        .compute_p_semiflows(&net, &ctx)
        .map_err(|e| format!("Invariant computation error: {e:?}"))?;
    let initial: Option<Vec<BigInt>> = net
        .initial_marking()
        .tokens
        .iter()
        .map(|t| match t {
            TokenValue::Finite(v) => Some(BigInt::from(*v)),
            TokenValue::Exact(v) => Some(BigInt::from(v.clone())),
            TokenValue::Omega => None,
        })
        .collect();
    if let Some(initial) = &initial {
        // Semiflows first (non-negative, easiest to read), then the full kernel basis:
        // y.M is constant for EVERY invariant, so the basis is the complete test.
        let mut vectors: Vec<Vec<BigInt>> = flows.iter().map(|flow| flow.coefficients.clone()).collect();
        vectors.extend(p_invariant_basis(&net).vectors);
        for coefficients in &vectors {
            let at = |m: &[BigInt]| -> BigInt { coefficients.iter().zip(m).map(|(y, v)| y * v).sum() };
            let (v0, vt) = (at(initial), at(&target));
            if v0 != vt {
                let mut out = base("unreachable");
                out.method = Some("p_invariant".into());
                out.invariant = Some(coefficients.iter().map(ToString::to_string).collect());
                out.invariant_initial = Some(v0.to_string());
                out.invariant_target = Some(vt.to_string());
                return Ok(out);
            }
        }
    }

    // 2) State equation M = M0 + C x, x >= 0 (necessary condition only).
    match state_equation_test(&net, &target) {
        StateEquationVerdict::Infeasible { farkas } => {
            let mut out = base("unreachable");
            out.method = Some("state_equation".into());
            out.farkas = Some(farkas.iter().map(ToString::to_string).collect());
            Ok(out)
        }
        StateEquationVerdict::Solution { parikh } => {
            let mut out = base("possibly_reachable");
            out.parikh = Some(parikh.iter().map(ToString::to_string).collect());
            Ok(out)
        }
        StateEquationVerdict::Inconclusive => Ok(base("possibly_reachable")),
    }
}

/// INA "test the coverability of a marking" (Karp-Miller). Unlike reachability it
/// always terminates, so it works for unbounded nets.
#[tauri::command]
async fn check_marking_coverability(
    file_path: String,
    marking: Vec<i32>,
    config: GraphGenerationConfig,
) -> Result<MarkingCoverabilityResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || check_marking_coverability_impl(file_path, marking, config))
        .await
        .map_err(|error| format!("Falha na tarefa de cobertura de marcação: {error}"))?
}

fn check_marking_coverability_impl(
    file_path: String,
    marking: Vec<i32>,
    _config: GraphGenerationConfig,
) -> Result<MarkingCoverabilityResult, String> {
    let net = parse_pnt_file(&file_path)?;
    if net.is_extended() {
        return Err(UNSUPPORTED_EXTENDED_COVERABILITY.to_string());
    }
    if marking.len() != net.place_count() {
        return Err(format!("Marking length {} does not match number of places {}", marking.len(), net.place_count()));
    }
    if marking.iter().any(|v| *v < 0) {
        return Err("Token counts must not be negative".into());
    }
    let target = marking_from_i32(&marking);
    let graph = cached_coverability(&file_path, &net)?;

    // M is coverable iff some node of the (complete) Karp-Miller graph is >= M
    // place by place, with ω above every finite count.
    let covers = |index: usize| {
        let node = &graph.nodes[index];
        node.is_active
            && node
                .marking
                .tokens
                .iter()
                .zip(target.tokens.iter())
                .all(|(have, want)| have.geq(want))
    };
    // Breadth-first from M0: the covering node reached with the fewest firings.
    let start = graph.initial_node.0 as usize;
    let mut parent: Vec<Option<(usize, usize)>> = vec![None; graph.nodes.len()];
    let mut seen = vec![false; graph.nodes.len()];
    let mut queue = std::collections::VecDeque::from([start]);
    seen[start] = true;
    let mut found = None;
    while let Some(current) = queue.pop_front() {
        if covers(current) {
            found = Some(current);
            break;
        }
        for &(next, transition) in &graph.edge_adjacency[current] {
            let next = next.0 as usize;
            if !seen[next] {
                seen[next] = true;
                parent[next] = Some((current, transition.0 as usize));
                queue.push_back(next);
            }
        }
    }

    let bounded = graph.bounded_status();
    let node_count = graph.nodes.iter().filter(|n| n.is_active).count();
    let Some(end) = found else {
        return Ok(MarkingCoverabilityResult {
            covered: false,
            path: Vec::new(),
            path_has_omega: false,
            covering_marking: None,
            bounded,
            node_count,
        });
    };

    let mut chain = vec![(end, None::<usize>)];
    let mut cursor = end;
    while let Some((previous, transition)) = parent[cursor] {
        chain.last_mut().expect("non-empty").1 = Some(transition);
        chain.push((previous, None));
        cursor = previous;
    }
    chain.reverse();
    // Each entry carries the transition fired to reach that node (None for M0).
    let steps: Vec<CheckPathStep> = chain
        .iter()
        .map(|(node, transition)| {
            let transition = *transition;
            let transition_name = transition.and_then(|id| {
                net.transitions().iter().find(|t| t.id.0 as usize == id).map(|t| t.name.clone())
            });
            CheckPathStep {
                marking: marking_to_dto_vec(&graph.nodes[*node].marking),
                transition,
                transition_name,
            }
        })
        .collect();
    let path_has_omega = chain.iter().any(|(node, _)| graph.nodes[*node].has_omega);
    Ok(MarkingCoverabilityResult {
        covered: true,
        path: steps,
        path_has_omega,
        covering_marking: Some(marking_to_dto_vec(&graph.nodes[end].marking)),
        bounded,
        node_count,
    })
}

/// INA "compute distances": breadth-first distances (in firings) over the exact graph.
#[tauri::command]
async fn compute_distances(
    file_path: String,
    from: Option<Vec<i32>>,
    to: Option<Vec<i32>>,
    config: GraphGenerationConfig,
) -> Result<DistanceResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || compute_distances_impl(file_path, from, to, config))
        .await
        .map_err(|error| format!("Falha na tarefa de distâncias: {error}"))?
}

/// Scanning every source is O(states x edges); skip it beyond this many states.
const DIAMETER_STATE_LIMIT: usize = 2_000;

fn compute_distances_impl(
    file_path: String,
    from: Option<Vec<i32>>,
    to: Option<Vec<i32>>,
    _config: GraphGenerationConfig,
) -> Result<DistanceResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let graph = cached_reachability(&file_path, &net)?;

    let state_of = |marking: &[i32], label: &str| -> Result<usize, String> {
        if marking.len() != net.place_count() {
            return Err(format!("Marking length {} does not match number of places {}", marking.len(), net.place_count()));
        }
        if marking.iter().any(|v| *v < 0) {
            return Err("Token counts must not be negative".into());
        }
        graph
            .get_node_by_marking(&marking_from_i32(marking))
            .ok_or_else(|| format!("The {label} marking is not reachable, so it is not a state of the graph"))
    };
    let source = match &from {
        Some(marking) => state_of(marking, "source")?,
        None => 0,
    };

    let count = graph.node_count();
    let distances_from = |origin: usize| -> Vec<Option<usize>> {
        let mut distance: Vec<Option<usize>> = vec![None; count];
        distance[origin] = Some(0);
        let mut queue = std::collections::VecDeque::from([origin]);
        while let Some(current) = queue.pop_front() {
            for next in graph.adjacency[current].iter().map(|n| n.0 as usize) {
                if distance[next].is_none() {
                    distance[next] = distance[current].map(|d| d + 1);
                    queue.push_back(next);
                }
            }
        }
        distance
    };
    let from_source = distances_from(source);
    let eccentricity = from_source.iter().flatten().copied().max().unwrap_or(0);
    let mut histogram = vec![0usize; eccentricity + 1];
    for d in from_source.iter().flatten() {
        histogram[*d] += 1;
    }
    let unreachable = from_source.iter().filter(|d| d.is_none()).count();

    let (diameter, strongly_connected) = if count <= DIAMETER_STATE_LIMIT {
        let mut diameter = 0usize;
        let mut connected = true;
        for origin in 0..count {
            let row = distances_from(origin);
            connected &= row.iter().all(Option::is_some);
            diameter = diameter.max(row.iter().flatten().copied().max().unwrap_or(0));
        }
        (Some(diameter), Some(connected))
    } else {
        (None, None)
    };

    let target = match &to {
        Some(marking) => {
            let end = state_of(marking, "target")?;
            match from_source[end] {
                Some(distance) => Some(DistanceTarget {
                    distance: Some(distance),
                    path: graph.bfs_path(source, end).map(|path| build_check_path(&graph, &net, &path)).unwrap_or_default(),
                }),
                None => Some(DistanceTarget { distance: None, path: Vec::new() }),
            }
        }
        None => None,
    };
    Ok(DistanceResult { state_count: count, histogram, eccentricity, unreachable, diameter, strongly_connected, target })
}

/// INA "compute circuits": elementary circuits of the exact reachability graph.
///
/// A graph can have astronomically many circuits (billions for a net with a
/// hundred states), so the search is bounded by a time limit and a list size; the
/// result says whether it was exhaustive.
#[tauri::command]
async fn compute_circuits(
    file_path: String,
    request: CircuitsRequest,
    config: GraphGenerationConfig,
) -> Result<CircuitsResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || compute_circuits_impl(file_path, request, config))
        .await
        .map_err(|error| format!("Falha na tarefa de circuitos: {error}"))?
}

type Adjacency = Vec<Vec<(usize, usize)>>;

/// Depth-limited DFS for elementary circuits. `start` is the smallest-index node of
/// every circuit it reports when `smallest_only`, otherwise any circuit through it.
/// `on_circuit(nodes, transitions)` returns false to stop. Returns false when stopped
/// (by the callback or the deadline).
/// Shortest distance (in firings) from every node back to `start`, ignoring nodes
/// below `start` when `smallest_only`; `usize::MAX` when it cannot return.
fn back_distances(reverse: &[Vec<usize>], start: usize, smallest_only: bool) -> Vec<usize> {
    let mut distance = vec![usize::MAX; reverse.len()];
    distance[start] = 0;
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        for &previous in &reverse[current] {
            if distance[previous] == usize::MAX && (!smallest_only || previous >= start) {
                distance[previous] = distance[current] + 1;
                queue.push_back(previous);
            }
        }
    }
    distance
}

fn dfs_circuits(
    adj: &Adjacency,
    back: &[usize],
    start: usize,
    smallest_only: bool,
    max_len: usize,
    deadline: Instant,
    steps: &mut u64,
    on_circuit: &mut dyn FnMut(&[usize], &[usize]) -> bool,
) -> bool {
    let mut path = vec![start];
    let mut via: Vec<usize> = Vec::new();
    let mut on_path = vec![false; adj.len()];
    on_path[start] = true;
    let mut cursor = vec![0usize];
    while let Some(&position) = cursor.last() {
        *steps += 1;
        if *steps % 4096 == 0 && Instant::now() >= deadline {
            return false;
        }
        let node = *path.last().expect("non-empty");
        if position < adj[node].len() {
            *cursor.last_mut().expect("non-empty") += 1;
            let (next, transition) = adj[node][position];
            if next == start {
                via.push(transition);
                let keep_going = on_circuit(&path, &via);
                via.pop();
                if !keep_going {
                    return false;
                }
            } else if (!smallest_only || next > start)
                && !on_path[next]
                // Prune: after stepping to `next` the circuit still needs `back[next]` more firings.
                && back[next] != usize::MAX
                && path.len() + back[next] <= max_len
            {
                on_path[next] = true;
                path.push(next);
                via.push(transition);
                cursor.push(0);
            }
        } else {
            cursor.pop();
            on_path[path.pop().expect("non-empty")] = false;
            via.pop();
        }
    }
    true
}

/// Length of the shortest circuit through `start` (BFS), if any.
fn shortest_circuit_through(adj: &Adjacency, start: usize) -> Option<usize> {
    let mut distance = vec![usize::MAX; adj.len()];
    distance[start] = 0;
    let mut queue = std::collections::VecDeque::from([start]);
    let mut best: Option<usize> = None;
    while let Some(current) = queue.pop_front() {
        for &(next, _) in &adj[current] {
            if next == start {
                let length = distance[current] + 1;
                best = Some(best.map_or(length, |b| b.min(length)));
            } else if distance[next] == usize::MAX {
                distance[next] = distance[current] + 1;
                queue.push_back(next);
            }
        }
    }
    best
}

fn compute_circuits_impl(
    file_path: String,
    request: CircuitsRequest,
    _config: GraphGenerationConfig,
) -> Result<CircuitsResult, String> {
    if !["shortest", "longest", "all"].contains(&request.mode.as_str()) {
        return Err(format!("Unknown circuit mode: {}", request.mode));
    }
    let net = parse_pnt_file(&file_path)?;
    let graph = cached_reachability(&file_path, &net)?;

    let count = graph.node_count();
    let adj: Adjacency = (0..count)
        .map(|u| graph.outgoing_edges(NodeId(u as u32)).iter().map(|&(to, label)| (to.0 as usize, label.0 as usize)).collect())
        .collect();
    let through = match &request.through {
        Some(marking) => {
            if marking.len() != net.place_count() || marking.iter().any(|v| *v < 0) {
                return Err("The marking must have one non-negative value per place".into());
            }
            Some(graph.get_node_by_marking(&marking_from_i32(marking)).ok_or("That marking is not a reachable state")?)
        }
        None => None,
    };

    let limit = request.limit.clamp(1, 1000);
    let deadline = Instant::now() + std::time::Duration::from_millis(request.time_limit_ms.clamp(100, 60_000));
    let cap = request.max_length.unwrap_or(count).max(1).min(count.max(1));
    let mut steps = 0u64;

    // Which circuit lengths qualify, and how deep the search must go.
    let mut target: Option<usize> = None;
    if request.mode == "shortest" {
        let girth = match through {
            Some(s) => shortest_circuit_through(&adj, s),
            None => (0..count).filter_map(|s| shortest_circuit_through(&adj, s)).min(),
        };
        match girth {
            Some(g) => target = Some(g),
            None => {
                return Ok(CircuitsResult { state_count: count, circuits: Vec::new(), total_found: 0, length_counts: Vec::new(), shortest: None, longest: None, exhaustive: true, list_truncated: false });
            }
        }
    }
    let depth = target.unwrap_or(cap);
    let mut reverse: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (u, edges) in adj.iter().enumerate() {
        for &(v, _) in edges {
            reverse[v].push(u);
        }
    }

    let mut total = 0usize;
    let mut length_counts = vec![0usize; depth + 1];
    // Circuits kept: for "all"/"shortest" the first `limit` per length (assembled shortest
    // first); for "longest" those of the longest length reached.
    let mut kept: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
    let mut stopped = false;
    let starts: Vec<usize> = match through {
        Some(s) => vec![s],
        None => (0..count).collect(),
    };
    if request.mode == "longest" {
        // Iterative deepening: find circuits of exactly length 2, 3, 4, ... and keep the longest
        // length reached before the time limit. Circuit lengths need not be contiguous (a
        // bipartite graph has only even ones), so "none of length L" proves nothing about L+1:
        // the result is "at least this long", exact only when the loop reaches the cap.
        for length in 2..=cap {
            let mut found: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
            let mut timed_out = false;
            'starts: for &start in &starts {
                let back = back_distances(&reverse, start, through.is_none());
                let mut collect = |nodes: &[usize], via: &[usize]| -> bool {
                    if nodes.len() == length {
                        found.push((nodes.to_vec(), via.to_vec()));
                    }
                    found.len() < limit
                };
                if !dfs_circuits(&adj, &back, start, through.is_none(), length, deadline, &mut steps, &mut collect) {
                    timed_out = Instant::now() >= deadline;
                    break 'starts;
                }
            }
            if !found.is_empty() {
                kept = found;
            }
            if timed_out {
                stopped = true;
                break;
            }
        }
        total = kept.len();
        if let Some((nodes, _)) = kept.first() {
            length_counts[nodes.len().min(depth)] = kept.len();
        }
    } else {
        let mut handle = |nodes: &[usize], via: &[usize]| -> bool {
            let length = nodes.len();
            if let Some(t) = target {
                if length != t {
                    return true;
                }
            }
            total += 1;
            length_counts[length] += 1;
            if length_counts[length] <= limit {
                kept.push((nodes.to_vec(), via.to_vec()));
            }
            !request.first_only
        };
        for &start in &starts {
            let back = back_distances(&reverse, start, through.is_none());
            if !dfs_circuits(&adj, &back, start, through.is_none(), depth, deadline, &mut steps, &mut handle) {
                stopped = true;
                break;
            }
        }
    }
    let exhaustive = !stopped;

    kept.sort_by_key(|(nodes, _)| nodes.len());
    let list_truncated = kept.len() > limit;
    kept.truncate(limit);

    let transition_ids: Vec<TransitionId> = net.transitions().iter().map(|t| t.id).collect();
    let circuits: Vec<CircuitDto> = kept
        .iter()
        .map(|(nodes, via)| {
            let mut counts = vec![0usize; transition_ids.len()];
            for t in via {
                counts[*t] += 1;
            }
            let mut steps_out: Vec<CheckPathStep> = Vec::with_capacity(nodes.len() + 1);
            for (i, node) in nodes.iter().chain(std::iter::once(&nodes[0])).enumerate() {
                let transition = i.checked_sub(1).map(|k| via[k]);
                steps_out.push(CheckPathStep {
                    marking: marking_to_dto_vec(&graph.nodes[*node].marking),
                    transition,
                    transition_name: transition.and_then(|id| net.transitions().iter().find(|t| t.id.0 as usize == id).map(|t| t.name.clone())),
                });
            }
            let fired = |t: &TransitionId| counts[t.0 as usize] > 0;
            let enabled_at = |t: &TransitionId, node: &usize| net.is_enabled(*t, &graph.nodes[*node].marking);
            CircuitDto {
                length: nodes.len(),
                states: nodes.iter().map(|n| format!("M{n}")).collect(),
                steps: steps_out,
                parikh: net
                    .transitions()
                    .iter()
                    .filter(|t| counts[t.id.0 as usize] > 0)
                    .map(|t| TransitionCount { name: t.name.clone(), count: counts[t.id.0 as usize] })
                    .collect(),
                not_fired: net.transitions().iter().filter(|t| counts[t.id.0 as usize] == 0).map(|t| t.name.clone()).collect(),
                fair: transition_ids.iter().all(|t| !nodes.iter().any(|n| enabled_at(t, n)) || fired(t)),
                state_fair: transition_ids.iter().all(|t| !nodes.iter().all(|n| enabled_at(t, n)) || fired(t)),
            }
        })
        .collect();
    let seen: Vec<usize> = length_counts.iter().enumerate().filter(|(_, c)| **c > 0).map(|(l, _)| l).collect();
    Ok(CircuitsResult {
        state_count: count,
        circuits,
        total_found: total,
        length_counts: if request.mode == "all" { length_counts } else { Vec::new() },
        shortest: seen.first().copied(),
        longest: seen.last().copied(),
        exhaustive,
        list_truncated,
    })
}

/// INA "define an enabledness predicate" (E) and "convert a set of states to a predicate" (C).
///
/// `kind = "enabled"`: a state satisfies it when at least one of `transitions` is enabled
/// (input places hold the arc weights, inhibitor places are below their thresholds).
/// `kind = "states"`: a state satisfies it when it is one of `states` (graph node ids).
#[tauri::command]
async fn build_predicate(
    file_path: String,
    kind: String,
    transitions: Vec<String>,
    states: Vec<usize>,
    config: GraphGenerationConfig,
) -> Result<PredicateResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || build_predicate_impl(file_path, kind, transitions, states, config))
        .await
        .map_err(|error| format!("Falha na tarefa de predicado: {error}"))?
}

/// INA's `.pdc` text. Places are listed from the last to the first, as INA writes them.
fn pdc_text(net: &PetriNet, conjunctions: &[(usize, Vec<(usize, u64, Option<u64>)>)]) -> String {
    let mut out = String::from("predicate for net:   0 BNA\n");
    for (number, bounds) in conjunctions {
        out.push_str(&format!("Conjunction nr: {number:>2}\n"));
        for (place, low, upp) in bounds.iter().rev() {
            let upp = upp.map_or_else(|| "oo".to_string(), |v| v.to_string());
            out.push_str(&format!("m + p{place}.{:<18}low = {low:>2}  upp = {upp}\n", net.places()[*place].name));
        }
    }
    out.push_str("@\n");
    out
}

fn build_predicate_impl(
    file_path: String,
    kind: String,
    transitions: Vec<String>,
    states: Vec<usize>,
    _config: GraphGenerationConfig,
) -> Result<PredicateResult, String> {
    let net = parse_pnt_file(&file_path)?;
    // (number written in the .pdc, label shown, place index, low, upp)
    let mut raw: Vec<(usize, String, Vec<(usize, u64, Option<u64>)>)> = Vec::new();
    match kind.as_str() {
        "enabled" => {
            if transitions.is_empty() {
                return Err("Choose at least one transition".into());
            }
            for name in &transitions {
                let transition = net
                    .transitions()
                    .iter()
                    .find(|t| t.name == *name)
                    .ok_or_else(|| format!("Unknown transition: {name}"))?;
                let mut bounds: std::collections::BTreeMap<usize, (u64, Option<u64>)> = std::collections::BTreeMap::new();
                for &(place, weight) in net.preset(transition.id) {
                    let entry = bounds.entry(place.0 as usize).or_insert((0, None));
                    entry.0 = entry.0.max(u64::from(weight));
                }
                for &(place, threshold) in net.inhibitors(transition.id) {
                    let entry = bounds.entry(place.0 as usize).or_insert((0, None));
                    let cap = threshold.saturating_sub(1);
                    entry.1 = Some(entry.1.map_or(cap, |u| u.min(cap)));
                }
                raw.push((
                    transition.id.0 as usize,
                    name.clone(),
                    bounds.into_iter().map(|(place, (low, upp))| (place, low, upp)).collect(),
                ));
            }
        }
        "states" => {
            if states.is_empty() {
                return Err("Choose at least one state".into());
            }
            if states.len() > 500 {
                return Err("Choose at most 500 states".into());
            }
            let graph = cached_reachability(&file_path, &net)?;
            for (index, &state) in states.iter().enumerate() {
                if state >= graph.node_count() {
                    return Err(format!("State M{state} does not exist: the graph has {} states", graph.node_count()));
                }
                let bounds = graph
                    .nodes[state]
                    .marking
                    .tokens
                    .iter()
                    .enumerate()
                    .map(|(place, token)| {
                        let value = token.as_biguint().and_then(|v| u64::try_from(v).ok()).ok_or("A state holds an unbounded token count")?;
                        Ok((place, value, Some(value)))
                    })
                    .collect::<Result<Vec<_>, &str>>()?;
                raw.push((index + 1, format!("M{state}"), bounds));
            }
        }
        other => return Err(format!("Unknown predicate kind: {other}")),
    }
    let pdc = pdc_text(&net, &raw.iter().map(|(n, _, b)| (*n, b.clone())).collect::<Vec<_>>());
    let conjunctions = raw
        .into_iter()
        .map(|(_, label, bounds)| PredicateConjunction {
            label,
            bounds: bounds
                .into_iter()
                .map(|(place, low, upp)| PredicateBound { place: net.places()[place].name.clone(), low, upp })
                .collect(),
        })
        .collect();
    Ok(PredicateResult { conjunctions, pdc })
}

/// INA "test place- or transition-vectors for invariant properties".
#[tauri::command]
fn test_vector(file_path: String, kind: String, vector: Vec<i64>) -> Result<VectorTestResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let matrix = net.incidence_matrix();
    let (places, transitions) = (matrix.place_count(), matrix.transition_count());
    let place_kind = match kind.as_str() {
        "place" => true,
        "transition" => false,
        other => return Err(format!("Unknown vector kind: {other}")),
    };
    let expected = if place_kind { places } else { transitions };
    if vector.len() != expected {
        return Err(format!("Vector length {} does not match {} ({expected})", vector.len(), kind));
    }
    let v: Vec<BigInt> = vector.iter().map(|x| BigInt::from(*x)).collect();
    let cell = |p: usize, t: usize| matrix.get(p, t).unwrap_or_default();
    let residual: Vec<BigInt> = if place_kind {
        (0..transitions).map(|t| (0..places).map(|p| &v[p] * cell(p, t)).sum()).collect()
    } else {
        (0..places).map(|p| (0..transitions).map(|t| cell(p, t) * &v[t]).sum()).collect()
    };
    let zero = BigInt::default();
    let names: Vec<String> = if place_kind {
        net.places().iter().map(|p| p.name.clone()).collect()
    } else {
        net.transitions().iter().map(|t| t.name.clone()).collect()
    };
    let residual_names: Vec<String> = if place_kind {
        net.transitions().iter().map(|t| t.name.clone()).collect()
    } else {
        net.places().iter().map(|p| p.name.clone()).collect()
    };
    let initial_value = place_kind.then(|| {
        v.iter()
            .zip(net.initial_marking().tokens.iter())
            .try_fold(BigInt::default(), |acc, (y, token)| match token {
                TokenValue::Finite(m) => Some(acc + y * BigInt::from(*m)),
                TokenValue::Exact(m) => Some(acc + y * BigInt::from(m.clone())),
                TokenValue::Omega => None,
            })
            .map(|value| value.to_string())
    });
    Ok(VectorTestResult {
        kind,
        semipositive: v.iter().all(|x| *x >= zero) && v.iter().any(|x| *x != zero),
        positive: !v.is_empty() && v.iter().all(|x| *x > zero),
        invariant: residual.iter().all(|r| *r == zero),
        sub: residual.iter().all(|r| *r <= zero),
        sur: residual.iter().all(|r| *r >= zero),
        support: v.iter().enumerate().filter(|(_, x)| **x != zero).map(|(i, _)| names[i].clone()).collect(),
        initial_value: initial_value.flatten(),
        residual: residual.iter().map(ToString::to_string).collect(),
        names,
        residual_names,
    })
}

// ── Incidence Matrix ───────────────────────────────────────────────────

#[tauri::command]
fn get_incidence_matrix(file_path: String) -> Result<IncidenceMatrixResponse, String> {
    let net = parse_pnt_file(&file_path)?;

    let places: Vec<String> = net.places().iter().map(|p| p.name.clone()).collect();
    let transitions: Vec<String> = net.transitions().iter().map(|t| t.name.clone()).collect();

    let c = net.incidence_matrix();
    let matrix: Vec<Vec<String>> = c
        .data()
        .iter()
        .map(|row| row.iter().map(ToString::to_string).collect())
        .collect();

    Ok(IncidenceMatrixResponse {
        places,
        transitions,
        matrix,
    })
}

// ── Analysis ───────────────────────────────────────────────────────────

#[tauri::command]
async fn analyze_pnt(file_path: String) -> Result<AnalysisResult, String> {
    tauri::async_runtime::spawn_blocking(move || analyze_pnt_impl(file_path))
        .await
        .map_err(|error| format!("Falha na tarefa de análise: {error}"))?
}

fn analyze_pnt_impl(file_path: String) -> Result<AnalysisResult, String> {
    let start = Instant::now();
    let net = parse_pnt_file(&file_path)?;

    let (has_self_loops, self_loops) = find_self_loops(&net);
    let structural_properties = StructuralAnalyzer::analyze_basic(&net);
    let structural = structural_result_from_properties(&net, &structural_properties);

    Ok(AnalysisResult {
        initial_marking: marking_to_dto_vec(net.initial_marking()),
        bad_state_safe: None,
        bounded: None,
        safe: None,
        live: None,
        place_live: None,
        exact_graph_complete: false,
        analysis_status: "structural_analysis_complete".into(),
        analysis_message: Some("Análise estrutural concluída. As propriedades que dependem do espaço de estados não foram calculadas nesta etapa.".into()),
        unbounded_places: Vec::new(),
        deadlock_free: None,
        deadlock_nodes: Vec::new(),
        dead_transitions: None,
        reversible: None,
        non_reversible_nodes: Vec::new(),
        dynamic_conflict_free: None,
        persistent: None,
        gfc: None,
        deadlock_trap: None,
        liveness: Vec::new(),
        place_liveness: Vec::new(),
        dead_places: Vec::new(),
        has_self_loops,
        self_loops,
        structural: Some(structural),
        invariant: None,
        analysis_info: AnalysisInfo {
            num_states: None,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        },
        net_places: net.place_count(),
        net_transitions: net.transition_count(),
        total_arcs: net.arcs().len(),
        total_tokens: total_tokens_dto(net.initial_marking()),
        evidence: Vec::new(),
    })
}

fn required_initial_properties_impl(file_path: String) -> Result<RequiredInitialProperties, String> {
    let net = parse_pnt_file(&file_path)?;
    let config = GraphGenerationConfig::default();
    let context = analysis_context_for_graph(config);
    let invariant_engine = InvariantEngineImpl::default();
    let p_semiflows = invariant_engine.compute_p_semiflows(&net, &context).ok();
    let conservative = p_semiflows.as_ref().map(|flows| {
        flows.iter().any(|flow| {
            flow.coefficients
                .iter()
                .all(|coefficient| coefficient > &BigInt::from(0))
        })
    });
    let cpi = p_semiflows.as_ref().map(|flows| {
        (0..net.place_count()).all(|place| {
            flows.iter().any(|flow| {
                flow.coefficients
                    .get(place)
                    .is_some_and(|coefficient| coefficient > &BigInt::from(0))
            })
        })
    });
    let structural_certificate = structural_boundedness_certificate(&net);
    let structurally_bounded = if cpi == Some(true)
        || conservative == Some(true)
        || structural_certificate.is_some()
    {
        Some(true)
    } else {
        None
    };

    // Starke Satz 4.3: decide boundedness with early exit (halts at the first
    // marking that strictly covers an ancestor) instead of building the full
    // Karp-Miller graph. Only an inconclusive answer falls back to it.
    let verdict = match structurally_bounded {
        Some(true) => None,
        _ => racon_coverability::decide_boundedness(
            &net,
            GraphGenerationConfig::default().max_nodes,
            &context,
        )
        .ok(),
    };
    // The full Karp-Miller graph and the reachability graph are never built here: they come from
    // the cache, which the Graphs tab fills when the user generates the graph.
    let bounded = match (structurally_bounded, verdict.and_then(|v| v.as_bool())) {
        (Some(true), _) => Some(true),
        (_, Some(value)) => Some(value),
        _ => cached_coverability(&file_path, &net).ok().and_then(|graph| graph.bounded_status()),
    };
    let reachability = if bounded == Some(false) { None } else { cached_reachability(&file_path, &net).ok() };
    // An unbounded net is not structurally bounded either (SB implies B for every M0).
    let structurally_bounded = if bounded == Some(false) { Some(false) } else { structurally_bounded };
    // A fully explored finite reachability graph is itself a proof of
    // boundedness; this is the only conclusive route for extended nets.
    let bounded = match (&reachability, bounded) {
        (Some(graph), None) if graph.is_exact() => Some(true),
        (_, other) => other,
    };
    let state_count = match &reachability {
        Some(graph) if graph.is_exact() => Some(graph.node_count()),
        _ => None,
    };
    let (live, safe, reachability_warning) = match &reachability {
        // Unbounded => not 1-bounded, hence not safe and not live-and-safe; only L stays undecided.
        None if bounded == Some(false) => (None, Some(false), Some("A rede é ilimitada: o grafo de alcançabilidade é infinito, então L não é calculado.".into())),
        Some(graph) if graph.is_exact() => {
            let live = ExpertSystem::is_live(&net, graph);
            let safe = (0..graph.node_count()).all(|index| {
                graph
                    .marking(NodeId(index as u32))
                    .tokens
                    .iter()
                    .all(|token| matches!(token, TokenValue::Finite(value) if *value <= 1))
            });
            (Some(live), Some(safe), None)
        }
        Some(_) => (None, None, Some("O espaço de estados não é exato; L e L&S continuam sem resultado.".into())),
        // No graph yet: L, S and L&S wait for the Graphs tab.
        None => (None, None, None),
    };
    let live_and_safe = match (live, safe) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(is_safe)) => Some(is_safe),
        _ => None,
    };
    let mut warnings = Vec::new();
    if p_semiflows.is_none() {
        warnings.push("Não foi possível concluir CSV com os P-invariantes.".to_string());
    }
    if structurally_bounded.is_none() {
        warnings.push("Não foi possível concluir SB.".to_string());
    }
    if let Some(warning) = reachability_warning {
        warnings.push(warning);
    }
    if bounded.is_none() {
        warnings.push(if net.is_extended() {
            "Rede com arcos inibidores/transições imediatas: B só é decidido por certificado estrutural ou por exploração exata completa (UNKNOWN).".to_string()
        } else {
            "Não foi possível concluir B pelo certificado estrutural nem pela árvore de cobertura.".to_string()
        });
    }
    let warning = (!warnings.is_empty()).then(|| warnings.join(" "));

    Ok(RequiredInitialProperties {
        conservative,
        structurally_bounded,
        bounded,
        live,
        live_and_safe,
        state_count,
        warning,
    })
}

#[tauri::command]
async fn get_required_initial_properties(file_path: String) -> Result<RequiredInitialProperties, String> {
    tauri::async_runtime::spawn_blocking(move || required_initial_properties_impl(file_path))
        .await
        .map_err(|error| format!("Falha na tarefa de propriedades iniciais: {error}"))?
}

/// Validate an explicitly selected periodic execution against the fairness
/// definitions. Nodes must form a closed path and transitions must label its
/// consecutive edges.
#[tauri::command]
async fn check_cycle_fairness(
    file_path: String,
    nodes: Vec<usize>,
    transitions: Vec<usize>,
    config: GraphGenerationConfig,
) -> Result<FairnessResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        check_cycle_fairness_impl(file_path, nodes, transitions, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de análise do ciclo: {error}"))?
}

fn check_cycle_fairness_impl(
    file_path: String,
    nodes: Vec<usize>,
    transitions: Vec<usize>,
    _config: GraphGenerationConfig,
) -> Result<FairnessResult, String> {
    if nodes.len() < 2 || nodes.len() != transitions.len() + 1 || nodes.first() != nodes.last() {
        return Err("cycle must be closed and contain one transition per edge".into());
    }
    let net = parse_pnt_file(&file_path)?;
    let graph = cached_reachability(&file_path, &net)?;
    let mut fired = HashSet::new();
    for i in 0..transitions.len() {
        let source = NodeId(nodes[i] as u32);
        let target = NodeId(nodes[i + 1] as u32);
        if source.0 as usize >= graph.node_count() || target.0 as usize >= graph.node_count() {
            return Err("cycle references an invalid state".into());
        }
        let transition = TransitionId(transitions[i] as u32);
        if !graph.outgoing_edges(source).iter().any(|&(to, label)| to == target && label == transition) {
            return Err("cycle contains an edge not present in the reachability graph".into());
        }
        fired.insert(transition);
    }
    let fair = net.transitions().iter().all(|t| {
        let enabled_somewhere = (0..transitions.len()).any(|i| net.is_enabled(t.id, graph.marking(NodeId(nodes[i] as u32))));
        !enabled_somewhere || fired.contains(&t.id)
    });
    let state_fair = net.transitions().iter().all(|t| {
        let enabled_everywhere = (0..transitions.len()).all(|i| net.is_enabled(t.id, graph.marking(NodeId(nodes[i] as u32))));
        !enabled_everywhere || fired.contains(&t.id)
    });
    Ok(FairnessResult { fair, state_fair })
}

// ── Reachability / Coverability Graphs ─────────────────────────────────

fn log_graph_event(app: &AppHandle, level: &str, event: &str, message: &str) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let _ = write_log_entry(
        app.clone(),
        LogEntry {
            timestamp,
            level: level.into(),
            source: "analysis".into(),
            event: event.into(),
            message: message.into(),
        },
    );
}

#[tauri::command]
async fn get_reachability_graph(
    app: AppHandle,
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<ReachabilityGraphResponse, String> {
    let config = validate_graph_config(config)?;
    log_graph_event(&app, "INFO", "reachability.started", "Geração solicitada pelo usuário");
    let result = tauri::async_runtime::spawn_blocking(move || {
        get_reachability_graph_impl(file_path, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de alcançabilidade: {error}"))?;
    match result {
        Ok(graph) => {
            log_graph_event(
                &app,
                "INFO",
                "reachability.completed",
                &format!("{} estados e {} arestas", graph.nodes.len(), graph.edges.len()),
            );
            Ok(graph)
        }
        Err(error) => {
            log_graph_event(&app, "ERROR", "reachability.failed", &error);
            Err(error)
        }
    }
}

fn get_reachability_graph_impl(
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<ReachabilityGraphResponse, String> {
    let net = parse_pnt_file(&file_path)?;
    let ctx = analysis_context_for_graph(config);
    let engine = ReachabilityEngineImpl {
        config: ReachabilityConfig {
            max_nodes: config.max_nodes,
            max_edges: config.max_edges,
            ..ReachabilityConfig::default()
        },
    };
    ensure_algorithm_allowed(
        &net,
        racon_reduction::catalog::Objective::Reachability,
        config.algorithm,
        config.stubborn,
        config.symmetry,
    )?;
    // (markings, labelled edges) of the chosen exploration, in discovery order.
    let (markings, raw_edges): (Vec<Marking>, Vec<(usize, usize, usize)>) = if config.stubborn || config.symmetry {
        // Experimental reductions; what each preserves is in the catalogue
        // (partial_order_stubborn, symmetry_reduction).
        let mut kinds = Vec::new();
        if config.symmetry {
            kinds.push(racon_reduction::ReductionKind::Symmetry);
        }
        if config.stubborn {
            kinds.push(racon_reduction::ReductionKind::Stubborn);
        }
        let reduced = racon_reduction::ReducedReachabilityGraph::build(
            &net,
            &kinds,
            &racon_reduction::SymmetryOptions::default(),
            racon_reduction::ReducedExplorationConfig {
                max_nodes: config.max_nodes,
                ..racon_reduction::ReducedExplorationConfig::default()
            },
            &ctx,
        )
        .map_err(|e| format!("Geração do grafo reduzido interrompida: {e}"))?;
        let markings = reduced.nodes().iter().map(|n| n.marking.clone()).collect();
        let mut edges = Vec::new();
        for index in 0..reduced.node_count() {
            for (target, transition) in reduced.outgoing_edges(NodeId(index as u32)) {
                edges.push((index, target.0 as usize, transition.0 as usize));
            }
        }
        (markings, edges)
    } else {
        // One exploration per net: a saved graph is reused, a new one is saved when it is complete.
        let graph: Arc<ReachabilityGraph> = match cached_reachability(&file_path, &net) {
            Ok(saved) => saved,
            Err(_) => {
                let built = match config.algorithm {
                    GraphAlgorithm::ReachabilityDfs => engine.build_complete_dfs(&net, &ctx),
                    _ => engine.build_complete(&net, &ctx),
                }
                .map_err(|e| format!("Geração do grafo de alcançabilidade interrompida: {e}"))?;
                if built.is_exact() {
                    graph_cache::store_reachability(&file_path, &built)?;
                }
                Arc::new(built)
            }
        };
        (
            graph.nodes.iter().map(|n| n.marking.clone()).collect(),
            graph
                .edges
                .iter()
                .map(|e| (e.source.0 as usize, e.target.0 as usize, e.transition.0 as usize))
                .collect(),
        )
    };

    const DISPLAY_NODE_LIMIT: usize = 1_000;
    const DISPLAY_EDGE_LIMIT: usize = 5_000;
    let total_nodes = markings.len();
    let total_edges = raw_edges.len();
    let nodes: Vec<GraphNode> = markings
        .iter()
        .take(DISPLAY_NODE_LIMIT)
        .enumerate()
        .map(|(i, marking)| GraphNode {
            id: i,
            marking: marking_to_dto_vec(marking),
            is_omega: false,
        })
        .collect();
    let edges: Vec<GraphEdge> = raw_edges
        .iter()
        .filter(|(source, target, _)| *source < DISPLAY_NODE_LIMIT && *target < DISPLAY_NODE_LIMIT)
        .take(DISPLAY_EDGE_LIMIT)
        .map(|&(source, target, transition)| GraphEdge {
            source,
            target,
            transition,
            is_abstract: false,
        })
        .collect();
    let display_truncated = nodes.len() < total_nodes || edges.len() < total_edges;
    Ok(ReachabilityGraphResponse {
        nodes,
        edges,
        total_nodes,
        total_edges,
        display_truncated,
    })
}

/// The graph is computed completely before pagination. Pagination only limits
/// the amount transported/rendered by the frontend and never the analysis.
#[tauri::command]
async fn get_reachability_graph_page(
    app: AppHandle,
    file_path: String,
    offset: usize,
    limit: usize,
    config: GraphGenerationConfig,
) -> Result<ReachabilityGraphPageResponse, String> {
    let full = get_reachability_graph(app, file_path, config).await?;
    let total_nodes = full.total_nodes;
    let total_edges = full.total_edges;
    let displayed_nodes = full.nodes.len();
    let safe_limit = limit.min(5_000);
    let end = offset.saturating_add(safe_limit).min(displayed_nodes);
    let visible_ids: std::collections::HashSet<usize> =
        (offset.min(total_nodes)..end).collect();
    let nodes = full
        .nodes
        .into_iter()
        .skip(offset.min(end))
        .take(end.saturating_sub(offset.min(end)))
        .collect();
    let edges = full
        .edges
        .into_iter()
        .filter(|edge| visible_ids.contains(&edge.source) && visible_ids.contains(&edge.target))
        .collect();
    Ok(ReachabilityGraphPageResponse {
        nodes,
        edges,
        offset,
        limit: safe_limit,
        total_nodes,
        total_edges,
        complete: end >= displayed_nodes && !full.display_truncated,
    })
}

#[tauri::command]
async fn get_coverage_tree(
    app: AppHandle,
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<CoverageTreeResponse, String> {
    let config = validate_graph_config(config)?;
    log_graph_event(&app, "INFO", "coverability.started", "Geração solicitada pelo usuário");
    let result = tauri::async_runtime::spawn_blocking(move || {
        get_coverage_tree_impl(file_path, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de cobertura: {error}"))?;
    match result {
        Ok(graph) => {
            log_graph_event(
                &app,
                "INFO",
                "coverability.completed",
                &format!("{} estados e {} arestas", graph.nodes.len(), graph.edges.len()),
            );
            Ok(graph)
        }
        Err(error) => {
            log_graph_event(&app, "ERROR", "coverability.failed", &error);
            Err(error)
        }
    }
}

/// Karp-Miller/Finkel acceleration needs a monotone firing relation, which
/// inhibitor arcs and immediate-transition precedence break (spec §5.3).
const UNSUPPORTED_EXTENDED_COVERABILITY: &str = "UNSUPPORTED: a árvore de cobertura (Karp–Miller/Finkel) não é válida para redes com arcos inibidores ou transições imediatas, pois a monotonicidade falha. Use o grafo de alcançabilidade exato (limitado por recursos).";

fn get_coverage_tree_impl(
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<CoverageTreeResponse, String> {
    let net = parse_pnt_file(&file_path)?;
    if net.is_extended() {
        return Err(UNSUPPORTED_EXTENDED_COVERABILITY.to_string());
    }
    let ctx = analysis_context_for_graph(config);
    let engine = CoverabilityEngineImpl {
        config: CoverabilityConfig {
            max_nodes: config.max_nodes,
            max_edges: config.max_edges,
        },
    };
    ensure_algorithm_allowed(&net, racon_reduction::catalog::Objective::Coverability, config.algorithm, config.stubborn, false)?;
    if config.algorithm == GraphAlgorithm::CoverabilityMincov {
        return Err("O MinCov produz o conjunto mínimo de cobertura, não uma árvore; use o comando do conjunto de cobertura.".into());
    }
    let strategy = match config.algorithm {
        GraphAlgorithm::CoverabilityFinkelLegacy => CoverabilityStrategy::FinkelMinimalCoverability,
        _ => CoverabilityStrategy::KarpMiller,
    };
    // One construction per net: the Karp-Miller tree is saved and reused.
    let graph: Arc<racon_coverability::CoverabilityGraph> = match (strategy, cached_coverability(&file_path, &net)) {
        (CoverabilityStrategy::KarpMiller, Ok(saved)) => saved,
        _ => {
            let built = engine
                .build_complete(&net, strategy, &ctx)
                .map_err(|e| format!("Geração da árvore de cobertura interrompida: {e}"))?;
            if strategy == CoverabilityStrategy::KarpMiller {
                graph_cache::store_coverability(&file_path, &built)?;
            }
            Arc::new(built)
        }
    };

    const DISPLAY_NODE_LIMIT: usize = 1_000;
    const DISPLAY_EDGE_LIMIT: usize = 5_000;
    let total_nodes = graph.nodes.iter().filter(|n| n.is_active).count();
    let total_edges = graph.edges.len();
    let nodes: Vec<GraphNode> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.is_active)
        .take(DISPLAY_NODE_LIMIT)
        .map(|(i, n)| GraphNode {
            id: i,
            marking: marking_to_dto_vec(&n.marking),
            is_omega: n.has_omega,
        })
        .collect();
    let edges: Vec<GraphEdge> = graph
        .edges
        .iter()
        .filter(|e| (e.source.0 as usize) < DISPLAY_NODE_LIMIT && (e.target.0 as usize) < DISPLAY_NODE_LIMIT)
        .take(DISPLAY_EDGE_LIMIT)
        .map(|e| GraphEdge {
            source: e.source.0 as usize,
            target: e.target.0 as usize,
            transition: e.transition.0 as usize,
            is_abstract: e.is_abstract,
        })
        .collect();
    let display_truncated = nodes.len() < total_nodes || edges.len() < total_edges;
    Ok(CoverageTreeResponse {
        nodes,
        edges,
        is_bounded: graph.bounded_status(),
        max_states_exceeded: false,
        display_truncated,
        total_nodes,
        total_edges,
    })
}

// ── Structural Properties ──────────────────────────────────────────────

#[tauri::command]
async fn get_structural_properties(file_path: String) -> Result<StructuralResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        Ok(structural_to_result(&net))
    })
    .await
    .map_err(|error| format!("Falha na tarefa de propriedades estruturais: {error}"))?
}

fn parse_bad_state_operator(operator: &str) -> Result<ComparisonOp, String> {
    match operator.trim() {
        "=" | "==" => Ok(ComparisonOp::Eq),
        "!=" | "<>" => Ok(ComparisonOp::Neq),
        "<" => Ok(ComparisonOp::Lt),
        "<=" => Ok(ComparisonOp::Le),
        ">" => Ok(ComparisonOp::Gt),
        ">=" => Ok(ComparisonOp::Ge),
        other => Err(format!("unsupported Bad State operator: {other}")),
    }
}

/// INA-style Bad State check. P-invariants are used as a cheap proof layer;
/// only formulas not refuted algebraically are sent to exact reachability.
#[tauri::command]
async fn check_bad_states(
    file_path: String,
    definitions: Vec<BadStateInput>,
    config: GraphGenerationConfig,
) -> Result<BadStateCheckResponse, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        check_bad_states_impl(file_path, definitions, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de Bad States: {error}"))?
}

fn check_bad_states_impl(
    file_path: String,
    definitions: Vec<BadStateInput>,
    config: GraphGenerationConfig,
) -> Result<BadStateCheckResponse, String> {
    let net = parse_pnt_file(&file_path)?;
    let ctx = analysis_context_for_graph(config);
    let p_invariants = InvariantEngineImpl::default()
        .compute_p_semiflows(&net, &ctx)
        .map_err(|e| e.to_string())?;
    let property_definitions: Vec<BadStateDefinition> = definitions.iter().map(|definition| {
        let operator = parse_bad_state_operator(&definition.operator)?;
        let terms = definition.terms.iter().map(|term| {
            if term.place >= net.place_count() { return Err(format!("invalid place id: {}", term.place)); }
            Ok((PlaceId(term.place as u32), term.coefficient.into()))
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(BadStateDefinition {
            id: definition.id.clone(),
            description: definition.description.clone(),
            predicate: StatePredicate::LinearConstraint { terms, op: operator, constant: definition.constant.into() },
        })
    }).collect::<Result<_, String>>()?;
    let algebraic = PropertyEngineImpl::default()
        .analyze_bad_states_algebraic(&net, &property_definitions, &p_invariants, &ctx)
        .map_err(|e| e.to_string())?;

    if config.structural_rules && net.has_immediate_transitions() {
        return Err("UNSUPPORTED: as regras estruturais A-I não cobrem transições imediatas.".into());
    }
    if config.stubborn {
        ensure_algorithm_allowed(&net, racon_reduction::catalog::Objective::Reachability, config.algorithm, true, false)?;
    }
    if config.stubborn || config.structural_rules {
        return Ok(check_bad_states_stubborn(&net, &definitions, algebraic, config, &ctx));
    }
    let needs_exact = algebraic.iter().any(|result| !result.proven_unreachable);
    // Exact check on the saved graph; without one, only the P-invariant proofs apply.
    let exact_report = needs_exact
        .then(|| cached_reachability(&file_path, &net).ok()
            .and_then(|graph| PropertyEngineImpl::default().evaluate_bad_states(&net, &property_definitions, &graph, &ctx).ok()))
        .flatten();

    let mut safe = Some(true);
    let results = algebraic.into_iter().map(|algebraic_result| {
        if algebraic_result.proven_unreachable {
            return BadStateCheckEntry { id: algebraic_result.definition_id, status: "unreachable_by_p_invariants".into(), certificate: algebraic_result.certificate };
        }
        if let Some(report) = &exact_report {
            if let Some(violation) = report.violations.iter().find(|v| v.definition_id == algebraic_result.definition_id) {
                if violation.reached {
                    safe = Some(false);
                    return BadStateCheckEntry { id: algebraic_result.definition_id, status: "reachable".into(), certificate: None };
                }
                return BadStateCheckEntry { id: algebraic_result.definition_id, status: "unreachable_exactly".into(), certificate: Some("Nenhuma marcação alcançável satisfaz o Bad State.".into()) };
            }
        }
        if safe != Some(false) {
            safe = None;
        }
        BadStateCheckEntry { id: algebraic_result.definition_id, status: "possibly_reachable".into(), certificate: algebraic_result.certificate }
    }).collect();
    Ok(BadStateCheckResponse { safe, results })
}

/// Bad-state check through goal-directed stubborn search: each formula not
/// refuted by P-invariants is searched on its own reduced graph.
fn check_bad_states_stubborn(
    net: &PetriNet,
    definitions: &[BadStateInput],
    algebraic: Vec<racon_property::BadStateAlgebraicResult>,
    config: GraphGenerationConfig,
    ctx: &AnalysisContext,
) -> BadStateCheckResponse {
    use racon_reduction::{goal_places, reduce_for_query, search_goal, GoalOp, GoalSearchConfig, GoalSearchOutcome, LinearConstraint, QueryKind, ReachGoal};
    let mut safe = Some(true);
    let results = algebraic.into_iter().map(|result| {
        if result.proven_unreachable {
            return BadStateCheckEntry { id: result.definition_id, status: "unreachable_by_p_invariants".into(), certificate: result.certificate };
        }
        let goal = definitions.iter().find(|d| d.id == result.definition_id).and_then(|d| {
            let terms: Vec<_> = d.terms.iter().map(|t| (PlaceId(t.place as u32), i64::from(t.coefficient))).collect();
            let make = |op, constant| LinearConstraint { terms: terms.clone(), op, constant };
            match d.operator.trim() {
                "=" | "==" => Some(ReachGoal::single(make(GoalOp::Eq, d.constant))),
                ">=" => Some(ReachGoal::single(make(GoalOp::Ge, d.constant))),
                "<=" => Some(ReachGoal::single(make(GoalOp::Le, d.constant))),
                ">" => Some(ReachGoal::single(make(GoalOp::Ge, d.constant + 1))),
                "<" => Some(ReachGoal::single(make(GoalOp::Le, d.constant - 1))),
                "!=" | "<>" => Some(ReachGoal { disjuncts: vec![vec![make(GoalOp::Le, d.constant - 1)], vec![make(GoalOp::Ge, d.constant + 1)]] }),
                _ => None,
            }
        });
        let outcome = goal.and_then(|goal| {
            let search = GoalSearchConfig { max_nodes: config.max_nodes, use_stubborn: config.stubborn };
            if config.structural_rules {
                // places(φ) are protected; the reduced net answers this query only.
                let reduction = reduce_for_query(net, &goal_places(&goal), QueryKind::Reachability).ok()?;
                let translated = reduction.translate_goal(&goal)?;
                let search = GoalSearchConfig { use_stubborn: search.use_stubborn && !reduction.net.is_extended(), ..search };
                return search_goal(&reduction.net, &translated, search, ctx).ok();
            }
            search_goal(net, &goal, search, ctx).ok()
        });
        match outcome.map(|report| report.outcome) {
            Some(GoalSearchOutcome::Reachable { .. }) => {
                safe = Some(false);
                BadStateCheckEntry { id: result.definition_id, status: "reachable".into(), certificate: None }
            }
            Some(GoalSearchOutcome::Unreachable) => BadStateCheckEntry {
                id: result.definition_id,
                status: "unreachable_exactly".into(),
                certificate: Some("Nenhuma marcação satisfaz o Bad State no grafo reduzido após as reduções selecionadas (stubborn orientado à propriedade, Schmidt 1999 Teor. 4; regras A-I, Bønneland et al. 2019).".into()),
            },
            None => {
                if safe != Some(false) {
                    safe = None;
                }
                BadStateCheckEntry { id: result.definition_id, status: "possibly_reachable".into(), certificate: result.certificate }
            }
        }
    }).collect();
    BadStateCheckResponse { safe, results }
}

#[derive(Serialize)]
pub struct PlaceBoundednessResult {
    pub place: String,
    /// `None` when the node budget was exhausted: UNKNOWN, never a verdict.
    pub bounded: Option<bool>,
    pub visited: usize,
}

/// Boundedness of one place through the stubborn-set Karp–Miller tree
/// (Schmidt 1999, §6, Thm 2). Plain P/T nets only.
#[tauri::command]
async fn check_place_boundedness(
    file_path: String,
    place: usize,
    config: GraphGenerationConfig,
) -> Result<PlaceBoundednessResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        use racon_reduction::{decide_boundedness_stubborn_omega, BoundSearchConfig, BoundTarget, OmegaVerdict, ReductionError};
        let net = parse_pnt_file(&file_path)?;
        if place >= net.place_count() {
            return Err(format!("invalid place id: {place}"));
        }
        let name = net.places()[place].name.clone();
        let ctx = analysis_context_for_graph(config);
        match decide_boundedness_stubborn_omega(
            &net,
            BoundTarget::Place(PlaceId(place as u32)),
            BoundSearchConfig { max_nodes: config.max_nodes, use_stubborn: true },
            &ctx,
        ) {
            Ok(report) => Ok(PlaceBoundednessResult { place: name, bounded: Some(report.verdict == OmegaVerdict::Bounded), visited: report.visited }),
            Err(ReductionError::LimitExceeded { limit }) => Ok(PlaceBoundednessResult { place: name, bounded: None, visited: limit }),
            Err(error) => Err(format!("UNSUPPORTED: {error}")),
        }
    })
    .await
    .map_err(|error| format!("Falha na tarefa de limitação por lugar: {error}"))?
}

#[derive(Serialize)]
pub struct DeadTransitionResult {
    pub transition: String,
    /// `Some(true)` dead, `Some(false)` can fire, `None` budget exhausted (UNKNOWN).
    pub dead: Option<bool>,
    /// Shortest firing sequence that enables the transition (transition names).
    pub path: Vec<String>,
    pub visited: usize,
}

/// Dead-transition test through stubborn sets with attractor `{t}`
/// (Schmidt 1999, Thm 6). Plain P/T nets only.
#[tauri::command]
async fn check_transition_dead(
    file_path: String,
    transition: usize,
    config: GraphGenerationConfig,
) -> Result<DeadTransitionResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        use racon_reduction::{decide_dead_transition, DeadSearchConfig, DeadVerdict, ReductionError};
        let net = parse_pnt_file(&file_path)?;
        if transition >= net.transition_count() {
            return Err(format!("invalid transition id: {transition}"));
        }
        let name = net.transitions()[transition].name.clone();
        let ctx = analysis_context_for_graph(config);
        match decide_dead_transition(
            &net,
            TransitionId(transition as u32),
            DeadSearchConfig { max_nodes: config.max_nodes, use_stubborn: true },
            &ctx,
        ) {
            Ok(report) => {
                let (dead, path) = match report.verdict {
                    DeadVerdict::Dead => (Some(true), Vec::new()),
                    DeadVerdict::Live { path } => (
                        Some(false),
                        path.iter().map(|t| net.transitions()[t.0 as usize].name.clone()).collect(),
                    ),
                };
                Ok(DeadTransitionResult { transition: name, dead, path, visited: report.visited })
            }
            Err(ReductionError::LimitExceeded { limit }) => Ok(DeadTransitionResult { transition: name, dead: None, path: Vec::new(), visited: limit }),
            Err(error) => Err(format!("UNSUPPORTED: {error}")),
        }
    })
    .await
    .map_err(|error| format!("Falha na tarefa de transição morta: {error}"))?
}

// ── Invariants ─────────────────────────────────────────────────────────

#[tauri::command]
async fn get_invariants(file_path: String) -> Result<InvariantResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        compute_invariant_result(&net)
    })
    .await
    .map_err(|error| format!("Falha na tarefa de invariantes: {error}"))?
}

#[tauri::command]
async fn get_analysis_evidence(file_path: String) -> Result<Vec<MathematicalEvidence>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        let structural = StructuralAnalyzer::analyze_basic(&net);
        let Some(invariants) = compute_invariant_analysis(&net).ok() else {
            return Ok(Vec::new());
        };
        // Behavioural evidence (B, DSt, DTr, REV, L, L&S, DCF) needs real state-space data.
        // It is read from the saved graphs; before they are generated it stays inconclusive.
        let coverability = if net.is_extended() { None } else { cached_coverability(&file_path, &net).ok() };
        let graph = cached_reachability(&file_path, &net).ok();
        Ok(build_all_evidence(&net, &structural, &invariants, graph.as_deref(), coverability.as_deref()))
    })
    .await
    .map_err(|error| format!("Falha na tarefa de evidências: {error}"))?
}

// ── CTL Model Checker ──────────────────────────────────────────────────

#[tauri::command]
async fn check_ctl_formula(
    file_path: String,
    formula: String,
    config: GraphGenerationConfig,
) -> Result<CTLCheckResult, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        check_ctl_formula_impl(file_path, formula, config)
    })
    .await
    .map_err(|error| format!("Falha na tarefa CTL: {error}"))?
}

fn check_ctl_formula_impl(
    file_path: String,
    formula: String,
    config: GraphGenerationConfig,
) -> Result<CTLCheckResult, String> {
    let start = Instant::now();
    let net = parse_pnt_file(&file_path)?;
    let ctx = analysis_context_for_graph(config);

    // 1. Parse the CTL formula
    let ctl_formula = parse_ctl_query(&formula, &net)
        .map_err(|e| format!("Formula parse error: {}", e))?;

    // 2. CTL is defined over the exact reachable markings. A coverability
    // abstraction must not be silently substituted for this graph.
    let graph = cached_reachability(&file_path, &net)?;

    // 3. Evaluate the CTL formula
    let ctl_engine = CTLEngineImpl {
        config: CTLConfig {
            max_iterations: 1_000_000,
            extract_witness: true,
        },
    };

    let result = ctl_engine
        .evaluate(&ctl_formula, &graph, &ctx)
        .map_err(|e| format!("CTL evaluation failed: {}", e))?;

    let elapsed_ms = start.elapsed().as_millis() as f64;

    let mut satisfying: Vec<usize> = result.satisfying_states.iter().map(|node| node.0 as usize).collect();
    satisfying.sort_unstable();
    let states_truncated = satisfying.len() > CTL_LISTED_STATES;
    let states = satisfying
        .iter()
        .take(CTL_LISTED_STATES)
        .map(|&id| CTLStateEntry { id, marking: marking_to_dto_vec(&graph.nodes[id].marking) })
        .collect();

    Ok(CTLCheckResult {
        states,
        states_truncated,
        place_names: net.places().iter().map(|p| p.name.clone()).collect(),
        satisfied: result.satisfied,
        count: result.satisfying_states.len(),
        total_states: result.total_states,
        time: elapsed_ms,
        formula: format_ctl_formula(&result.formula, &net),
        exact_state_space: graph.is_exact(),
        witness: result.witness.map(|path| CTLPathResult {
            steps: ctl_path_steps(&graph, &net, &path.nodes, &path.transitions),
            place_names: net.places().iter().map(|p| p.name.clone()).collect(),
            description: path.description,
            nodes: path.nodes.into_iter().map(|node| node.0 as usize).collect(),
            transitions: path
                .transitions
                .into_iter()
                .map(|transition| transition.0 as usize)
                .collect(),
            violation_point: None,
            is_infinite: path.is_infinite,
            cycle_start: path.cycle_start,
        }),
        counterexample: result.counterexample.map(|path| CTLPathResult {
            steps: ctl_path_steps(&graph, &net, &path.nodes, &path.transitions),
            place_names: net.places().iter().map(|p| p.name.clone()).collect(),
            description: path.description,
            nodes: path.nodes.into_iter().map(|node| node.0 as usize).collect(),
            transitions: path
                .transitions
                .into_iter()
                .map(|transition| transition.0 as usize)
                .collect(),
            violation_point: Some(path.violation_point.0 as usize),
            is_infinite: path.is_infinite,
            cycle_start: None,
        }),
    })
}

/// Steps of a CTL witness / counterexample: the marking at each node and the
/// named transition fired to reach it (transition `i` leads to node `i + 1`).
fn ctl_path_steps(
    graph: &racon_reachability::ReachabilityGraph,
    net: &PetriNet,
    nodes: &[NodeId],
    transitions: &[TransitionId],
) -> Vec<CheckPathStep> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let transition = i.checked_sub(1).and_then(|k| transitions.get(k)).map(|t| t.0 as usize);
            let transition_name = transition.and_then(|id| {
                net.transitions()
                    .iter()
                    .find(|candidate| candidate.id.0 as usize == id)
                    .map(|candidate| candidate.name.clone())
            });
            CheckPathStep {
                marking: marking_to_dto_vec(&graph.nodes[node.0 as usize].marking),
                transition,
                transition_name,
            }
        })
        .collect()
}

fn format_ctl_formula(formula: &CTLFormula, net: &PetriNet) -> String {
    match formula {
        CTLFormula::True => "TRUE".into(),
        CTLFormula::False => "FALSE".into(),
        CTLFormula::AtomicProposition(predicate) => format_atomic_predicate(predicate, net),
        CTLFormula::Not(inner) => format!("!({})", format_ctl_formula(inner, net)),
        CTLFormula::And(left, right) => format!(
            "({}) AND ({})",
            format_ctl_formula(left, net),
            format_ctl_formula(right, net)
        ),
        CTLFormula::Or(left, right) => format!(
            "({}) OR ({})",
            format_ctl_formula(left, net),
            format_ctl_formula(right, net)
        ),
        CTLFormula::Implies(left, right) => format!(
            "({}) -> ({})",
            format_ctl_formula(left, net),
            format_ctl_formula(right, net)
        ),
        CTLFormula::EX(inner) => format!("EX({})", format_ctl_formula(inner, net)),
        CTLFormula::AX(inner) => format!("AX({})", format_ctl_formula(inner, net)),
        CTLFormula::EF(inner) => format!("EF({})", format_ctl_formula(inner, net)),
        CTLFormula::AF(inner) => format!("AF({})", format_ctl_formula(inner, net)),
        CTLFormula::EG(inner) => format!("EG({})", format_ctl_formula(inner, net)),
        CTLFormula::AG(inner) => format!("AG({})", format_ctl_formula(inner, net)),
        CTLFormula::EU(left, right) => format!(
            "E[({}) U ({})]",
            format_ctl_formula(left, net),
            format_ctl_formula(right, net)
        ),
        CTLFormula::AU(left, right) => format!(
            "A[({}) U ({})]",
            format_ctl_formula(left, net),
            format_ctl_formula(right, net)
        ),
    }
}

fn format_atomic_predicate(predicate: &AtomicPredicate, net: &PetriNet) -> String {
    let terms = predicate
        .terms
        .iter()
        .map(|term| {
            let name = net
                .places()
                .get(term.place_id.0 as usize)
                .map(|place| place.name.as_str())
                .unwrap_or_else(|| "p?");
            match term.coefficient {
                1 => name.to_string(),
                -1 => format!("-{name}"),
                coefficient => format!("{coefficient}*{name}"),
            }
        })
        .collect::<Vec<_>>()
        .join(" + ");
    let operator = match predicate.operator {
        ComparisonOp::Eq => "=",
        ComparisonOp::Neq => "!=",
        ComparisonOp::Lt => "<",
        ComparisonOp::Le => "<=",
        ComparisonOp::Gt => ">",
        ComparisonOp::Ge => ">=",
    };
    format!("{terms} {operator} {}", predicate.constant)
}

fn ctl_parse_error_result(error: DslParseError, english: bool) -> CTLParseErrorResult {
    match error {
        DslParseError::SyntaxError { line, col, message } => CTLParseErrorResult {
            kind: "syntax".into(),
            line: Some(line),
            column: Some(col),
            message,
            expected: Vec::new(),
        },
        DslParseError::UnknownPlace(place) => CTLParseErrorResult {
            kind: "unknown_place".into(),
            line: None,
            column: None,
            message: if english {
                format!("Unknown place: {place}")
            } else {
                format!("Lugar desconhecido: {place}")
            },
            expected: Vec::new(),
        },
        DslParseError::UnknownTransition(transition) => CTLParseErrorResult {
            kind: "unknown_transition".into(),
            line: None,
            column: None,
            message: if english {
                format!("Unknown transition: {transition}")
            } else {
                format!("Transição desconhecida: {transition}")
            },
            expected: Vec::new(),
        },
        DslParseError::InvalidAtomicSyntax(message) => CTLParseErrorResult {
            kind: "invalid_atomic".into(),
            line: None,
            column: None,
            message,
            expected: vec!["PLACE".into(), "COMPARISON".into(), "NUMBER".into()],
        },
        DslParseError::SemanticError(message) => CTLParseErrorResult {
            kind: "semantic".into(),
            line: None,
            column: None,
            message,
            expected: Vec::new(),
        },
    }
}

fn is_english_language(language: Option<&str>) -> bool {
    language.map_or(true, |code| !code.to_lowercase().starts_with("pt"))
}

fn ctl_suggestions(input: &str, net: &PetriNet, english: bool) -> Vec<String> {
    let lower = input.trim_end().to_lowercase();
    // The DSL vocabulary follows the UI language (en-US default, pt-BR).
    let (quantifiers, temporal, negation): (Vec<String>, Vec<String>, &str) = if english {
        (
            vec!["for all".into(), "exists".into(), "always".into(), "eventually".into()],
            vec!["always".into(), "eventually".into(), "next".into(), "until".into()],
            "not",
        )
    } else {
        (
            vec!["para todos".into(), "existe".into(), "sempre".into(), "eventualmente".into()],
            vec!["sempre".into(), "eventualmente".into(), "próximo".into(), "até".into()],
            "não",
        )
    };
    let after_quantifier = if english {
        lower.ends_with("for all") || lower.ends_with("exists")
    } else {
        lower.ends_with("para todos") || lower.ends_with("existe")
    };
    let mut suggestions = if lower.is_empty() {
        quantifiers
    } else if after_quantifier {
        temporal
    } else if lower.ends_with('=') || lower.ends_with('>') || lower.ends_with('<') {
        vec!["0".into(), "1".into(), "2".into()]
    } else {
        let mut values = vec!["TRUE".into(), "FALSE".into(), negation.into(), "(".into()];
        values.extend(net.places().iter().map(|place| place.name.clone()));
        values
    };
    suggestions.truncate(12);
    suggestions
}

#[tauri::command]
fn validate_ctl_formula(
    file_path: String,
    formula: String,
    language: Option<String>,
) -> Result<CTLValidationResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let english = is_english_language(language.as_deref());
    let suggestions = ctl_suggestions(&formula, &net, english);
    match parse_ctl_query(&formula, &net) {
        Ok(parsed) => Ok(CTLValidationResult {
            valid: true,
            formula,
            preview: Some(format_ctl_formula(&parsed, &net)),
            error: None,
            suggestions,
        }),
        Err(error) => Ok(CTLValidationResult {
            valid: false,
            formula,
            preview: None,
            error: Some(ctl_parse_error_result(error, english)),
            suggestions,
        }),
    }
}

// ── Net Structure (for G6 graph) ───────────────────────────────────────

fn net_structure_arcs(net: &PetriNet) -> Vec<NetArc> {
    // Keep the mathematical Petri-net direction in the IPC contract. G6
    // already renders `source -> target`, so reversing here reverses the net.
    net.arcs()
        .iter()
        .filter_map(|arc| match (arc.source, arc.target) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                let p_name = net.places().get(p.0 as usize)?.name.clone();
                let t_name = net.transitions().get(t.0 as usize)?.name.clone();
                Some(NetArc {
                    source: p_name,
                    target: t_name,
                    weight: arc.weight as i32,
                    kind: "ordinary".to_string(),
                    threshold: None,
                })
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                let p_name = net.places().get(p.0 as usize)?.name.clone();
                let t_name = net.transitions().get(t.0 as usize)?.name.clone();
                Some(NetArc {
                    source: t_name,
                    target: p_name,
                    weight: arc.weight as i32,
                    kind: "ordinary".to_string(),
                    threshold: None,
                })
            }
            _ => None,
        })
        .chain(net.inhibitor_arcs().iter().filter_map(|inh| {
            Some(NetArc {
                source: net.places().get(inh.place.0 as usize)?.name.clone(),
                target: net.transitions().get(inh.transition.0 as usize)?.name.clone(),
                weight: 1,
                kind: "inhibitor".to_string(),
                threshold: Some(inh.threshold),
            })
        }))
        .collect()
}

#[tauri::command]
fn get_net_structure(file_path: String) -> Result<NetStructureResponse, String> {
    let net = parse_pnt_file(&file_path)?;

    let places: Vec<NetPlaceNode> = net
        .places()
        .iter()
        .map(|p| {
            let tokens = net
                .initial_marking()
                .tokens
                .get(p.id.0 as usize)
                .map(token_to_dto)
                .unwrap_or(TokenDto::Finite { value: "0".into() });
            NetPlaceNode {
                id: p.id.0 as usize,
                name: p.name.clone(),
                tokens,
            }
        })
        .collect();

    let transitions: Vec<NetTransitionNode> = net
        .transitions()
        .iter()
        .map(|t| NetTransitionNode {
            id: t.id.0 as usize,
            name: t.name.clone(),
            immediate: net.transition_kind(t.id) == racon_core::TransitionKind::Immediate,
        })
        .collect();

    let arcs = net_structure_arcs(&net);

    Ok(NetStructureResponse {
        places,
        transitions,
        arcs,
        extended: net.is_extended(),
        semantics_version: racon_core::SEMANTICS_VERSION.to_string(),
    })
}

// ── Token Game ─────────────────────────────────────────────────────────

#[tauri::command]
fn get_net_info(file_path: String) -> Result<NetInfo, String> {
    let net = parse_pnt_file(&file_path)?;

    Ok(NetInfo {
        num_places: net.place_count(),
        num_transitions: net.transition_count(),
        place_names: net.places().iter().map(|p| p.name.clone()).collect(),
        transition_names: net.transitions().iter().map(|t| t.name.clone()).collect(),
        initial_marking: marking_to_dto_vec(net.initial_marking()),
    })
}

#[tauri::command]
fn tg_get_enabled(file_path: String, marking: Vec<i32>) -> Result<FireResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let mark = marking_from_i32(&marking);

    let enabled: Vec<usize> = net
        .fireable_transitions(&mark)
        .into_iter()
        .map(|t| t.0 as usize)
        .collect();

    let deadlock = enabled.is_empty();

    Ok(FireResult {
        marking: marking_to_dto_vec(&mark),
        enabled,
        deadlock,
    })
}

#[tauri::command]
fn tg_fire_transition(
    file_path: String,
    marking: Vec<i32>,
    transition: usize,
) -> Result<FireResult, String> {
    let net = parse_pnt_file(&file_path)?;
    let t_id = TransitionId(transition as u32);

    if t_id.0 as usize >= net.transition_count() {
        return Err(format!("Invalid transition index: {}", transition));
    }

    let mark = marking_from_i32(&marking);

    if !net.is_enabled(t_id, &mark) {
        let t_name = net
            .transitions()
            .iter()
            .find(|t| t.id == t_id)
            .map(|t| t.name.as_str())
            .unwrap_or("?");
        return Err(format!(
            "Transition {} is not enabled in the current marking {:?}",
            t_name, marking
        ));
    }

    let new_marking = net.fire(t_id, &mark);
    let new_vec = marking_to_dto_vec(&new_marking);

    let enabled: Vec<usize> = net
        .fireable_transitions(&new_marking)
        .into_iter()
        .map(|t| t.0 as usize)
        .collect();

    let deadlock = enabled.is_empty();

    Ok(FireResult {
        marking: new_vec,
        enabled,
        deadlock,
    })
}

// ── Mercury → PNT conversion ──────────────────────────────────────────

fn petri_net_to_pnt_string(net: &PetriNet) -> String {
    // Shared with the engine so inhibitor arcs and immediate transitions
    // (racon-extended-pt-v1 block) survive Mercury -> PNT -> analysis.
    racon_parsers::petri_net_to_ina_string(net)
}

#[cfg(test)]
mod token_dto_tests {
    use super::*;

    #[test]
    fn finite_and_omega_have_distinct_semantic_json() {
        let finite = token_to_dto(&TokenValue::Exact(BigUint::from(u64::MAX) + 1u32));
        let omega = token_to_dto(&TokenValue::Omega);

        assert_eq!(
            serde_json::to_value(finite).unwrap(),
            serde_json::json!({
                "kind": "finite",
                "value": "18446744073709551616"
            })
        );
        assert_eq!(serde_json::to_value(omega).unwrap(), serde_json::json!({ "kind": "omega" }));
    }
}

#[tauri::command]
fn get_algorithm_catalog() -> serde_json::Value {
    serde_json::json!({
        "catalog_version": racon_reduction::catalog::CATALOG_VERSION,
        "algorithms": racon_reduction::catalog::ALGORITHMS,
        "references": racon_reduction::catalog::REFERENCES,
    })
}

#[tauri::command]
fn validate_analysis_config(
    config: racon_reduction::catalog::ConfigRequest,
) -> racon_reduction::catalog::ValidationReport {
    racon_reduction::catalog::validate(&config)
}

/// MinCov (Finkel-Haddad-Khmelnitsky 2020, Algorithm 1): minimal coverability set.
#[tauri::command]
async fn get_minimal_coverability_set(
    file_path: String,
    config: GraphGenerationConfig,
) -> Result<serde_json::Value, String> {
    let config = validate_graph_config(config)?;
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        ensure_algorithm_allowed(
            &net,
            racon_reduction::catalog::Objective::Coverability,
            GraphAlgorithm::CoverabilityMincov,
            false,
            false,
        )?;
        let ctx = analysis_context_for_graph(config);
        let started = Instant::now();
        let set = racon_coverability::MinCov::new(racon_coverability::MinCovConfig {
            max_vertices: config.max_nodes,
        })
        .compute(&net, &ctx)
        .map_err(|e| format!("UNKNOWN: o MinCov não concluiu ({e}); nenhuma conclusão sobre limitação é válida."))?;
        let names: Vec<String> = net.places().iter().map(|p| p.name.clone()).collect();
        let entry = racon_reduction::catalog::algorithm("coverability_mincov")
            .expect("catalogued");
        Ok(serde_json::json!({
            "algorithm": entry.id,
            "experimental": true,
            "reference_ids": entry.reference_ids,
            "kind": "MINIMAL_COVERABILITY_SET",
            "bounded": set.is_bounded(),
            "place_names": names,
            "unbounded_places": set.unbounded_places().into_iter().map(|i| names[i].clone()).collect::<Vec<_>>(),
            "elements": set.elements().iter().map(|m| m.iter().map(|v| match v {
                racon_coverability::OmegaValue::Finite(n) => serde_json::json!(n),
                racon_coverability::OmegaValue::Omega => serde_json::json!("omega"),
            }).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "tree_vertices": set.tree_vertices,
            "accelerations": set.accelerations,
            "iterations": set.iterations,
            "elapsed_ms": started.elapsed().as_millis() as u64,
            "does_not_preserve": entry.does_not_preserve,
        }))
    })
    .await
    .map_err(|error| format!("Falha na tarefa MinCov: {error}"))?
}

/// Symbolic state spaces (MDD/BDD), unfolding, structural/polyhedral reductions
/// and CovProc/MP coverability sets; same entry point as the HTTP API route
/// `POST /v1/sessions/{id}/algorithms/{algorithm}`.
#[tauri::command]
async fn run_advanced_analysis(
    file_path: String,
    algorithm: String,
    params: Option<racon_engine::AdvancedParams>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let net = parse_pnt_file(&file_path)?;
        racon_engine::run_advanced(&net, &algorithm, &params.unwrap_or_default()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| format!("Falha na tarefa de análise avançada: {error}"))?
}

#[tauri::command]
fn parse_mercury_text(script: String) -> Result<String, String> {
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    // `parse_gspn` only detects stochastic/temporal annotations that the
    // qualitative engine cannot represent. Immediate transitions with default
    // annotations and inhibitor arcs are handled by `parse_string`.
    let parsed = parser.parse_gspn(&script).map_err(|e| e.to_string())?;
    let has_timing = parsed.gspn_spec.as_ref().is_some_and(|spec| {
        spec.transition_types
            .values()
            .any(|kind| matches!(kind, racon_gspn::TransitionType::Timed { .. }))
    });
    if has_timing {
        return Err(
            "O script Mercury contém semântica GSPN temporal; a conversão para PNT perderia essa informação."
                .to_string(),
        );
    }
    let net = parser.parse_string(&script).map_err(|e| e.to_string())?;
    validate_petri_net(&net).map_err(|e| format!("Formal Petri-net validation failed: {e}"))?;
    let pnt_content = petri_net_to_pnt_string(&net);
    let dir = std::env::temp_dir();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = dir.join(format!("racon_mercury_{}.pnt", ts));
    std::fs::write(&path, &pnt_content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_structure_preserves_arc_direction() {
        let mut builder = racon_core::PetriNetBuilder::default();
        let p0 = builder.add_place("P0".into(), None);
        let p1 = builder.add_place("P1".into(), None);
        let t0 = builder.add_transition("T0".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        let net = builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .expect("test net should build");

        let arcs = net_structure_arcs(&net);
        assert_eq!((arcs[0].source.as_str(), arcs[0].target.as_str()), ("P0", "T0"));
        assert_eq!((arcs[1].source.as_str(), arcs[1].target.as_str()), ("T0", "P1"));
    }
}

// =============================================================================
// System tray
// =============================================================================

struct TrayState {
    show: tauri::menu::MenuItem<tauri::Wry>,
    quit: tauri::menu::MenuItem<tauri::Wry>,
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "tray-show", "Show SPECTRA", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("SPECTRA")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-show" => show_main_window(app),
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState { show, quit });
    Ok(())
}

/// Lets the frontend translate the tray menu and tooltip to the UI language.
#[tauri::command]
fn set_tray_labels(
    app: AppHandle,
    state: tauri::State<'_, TrayState>,
    tooltip: String,
    show: String,
    quit: String,
) -> Result<(), String> {
    state.show.set_text(show).map_err(|e| e.to_string())?;
    state.quit.set_text(quit).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if let Ok(dir) = app.path().app_cache_dir() {
                graph_cache::init(dir.join("graphs"));
            }
            if let Err(error) = setup_tray(app) {
                eprintln!("could not create the system tray icon: {error}");
            }
            let app_handle = app.handle().clone();
            let panic_app = app_handle.clone();
            let default_panic_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                default_panic_hook(panic_info);
                let _ = write_log_entry(panic_app.clone(), LogEntry {
                    timestamp: format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
                    level: "ERROR".into(),
                    source: "rust".into(),
                    event: "panic".into(),
                    message: panic_info.to_string(),
                });
            }));
            let _ = write_log_entry(app_handle, LogEntry {
                timestamp: format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
                level: "INFO".into(),
                source: "tauri".into(),
                event: "application.started".into(),
                message: format!("RACoN {} iniciado", env!("CARGO_PKG_VERSION")),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_tray_labels,
            write_log_entry,
            read_log_entries,
            analyze_pnt,
            get_required_initial_properties,
            check_cycle_fairness,
            get_reachability_graph,
            get_reachability_graph_page,
            get_coverage_tree,
            get_structural_properties,
            get_deadlock_trap_property,
            get_algorithm_catalog,
            validate_analysis_config,
            get_minimal_coverability_set,
            run_advanced_analysis,
            get_analysis_evidence,
            get_graph_cache_status,
            check_bad_states,
            check_place_boundedness,
            check_transition_dead,
            get_invariants,
            check_ctl_formula,
            validate_ctl_formula,
            get_net_structure,
            get_net_info,
            tg_get_enabled,
            tg_fire_transition,
            get_incidence_matrix,
            check_marking_reachability,
            compute_behavioral_properties,
            test_marking_algebraic,
            check_marking_coverability,
            compute_distances,
            compute_circuits,
            build_predicate,
            test_vector,
            parse_mercury_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod ina_parity_tests {
    use super::*;

    const MERCURY: &str = include_str!("../../crates/racon-engine/tests/fixtures/water_pumps.mercury");

    /// What the user does in the Graphs tab: generate (and so save) the graphs of the net.
    /// A graph that cannot be completed (unbounded net) is simply not saved.
    fn generate_graphs(path: &str) {
        graph_cache::init(std::env::temp_dir().join(format!("racon_graph_cache_tests_{}", std::process::id())));
        let config = GraphGenerationConfig { max_nodes: 50_000, ..GraphGenerationConfig::default() };
        let _ = get_reachability_graph_impl(path.to_string(), config);
        let _ = get_coverage_tree_impl(path.to_string(), config);
    }

    fn water_pump_file(tag: &str) -> String {
        let parser = MercuryParserImpl::new(MercuryParserConfig::default());
        let net = parser.parse_string(MERCURY).unwrap();
        let path = std::env::temp_dir().join(format!("racon_ina_parity_{tag}_{}.pnt", std::process::id()));
        std::fs::write(&path, petri_net_to_pnt_string(&net)).unwrap();
        let path = path.to_string_lossy().into_owned();
        generate_graphs(&path);
        path
    }

    /// IR(3) IR_BUFFER MP1_RUNNING(1) MP1_STOPED MP2_RUNNING(1) MP2_STOPED OR(1) OR_BUFFER(2)
    /// SS_AVAILABLE(1) SS_UNAVAILABLE WATER_INPUT(1) WATER_OUTPUT(1)
    const M0: [i32; 12] = [3, 0, 1, 0, 1, 0, 1, 2, 1, 0, 1, 1];

    #[test]
    fn behavioural_properties_are_complete_once_the_graph_is_generated() {
        let path = water_pump_file("behaviour");
        assert!(graph_cache::status(&path).reachability);
        let result = compute_behavioral_properties_impl(path, GraphGenerationConfig::default()).unwrap();
        // Lp, PER, GFC and DCF are all decided from the saved graph.
        assert_eq!(result.state_count, 111);
        let _ = (result.place_live, result.persistent, result.gfc, result.dynamic_conflict_free);
        assert!(!result.place_liveness.is_empty());
    }

    #[test]
    fn queries_refuse_to_run_before_the_graph_is_generated() {
        let path = std::env::temp_dir().join(format!("racon_no_graph_{}.pnt", std::process::id()));
        std::fs::write(&path, INA_PNT).unwrap();
        let path = path.to_string_lossy().into_owned();
        graph_cache::init(std::env::temp_dir().join(format!("racon_graph_cache_empty_{}", std::process::id())));
        let Err(error) = compute_distances_impl(path.clone(), None, None, GraphGenerationConfig::default()) else { panic!("ran without a graph") };
        assert!(error.starts_with("GRAFO_NAO_GERADO"), "{error}");
        assert!(!graph_cache::status(&path).reachability);
    }

    #[test]
    fn marking_that_breaks_a_complement_invariant_is_refuted_algebraically() {
        let path = water_pump_file("pinv");
        let mut target = M0;
        target[1] = 1; // IR + IR_BUFFER would be 4, but it is always 3
        let result = test_marking_algebraic_impl(path, target.to_vec()).unwrap();
        assert_eq!(result.verdict, "unreachable");
        assert_eq!(result.method.as_deref(), Some("p_invariant"));
        assert_ne!(result.invariant_initial, result.invariant_target);
    }

    #[test]
    fn initial_marking_passes_the_algebraic_test_with_the_empty_firing_vector() {
        let path = water_pump_file("m0");
        let result = test_marking_algebraic_impl(path, M0.to_vec()).unwrap();
        assert_eq!(result.verdict, "possibly_reachable");
        assert!(result.parikh.is_some_and(|x| x.iter().all(|v| v == "0")));
    }

    fn net_file(tag: &str, build: impl FnOnce(&mut racon_core::PetriNetBuilder), tokens: Vec<TokenValue>) -> String {
        let mut builder = racon_core::PetriNetBuilder::default();
        build(&mut builder);
        let net = builder.build(tokens).unwrap();
        let path = std::env::temp_dir().join(format!("racon_cover_{tag}_{}.pnt", std::process::id()));
        std::fs::write(&path, petri_net_to_pnt_string(&net)).unwrap();
        let path = path.to_string_lossy().into_owned();
        generate_graphs(&path);
        path
    }

    #[test]
    fn coverability_works_on_an_unbounded_net_and_refuses_extended_ones() {
        // p0(1) --t0--> 2 p0 : unbounded, so any token count is covered.
        let path = net_file("unb", |b| {
            let p = b.add_place("p0".into(), None);
            let t = b.add_transition("t0".into(), None);
            b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
            b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 2);
        }, vec![TokenValue::Finite(1)]);
        let covered = check_marking_coverability_impl(path.clone(), vec![50], GraphGenerationConfig::default()).unwrap();
        assert!(covered.covered && covered.path_has_omega);
        assert_eq!(covered.bounded, Some(false));
        assert!(covered.path.len() >= 2);
        let initial = check_marking_coverability_impl(path, vec![1], GraphGenerationConfig::default()).unwrap();
        assert_eq!(initial.path.len(), 1, "M0 already covers [1]");

        // The water-pump net has inhibitor arcs and immediate transitions: Karp-Miller is not valid.
        let err = check_marking_coverability_impl(water_pump_file("cov"), M0.to_vec(), GraphGenerationConfig::default()).err().expect("extended nets are refused");
        assert!(err.starts_with("UNSUPPORTED"), "{err}");
    }

    #[test]
    fn coverability_refutes_a_marking_no_reachable_state_dominates() {
        // p0(1) --t0--> p1 : one token in total.
        let path = net_file("bnd", |b| {
            let p0 = b.add_place("p0".into(), None);
            let p1 = b.add_place("p1".into(), None);
            let t = b.add_transition("t0".into(), None);
            b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t), 1);
            b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p1), 1);
        }, vec![TokenValue::Finite(1), TokenValue::Finite(0)]);
        let no = check_marking_coverability_impl(path.clone(), vec![1, 1], GraphGenerationConfig::default()).unwrap();
        assert!(!no.covered && no.path.is_empty());
        let yes = check_marking_coverability_impl(path, vec![0, 1], GraphGenerationConfig::default()).unwrap();
        assert!(yes.covered && !yes.path_has_omega);
        assert_eq!(yes.path.len(), 2);
        assert_eq!(yes.path[1].transition_name.as_deref(), Some("t0"));
    }

    #[test]
    fn complement_pairs_are_classified_as_exact_place_invariants() {
        let path = water_pump_file("vec");
        // y = IR + IR_BUFFER
        let mut y = vec![0i64; 12];
        y[0] = 1;
        y[1] = 1;
        let r = test_vector(path.clone(), "place".into(), y).unwrap();
        assert!(r.invariant && r.sub && r.sur && r.semipositive && !r.positive);
        assert_eq!(r.initial_value.as_deref(), Some("3"));
        // y = IR alone is not an invariant (WATER transfers move tokens out of it).
        let mut lone = vec![0i64; 12];
        lone[0] = 1;
        let r = test_vector(path, "place".into(), lone).unwrap();
        assert!(!r.invariant);
        assert!(test_vector(water_pump_file("len"), "place".into(), vec![1, 2]).is_err());
    }

    #[test]
    fn invariant_bases_span_more_than_the_semiflows_and_are_exact() {
        let path = water_pump_file("basis");
        let net = parse_pnt_file(&path).unwrap();
        let result = compute_invariant_result(&net).unwrap();
        // rank(C) = 5 for this net: 12 places -> 7 P-invariants, 15 transitions -> 10 T-invariants.
        assert_eq!((result.p_basis.rank, result.p_basis.dimension), (5, 7));
        assert_eq!((result.t_basis.rank, result.t_basis.dimension), (5, 10));
        assert_eq!(result.p_basis.vectors.len(), 7);
        assert_eq!(result.t_basis.vectors.len(), 10);
        // The app finds 9 minimal T-semiflows: they do not span the whole kernel.
        assert_eq!(result.minimal_t_semiflows.len(), 9);
        for v in &result.p_basis.vectors {
            let r = test_vector(path.clone(), "place".into(), v.iter().map(|x| x.parse().unwrap()).collect()).unwrap();
            assert!(r.invariant, "{v:?}");
        }
        for v in &result.t_basis.vectors {
            let r = test_vector(path.clone(), "transition".into(), v.iter().map(|x| x.parse().unwrap()).collect()).unwrap();
            assert!(r.invariant, "{v:?}");
        }
    }

    #[test]
    fn distances_match_an_independent_breadth_first_search() {
        let path = water_pump_file("dist");
        let r = compute_distances_impl(path.clone(), None, None, GraphGenerationConfig::default()).unwrap();
        assert_eq!(r.state_count, 111);
        assert_eq!(r.histogram, vec![1, 5, 13, 19, 21, 19, 15, 11, 4, 2, 1]);
        assert_eq!(r.eccentricity, 10);
        assert_eq!(r.unreachable, 0);
        assert_eq!((r.diameter, r.strongly_connected), (Some(13), Some(true)));
        // Pairwise: M0 to a marking one firing away (MP1_WATER_TRANSFER: IR 3->2, IR_BUFFER 0->1, OR_BUFFER 2->1, OR 1->2).
        let mut next = M0;
        next[0] = 2;
        next[1] = 1;
        next[6] = 2;
        next[7] = 1;
        let r = compute_distances_impl(path.clone(), None, Some(next.to_vec()), GraphGenerationConfig::default()).unwrap();
        let target = r.target.unwrap();
        assert_eq!(target.distance, Some(1));
        assert_eq!(target.path.len(), 2);
        assert_eq!(target.path[1].transition_name.as_deref(), Some("MP1_WATER_TRANSFER"));
        // A marking that is not reachable is reported, not silently ignored.
        let mut bad = M0;
        bad[1] = 1;
        assert!(compute_distances_impl(path, None, Some(bad.to_vec()), GraphGenerationConfig::default()).is_err());
    }

    #[test]
    fn dynamic_conflicts_are_listed_with_counts_and_an_example_path() {
        let path = water_pump_file("conf");
        let r = compute_behavioral_properties_impl(path, GraphGenerationConfig::default()).unwrap();
        assert!(!r.dynamic_conflict_free);
        // 23 pairs, counting disabling by immediate precedence (net.rs: is_dynamic_conflict); checked
        // against an independent Python model of the same definition.
        assert_eq!(r.conflicts.pair_count, 23);
        let top = &r.conflicts.pairs[0];
        assert_eq!(top.states, 27);
        assert!(["MP1_STOP", "MP2_STOP"].contains(&top.first.as_str()) || ["MP1_STOP", "MP2_STOP"].contains(&top.second.as_str()));
        assert!(!top.example_path.is_empty() && top.example_state.starts_with('M'));
        assert_eq!(r.conflicts.pairs.iter().map(|p| p.states).sum::<usize>(), 237);
    }

    fn circuits(path: &str, mode: &str, through: Option<Vec<i32>>, max_length: Option<usize>, first_only: bool, limit: usize) -> CircuitsResult {
        compute_circuits_impl(
            path.to_string(),
            CircuitsRequest { mode: mode.into(), through, max_length, first_only, limit, time_limit_ms: 20_000 },
            GraphGenerationConfig::default(),
        )
        .unwrap()
    }

    #[test]
    fn shortest_circuits_match_the_ina_listing() {
        let path = water_pump_file("circ_short");
        // INA's "print circuits with minimal length" listed length-2 circuits such as
        // n1<-t5--n2<-t6--n1 (MP1_START / MP1_STOP); an independent search finds 70 of them.
        let r = circuits(&path, "shortest", None, None, false, 1000);
        assert!(r.exhaustive);
        assert_eq!((r.shortest, r.longest, r.total_found), (Some(2), Some(2), 70));
        assert_eq!(r.circuits.len(), 70);
        // Every circuit is closed, its firing counts form a T-invariant, and
        // "not fired" is exactly the complement of the firing support (INA's livelocked list).
        for c in &r.circuits {
            assert_eq!(c.steps.len(), c.length + 1);
            assert_eq!(c.steps.first().unwrap().marking.len(), c.steps.last().unwrap().marking.len());
            assert_eq!(c.parikh.iter().map(|t| t.count).sum::<usize>(), c.length);
            assert_eq!(c.parikh.len() + c.not_fired.len(), 15);
        }
        let start_stop = r.circuits.iter().any(|c| {
            let mut names: Vec<&str> = c.parikh.iter().map(|t| t.name.as_str()).collect();
            names.sort();
            names == ["MP1_START", "MP1_STOP"]
        });
        assert!(start_stop, "the MP1_START/MP1_STOP circuit INA printed first");
        // The first-only shortcut returns exactly one.
        let first = circuits(&path, "shortest", None, None, true, 1000);
        assert_eq!((first.circuits.len(), first.exhaustive), (1, false));
    }

    #[test]
    fn circuit_counts_by_length_match_an_independent_enumeration() {
        let path = water_pump_file("circ_all");
        let r = circuits(&path, "all", None, Some(8), false, 5);
        assert!(r.exhaustive);
        // lengths 2..=8, from an independent Python DFS over the same graph.
        assert_eq!(&r.length_counts[2..=8], &[70, 48, 36, 338, 34, 2448, 2028]);
        assert_eq!(r.total_found, 5002);
        // Listed shortest first, capped at 5 per length.
        assert!(r.circuits.windows(2).all(|w| w[0].length <= w[1].length));
        assert!(r.circuits.len() <= 5 * 7);
    }

    #[test]
    fn circuits_through_the_initial_state_are_counted_and_the_longest_search_is_bounded() {
        let path = water_pump_file("circ_through");
        let r = circuits(&path, "all", Some(M0.to_vec()), Some(8), false, 1000);
        assert_eq!(&r.length_counts[2..=8], &[2, 6, 2, 48, 6, 362, 500]);
        assert_eq!(r.total_found, 926);
        // "Longest" has no exact answer to promise: with a tiny time limit it reports the best so far.
        let longest = compute_circuits_impl(
            path.clone(),
            CircuitsRequest { mode: "longest".into(), through: None, max_length: None, first_only: false, limit: 3, time_limit_ms: 300 },
            GraphGenerationConfig::default(),
        )
        .unwrap();
        assert!(!longest.exhaustive && longest.longest.unwrap() > 8);
        assert!(longest.circuits.iter().all(|c| Some(c.length) == longest.longest));
        // A marking that is not a state is an error.
        let mut bad = M0;
        bad[1] = 1;
        assert!(compute_circuits_impl(path, CircuitsRequest { mode: "all".into(), through: Some(bad.to_vec()), max_length: Some(4), first_only: false, limit: 10, time_limit_ms: 1000 }, GraphGenerationConfig::default()).is_err());
    }

    const INA_PNT: &str = include_str!("../../crates/racon-engine/tests/fixtures/water_pumps_ina.pnt");
    const INA_ENABLED_PDC: &str = include_str!("../../crates/racon-engine/tests/fixtures/ina_enabled_t5_t0.pdc");
    const INA_STATES_PDC: &str = include_str!("../../crates/racon-engine/tests/fixtures/ina_states_1_2_3.pdc");

    fn ina_pump_file(tag: &str) -> String {
        let path = std::env::temp_dir().join(format!("racon_ina_pdc_{tag}_{}.pnt", std::process::id()));
        std::fs::write(&path, INA_PNT).unwrap();
        let path = path.to_string_lossy().into_owned();
        generate_graphs(&path);
        path
    }

    #[test]
    fn enabledness_predicate_is_byte_identical_to_the_ina_pdc_file() {
        // INA: option E with transitions 5 (MP1_START) then 0 (MP1_IR_EMPTY). The latter has a
        // weight-3 test arc on IR_BUFFER, which becomes `low = 3`.
        let r = build_predicate_impl(
            ina_pump_file("enabled"),
            "enabled".into(),
            vec!["MP1_START".into(), "MP1_IR_EMPTY".into()],
            vec![],
            GraphGenerationConfig::default(),
        )
        .unwrap();
        assert_eq!(r.pdc, INA_ENABLED_PDC);
        assert_eq!(r.conjunctions.len(), 2);
        assert_eq!(r.conjunctions[1].bounds.iter().find(|b| b.place == "IR_BUFFER").map(|b| (b.low, b.upp)), Some((3, None)));
    }

    #[test]
    fn state_set_predicate_is_byte_identical_to_the_ina_pdc_file() {
        // INA: option C with states 1, 2 and 3, which are these markings in the .gra.
        let path = ina_pump_file("states");
        let net = parse_pnt_file(&path).unwrap();
        let ctx = analysis_context_for_graph(GraphGenerationConfig::default());
        let graph = ReachabilityEngineImpl { config: ReachabilityConfig::default() }.build_complete(&net, &ctx).unwrap();
        let ids: Vec<usize> = [
            [3, 0, 1, 0, 1, 0, 1, 2, 1, 0, 1, 1],
            [3, 0, 0, 1, 1, 0, 1, 2, 1, 0, 1, 1],
            [3, 0, 0, 1, 0, 1, 1, 2, 1, 0, 1, 1],
        ]
        .iter()
        .map(|m| graph.get_node_by_marking(&marking_from_i32(m)).expect("reachable"))
        .collect();
        let r = build_predicate_impl(path, "states".into(), vec![], ids, GraphGenerationConfig::default()).unwrap();
        assert_eq!(r.pdc, INA_STATES_PDC);
        assert_eq!(r.conjunctions.len(), 3);
    }

    #[test]
    fn enabled_atom_selects_the_same_18_states_as_ina_option_p() {
        // INA evaluated the predicate for MP1_START (written by option E) over the graph
        // with its option P and wrote 18 states; the DSL atom selects the same number.
        let r = check_ctl_formula_impl(ina_pump_file("p18"), "enabled(MP1_START)".into(), GraphGenerationConfig::default()).unwrap();
        assert_eq!((r.count, r.total_states), (18, 111));
        assert_eq!(r.formula, "(((IR >= 1) AND (MP1_STOPED >= 1)) AND (OR_BUFFER >= 1)) AND (SS_AVAILABLE >= 1)");
    }

    #[test]
    fn predicate_inputs_are_validated() {
        let path = ina_pump_file("validate");
        let cfg = GraphGenerationConfig::default;
        assert!(build_predicate_impl(path.clone(), "enabled".into(), vec![], vec![], cfg()).is_err());
        assert!(build_predicate_impl(path.clone(), "enabled".into(), vec!["NOPE".into()], vec![], cfg()).is_err());
        assert!(build_predicate_impl(path.clone(), "states".into(), vec![], vec![99_999], cfg()).is_err());
        assert!(build_predicate_impl(path, "other".into(), vec![], vec![], cfg()).is_err());
    }

    #[test]
    fn water_pump_state_space_is_one_strongly_connected_component() {
        let path = water_pump_file("scc");
        let result = compute_behavioral_properties_impl(path, GraphGenerationConfig::default()).unwrap();
        assert_eq!(result.state_count, 111);
        assert_eq!(result.scc.count, 1);
        assert_eq!(result.scc.terminal_count, 1);
        assert_eq!(result.scc.largest, 111);
        assert!(result.scc.components[0].contains_initial);
        assert_eq!(result.scc.home_state_count, 111);
    }
}
