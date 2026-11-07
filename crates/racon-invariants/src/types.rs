use num_bigint::BigInt;
use racon_core::net::{PlaceId, TransitionId};
use std::collections::HashMap;

/// A Place Invariant (P-semiflow) — coefficients y such that y^T * C = 0.
#[derive(Debug, Clone)]
pub struct PInvariant {
    pub coefficients: Vec<BigInt>,
    pub is_semiflow: bool,
    pub is_minimal: bool,
    pub support: Vec<PlaceId>,
    pub constant_value: Option<BigInt>,
}

/// A Transition Invariant (T-semiflow) — coefficients x such that C * x = 0.
#[derive(Debug, Clone)]
pub struct TInvariant {
    pub coefficients: Vec<BigInt>,
    pub is_semiflow: bool,
    pub is_minimal: bool,
    pub support: Vec<TransitionId>,
    pub is_realizable: Option<bool>,
    /// A concrete firing sequence proving realizability when available.
    pub realization_witness: Option<Vec<TransitionId>>,
}

/// Result of a marking test using P-invariants.
#[derive(Debug, Clone)]
pub enum MarkingTestResult {
    /// Marking is mathematically proven unreachable (violates an invariant).
    Unreachable {
        violated_invariant_index: usize,
        expected_value: BigInt,
        actual_value: BigInt,
    },
    /// Marking passes all invariant tests (may or may not be reachable).
    PossiblyReachable,
}

/// Complete invariant analysis result.
#[derive(Debug, Clone)]
pub struct InvariantAnalysisResult {
    pub p_semiflows: Vec<PInvariant>,
    pub t_semiflows: Vec<TInvariant>,
    /// Minimal non-negative P-semiflows found by the elimination algorithm.
    /// This is not claimed to be a basis of the full kernel.
    pub minimal_p_semiflows: Vec<Vec<BigInt>>,
    /// Minimal non-negative T-semiflows found by the elimination algorithm.
    /// This is not claimed to be a basis of the full kernel.
    pub minimal_t_semiflows: Vec<Vec<BigInt>>,
    pub is_conservative: bool,
    pub is_cpi: bool,
    pub is_cti: bool,
    pub structural_bounds: HashMap<PlaceId, Option<u64>>,
    /// Positive certificate for Murata's C^T y <= 0 structural boundedness criterion.
    pub structural_bounded_by_inequality: Option<Vec<BigInt>>,
}

/// A finite, reproducible certificate that all non-negative kernel vectors
/// within a coefficient bound have a reported minimal support.
///
/// This is deliberately a bounded certificate, not a claim about vectors with
/// arbitrarily large coefficients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemiflowCompletenessCertificate {
    pub coefficient_bound: i64,
    pub dimension: usize,
    pub verified: bool,
}

/// Direction of a one-sided (inequality) place invariant.
///
/// IMPORTANT (spec §4, §17, §19, §20):
/// The inner mathematical core is DIRECTION-BASED and must NOT use the public
/// terminology "SUB"/"SUR" (those names are a semantic mapping concern of the
/// DTO layer in `racon-engine` / `racon-api`). The semantic mapping is:
///
/// ```text
/// SUB (non-increasing marking value)  -> LessOrEqual
/// SUR (non-decreasing marking value)  -> GreaterOrEqual
/// ```
///
/// Changing the incidence convention (C = Post - Pre is used here) REVERSES the
/// monotonic interpretation. Do not invert these relations without updating the
/// mathematical definition, the testsmati, the API contract and this doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InequalityRelation {
    /// Left inequality invariant with non-increasing value: y^T C <= 0.
    LessOrEqual,
    /// Left inequality invariant with non-decreasing value: y^T C >= 0.
    GreaterOrEqual,
}

/// A single place-oriented inequality invariant.
///
/// Place coefficients `y` (length = place count) satisfying the one-sided
/// relation `rel` against the dense incidence matrix `C` (rows = places,
/// cols = transitions), with residual `r = y^T C` recomputed independently.
///
/// The residual is the mathematical certificate: it is NEVER taken from the
/// slack-augmented solver output; it is recomputed as `y^T C` by exact
/// arithmetic and checked against `relation` (§9, §10, §13).
#[derive(Debug, Clone)]
pub struct PlaceInequalityInvariant {
    /// Non-negative place coefficients; primitive-normalized (gcd = 1).
    pub coefficients: Vec<BigInt>,
    /// Direction of the inequality satisfied by the residual.
    pub relation: InequalityRelation,
    /// Residual `r = y^T C` (length = transition count), recomputed exactly.
    pub residual: Vec<BigInt>,
    /// True iff the residual has at least one STRICT component
    /// (proper inequality invariant, divisibleby spec §10).
    pub is_proper: bool,
    /// True iff the residual is the zero vector (exact, an ordinary
    /// P-invariant) — this invariant also lives in the exact collection.
    pub is_exact: bool,
    /// True iff the invariant is support-minimal among the projected
    /// non-decreasing/non-increasing vectors.
    pub is_minimal: bool,
    /// Places with a strictly positive coefficient (nonzero support).
    pub support: Vec<PlaceId>,
    /// Initial marking evaluation `y^T M0` (exact), when the initial marking
    /// is completely known.
    pub initial_value: Option<BigInt>,
}

/// A finite, reproducible certificate that all non-negative place inequality
/// kernel vectors within a coefficient bound are support-covered by the
/// reported minimal set.
///
/// Like `SemiflowCompletenessCertificate`, this is a deliberately bounded
/// certificate, not a claim about arbitrarily large coefficients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InequalityCompletenessCertificate {
    /// Coefficient bound used for the exhaustive bounded enumeration.
    pub coefficient_bound: i64,
    /// Dimension of the projected place-coefficient space.
    pub dimension: usize,
    /// Declared inequality direction (used to orient the residual check).
    pub relation: InequalityRelation,
    /// Whether the bounded enumeration certified support-completeness.
    pub verified: bool,
}
