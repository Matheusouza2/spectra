use std::collections::HashSet;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use racon_core::execution::AnalysisContext;
use racon_core::net::{PetriNet, PlaceId, TransitionId};

use crate::types::*;

/// Errors from the invariant computation engine.
#[derive(Debug, Clone)]
pub enum InvariantError {
    Cancelled,
    MatrixTooLarge { rows: usize, cols: usize },
    ArithmeticOverflow,
    VerificationFailed(String),
}

impl std::fmt::Display for InvariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Invariant computation cancelled"),
            Self::MatrixTooLarge { rows, cols } => {
                write!(
                    f,
                    "Matrix too large for dense computation: {}x{}",
                    rows, cols
                )
            }
            Self::ArithmeticOverflow => write!(f, "Integer overflow during invariant computation"),
            Self::VerificationFailed(msg) => write!(f, "Invariant verification failed: {}", msg),
        }
    }
}

impl std::error::Error for InvariantError {}

/// Trait for invariant computation engines.
pub trait InvariantEngine {
    /// Compute minimal P-semiflows (non-negative P-invariants).
    fn compute_p_semiflows(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<Vec<PInvariant>, InvariantError>;

    /// Compute minimal T-semiflows (non-negative T-invariants).
    fn compute_t_semiflows(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<Vec<TInvariant>, InvariantError>;
}

/// Default invariant engine implementation.
///
/// Computes minimal non-negative semiflows via integer elimination.
#[derive(Default)]
pub struct InvariantEngineImpl;

impl InvariantEngine for InvariantEngineImpl {
    fn compute_p_semiflows(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<Vec<PInvariant>, InvariantError> {
        let c = net.incidence_matrix();
        let m = c.place_count();
        let n = c.transition_count();

        // Build dense C (m rows x n cols)
        let mut c_dense = vec![vec![BigInt::zero(); n]; m];
        for i in 0..m {
            for j in 0..n {
                c_dense[i][j] = c.data()[i][j].clone();
            }
        }

        // Starke's elimination computes the minimal non-negative vectors in
        // the left kernel rather than filtering an arbitrary null-space vector.
        let candidate_vectors =
            Self::compute_minimal_nonnegative_kernel_vectors(&c_dense, context)?;
        let mut p_semiflows = Self::extract_minimal_p_semiflows(&candidate_vectors, m)?;
        verify_minimal_nonnegative_vectors(
            &p_semiflows
                .iter()
                .map(|invariant| invariant.coefficients.clone())
                .collect::<Vec<_>>(),
            m,
        )
        .map_err(InvariantError::VerificationFailed)?;
        for invariant in &mut p_semiflows {
            let value = invariant
                .coefficients
                .iter()
                .zip(net.initial_marking().tokens.iter())
                .try_fold(BigInt::zero(), |sum, (coefficient, token)| match token {
                    racon_core::marking::TokenValue::Finite(value) => {
                        Some(sum + coefficient * BigInt::from(*value))
                    }
                    racon_core::marking::TokenValue::Exact(value) => {
                        Some(sum + coefficient * BigInt::from(value.clone()))
                    }
                    racon_core::marking::TokenValue::Omega => None,
                })
                .ok_or(InvariantError::ArithmeticOverflow)?;
            invariant.constant_value = Some(value);
        }
        Ok(p_semiflows)
    }

    fn compute_t_semiflows(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<Vec<TInvariant>, InvariantError> {
        let c = net.incidence_matrix();
        let m = c.place_count();
        let n = c.transition_count();

        // Build C^T (n rows x m cols)
        let mut ct = vec![vec![BigInt::zero(); m]; n];
        for i in 0..m {
            for j in 0..n {
                ct[j][i] = c.data()[i][j].clone();
            }
        }

        // The left kernel of C^T is the right kernel of C.
        let candidate_vectors = Self::compute_minimal_nonnegative_kernel_vectors(&ct, context)?;
        let mut t_semiflows = Self::extract_minimal_t_semiflows(&candidate_vectors, n)?;
        verify_minimal_nonnegative_vectors(
            &t_semiflows
                .iter()
                .map(|invariant| invariant.coefficients.clone())
                .collect::<Vec<_>>(),
            n,
        )
        .map_err(InvariantError::VerificationFailed)?;
        for invariant in &mut t_semiflows {
            let realization = realize_t_invariant(net, &invariant.coefficients);
            match realization {
                RealizationResult::Found(sequence) => {
                    invariant.is_realizable = Some(true);
                    invariant.realization_witness = Some(sequence);
                }
                RealizationResult::NotFound => invariant.is_realizable = Some(false),
                RealizationResult::Inconclusive => invariant.is_realizable = None,
            }
        }
        Ok(t_semiflows)
    }
}

impl InvariantEngineImpl {
    /// Compute minimal non-negative vectors in the left kernel of a matrix.
    ///
    /// This is the pairwise elimination procedure described by Starke on
    /// pp. 119-121: augment the matrix with an identity matrix, eliminate
    /// each original column by combining rows with opposite signs, then keep
    /// the non-negative vectors in the appended coordinates.
    fn compute_minimal_nonnegative_kernel_vectors(
        matrix: &[Vec<BigInt>],
        context: &AnalysisContext,
    ) -> Result<Vec<Vec<BigInt>>, InvariantError> {
        let rows = matrix.len();
        if rows == 0 {
            return Ok(Vec::new());
        }
        let cols = matrix[0].len();
        let mut work: Vec<Vec<BigInt>> = matrix
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let mut augmented = row.clone();
                augmented.extend((0..rows).map(|j| BigInt::from((i == j) as u8)));
                augmented
            })
            .collect();

        for column in 0..cols {
            context
                .check_cancellation()
                .map_err(|_| InvariantError::Cancelled)?;
            let current = work.clone();
            let mut next: Vec<Vec<BigInt>> = current
                .iter()
                .filter(|row| row[column].is_zero())
                .cloned()
                .collect();

            for i in 0..current.len() {
                for j in (i + 1)..current.len() {
                    let left = &current[i][column];
                    let right = &current[j][column];
                    if left.is_zero() || right.is_zero() || left.signum() == right.signum() {
                        continue;
                    }
                    let left_abs = left.abs();
                    let right_abs = right.abs();
                    let mut combined = Vec::with_capacity(current[i].len());
                    for (a, b) in current[i].iter().zip(current[j].iter()) {
                        combined.push(&right_abs * a + &left_abs * b);
                    }
                    normalize_bigint(&mut combined);
                    next.push(combined);
                }
            }

            let mut unique = HashSet::new();
            work = next
                .into_iter()
                .filter(|row| unique.insert(row.clone()))
                .collect();
        }

        let mut result = Vec::new();
        let mut unique = HashSet::new();
        for mut row in work {
            let mut vector = row.split_off(cols);
            normalize_bigint(&mut vector);
            if vector.iter().all(|value| value >= &BigInt::zero())
                && vector.iter().any(|value| value > &BigInt::zero())
            {
                if unique.insert(vector.clone()) {
                    result.push(vector);
                }
            }
        }

        Ok(Self::prune_dominated(&result))
    }

    fn extract_minimal_p_semiflows(
        candidate_vectors: &[Vec<BigInt>],
        place_count: usize,
    ) -> Result<Vec<PInvariant>, InvariantError> {
        // Filter strictly non-negative vectors
        let mut semiflows: Vec<Vec<BigInt>> = candidate_vectors
            .iter()
            .filter(|v| {
                v.iter().all(|c| c >= &BigInt::zero()) && v.iter().any(|c| c > &BigInt::zero())
            })
            .cloned()
            .collect();

        // Pad all to place_count length
        for v in &mut semiflows {
            while v.len() < place_count {
                v.push(BigInt::zero());
            }
        }

        semiflows = Self::prune_dominated(&semiflows);

        let result = semiflows
            .into_iter()
            .map(|coeffs| {
                let support: Vec<PlaceId> = coeffs
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| **c > BigInt::zero())
                    .map(|(i, _)| PlaceId(i as u32))
                    .collect();

                PInvariant {
                    coefficients: coeffs,
                    is_semiflow: true,
                    is_minimal: true,
                    support,
                    constant_value: None,
                }
            })
            .collect();

        Ok(result)
    }

    fn extract_minimal_t_semiflows(
        candidate_vectors: &[Vec<BigInt>],
        transition_count: usize,
    ) -> Result<Vec<TInvariant>, InvariantError> {
        let mut semiflows: Vec<Vec<BigInt>> = candidate_vectors
            .iter()
            .filter(|v| {
                v.iter().all(|c| c >= &BigInt::zero()) && v.iter().any(|c| c > &BigInt::zero())
            })
            .cloned()
            .collect();

        for v in &mut semiflows {
            while v.len() < transition_count {
                v.push(BigInt::zero());
            }
        }

        semiflows = Self::prune_dominated(&semiflows);

        let result = semiflows
            .into_iter()
            .map(|coeffs| {
                let support: Vec<TransitionId> = coeffs
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| **c > BigInt::zero())
                    .map(|(i, _)| TransitionId(i as u32))
                    .collect();

                TInvariant {
                    coefficients: coeffs,
                    is_semiflow: true,
                    is_minimal: true,
                    support,
                    is_realizable: None,
                    realization_witness: None,
                }
            })
            .collect();

        Ok(result)
    }

    fn prune_dominated(vectors: &[Vec<BigInt>]) -> Vec<Vec<BigInt>> {
        let mut result = Vec::new();
        let supports: Vec<HashSet<usize>> = vectors
            .iter()
            .map(|v| {
                v.iter()
                    .enumerate()
                    .filter(|(_, c)| **c > BigInt::zero())
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();

        'outer: for (i, vec) in vectors.iter().enumerate() {
            for other_support in &supports {
                if other_support.len() < supports[i].len() && other_support.is_subset(&supports[i])
                {
                    // Skip if support of another is strictly smaller
                    // But don't skip if it's the same support
                    continue 'outer;
                }
            }
            result.push(vec.clone());
        }

        result
    }
}

const REALIZATION_STATE_LIMIT: usize = 100_000;

enum RealizationResult {
    Found(Vec<TransitionId>),
    NotFound,
    Inconclusive,
}

/// Search exactly for a firing sequence whose Parikh vector equals `counts`.
/// This is the feasible-T-invariant definition from Popova-Zeugmann: an
/// algebraic T-invariant is realizable only when it is a concrete firing
/// sequence from the initial marking.
fn realize_t_invariant(net: &PetriNet, counts: &[BigInt]) -> RealizationResult {
    if counts.iter().any(|count| count < &BigInt::zero()) {
        return RealizationResult::NotFound;
    }
    let mut remaining = counts.to_vec();
    let mut memo = HashSet::new();
    let mut explored = 0usize;
    match search_realization(
        net,
        net.initial_marking().clone(),
        &mut remaining,
        &mut memo,
        &mut explored,
    ) {
        Some(sequence) => RealizationResult::Found(sequence),
        None if explored >= REALIZATION_STATE_LIMIT => RealizationResult::Inconclusive,
        None => RealizationResult::NotFound,
    }
}

fn search_realization(
    net: &PetriNet,
    marking: racon_core::marking::Marking,
    remaining: &mut [BigInt],
    memo: &mut HashSet<(racon_core::marking::Marking, Vec<BigInt>)>,
    explored: &mut usize,
) -> Option<Vec<TransitionId>> {
    if remaining.iter().all(|count| count.is_zero()) {
        return Some(Vec::new());
    }
    if *explored >= REALIZATION_STATE_LIMIT || !memo.insert((marking.clone(), remaining.to_vec())) {
        return None;
    }
    *explored += 1;

    for transition in net.transitions() {
        let index = transition.id.0 as usize;
        if remaining[index].is_zero() || !net.is_enabled(transition.id, &marking) {
            continue;
        }
        let next_marking = net.fire(transition.id, &marking);
        remaining[index] -= BigInt::from(1);
        if let Some(mut suffix) = search_realization(net, next_marking, remaining, memo, explored) {
            remaining[index] += BigInt::from(1);
            suffix.insert(0, transition.id);
            return Some(suffix);
        }
        remaining[index] += BigInt::from(1);
    }
    None
}

fn normalize_bigint(vector: &mut [BigInt]) {
    let gcd = vector
        .iter()
        .map(Signed::abs)
        .filter(|value| !value.is_zero())
        .fold(BigInt::zero(), |left, right| left.gcd(&right));
    if gcd > BigInt::from(1) {
        for value in vector.iter_mut() {
            *value /= gcd.clone();
        }
    }
    if let Some(first) = vector.iter().find(|value| !value.is_zero()) {
        if first < &BigInt::zero() {
            for value in vector.iter_mut() {
                *value = -value.clone();
            }
        }
    }
}

fn verify_minimal_nonnegative_vectors(
    vectors: &[Vec<BigInt>],
    dimension: usize,
) -> Result<(), String> {
    for (index, vector) in vectors.iter().enumerate() {
        if vector.len() != dimension
            || vector.iter().all(|value| value.is_zero())
            || vector.iter().any(|value| value < &BigInt::zero())
        {
            return Err(format!(
                "vector {index} is not a non-zero non-negative vector of dimension {dimension}"
            ));
        }
        let gcd = vector
            .iter()
            .map(Signed::abs)
            .filter(|value| !value.is_zero())
            .fold(BigInt::zero(), |left, right| left.gcd(&right));
        if gcd != BigInt::from(1) {
            return Err(format!("vector {index} is not primitive (gcd={gcd})"));
        }
        for (other_index, other) in vectors.iter().enumerate() {
            if index == other_index {
                continue;
            }
            let componentwise_dominates =
                other.iter().zip(vector).all(|(left, right)| left <= right);
            let strict = other.iter().zip(vector).any(|(left, right)| left < right);
            if componentwise_dominates && strict {
                return Err(format!(
                    "vector {index} is not minimal because of vector {other_index}"
                ));
            }
        }
    }
    Ok(())
}

/// Exhaustively demonstrate support-completeness for all non-negative kernel
/// vectors up to `coefficient_bound`. This is an independently checkable
/// finite certificate; `None` is deliberately not converted into a global
/// completeness claim for unbounded coefficients.
pub fn verify_p_semiflow_completeness(
    matrix: &racon_core::matrix::IncidenceMatrix,
    invariants: &[PInvariant],
    coefficient_bound: i64,
) -> Result<(), String> {
    let vectors = invariants
        .iter()
        .map(|invariant| invariant.coefficients.clone())
        .collect::<Vec<_>>();
    verify_bounded_kernel_support_completeness(matrix, &vectors, coefficient_bound, true)
}

pub fn certify_p_semiflow_completeness(
    matrix: &racon_core::matrix::IncidenceMatrix,
    invariants: &[PInvariant],
    coefficient_bound: i64,
) -> Result<SemiflowCompletenessCertificate, String> {
    verify_p_semiflow_completeness(matrix, invariants, coefficient_bound)?;
    Ok(SemiflowCompletenessCertificate {
        coefficient_bound,
        dimension: matrix.place_count(),
        verified: true,
    })
}

pub fn verify_t_semiflow_completeness(
    matrix: &racon_core::matrix::IncidenceMatrix,
    invariants: &[TInvariant],
    coefficient_bound: i64,
) -> Result<(), String> {
    let vectors = invariants
        .iter()
        .map(|invariant| invariant.coefficients.clone())
        .collect::<Vec<_>>();
    verify_bounded_kernel_support_completeness(matrix, &vectors, coefficient_bound, false)
}

pub fn certify_t_semiflow_completeness(
    matrix: &racon_core::matrix::IncidenceMatrix,
    invariants: &[TInvariant],
    coefficient_bound: i64,
) -> Result<SemiflowCompletenessCertificate, String> {
    verify_t_semiflow_completeness(matrix, invariants, coefficient_bound)?;
    Ok(SemiflowCompletenessCertificate {
        coefficient_bound,
        dimension: matrix.transition_count(),
        verified: true,
    })
}

fn verify_bounded_kernel_support_completeness(
    matrix: &racon_core::matrix::IncidenceMatrix,
    invariants: &[Vec<BigInt>],
    coefficient_bound: i64,
    place_kernel: bool,
) -> Result<(), String> {
    if coefficient_bound < 1 {
        return Err("coefficient bound must be positive".into());
    }
    let dimension = if place_kernel {
        matrix.place_count()
    } else {
        matrix.transition_count()
    };
    if dimension > 8 {
        return Err("bounded completeness certificate is limited to dimension <= 8".into());
    }
    let supports = invariants
        .iter()
        .map(|vector| {
            vector
                .iter()
                .enumerate()
                .filter(|(_, value)| **value > BigInt::zero())
                .map(|(index, _)| index)
                .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut candidate = vec![BigInt::zero(); dimension];
    enumerate_nonnegative_kernel_vectors(
        matrix,
        &mut candidate,
        0,
        coefficient_bound,
        place_kernel,
        &supports,
    )
}

fn enumerate_nonnegative_kernel_vectors(
    matrix: &racon_core::matrix::IncidenceMatrix,
    candidate: &mut [BigInt],
    index: usize,
    bound: i64,
    place_kernel: bool,
    supports: &[HashSet<usize>],
) -> Result<(), String> {
    if index < candidate.len() {
        for value in 0..=bound {
            candidate[index] = BigInt::from(value);
            enumerate_nonnegative_kernel_vectors(
                matrix,
                candidate,
                index + 1,
                bound,
                place_kernel,
                supports,
            )?;
        }
        return Ok(());
    }
    if candidate.iter().all(|value| value.is_zero()) {
        return Ok(());
    }
    let is_kernel = if place_kernel {
        matrix
            .left_multiply(candidate)
            .iter()
            .all(|value| value.is_zero())
    } else {
        matrix
            .right_multiply(candidate)
            .iter()
            .all(|value| value.is_zero())
    };
    if !is_kernel {
        return Ok(());
    }
    let candidate_support = candidate
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > BigInt::zero())
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    if supports
        .iter()
        .any(|support| support.is_subset(&candidate_support))
    {
        Ok(())
    } else {
        Err(format!(
            "kernel vector {candidate:?} has no generated minimal support"
        ))
    }
}

// ---------------------------------------------------------------------------
// Verification helpers
// ---------------------------------------------------------------------------

/// Verify that all P-invariants satisfy y^T * C = 0.
pub fn verify_p_invariants(
    c: &racon_core::matrix::IncidenceMatrix,
    p_invariants: &[PInvariant],
) -> Result<(), String> {
    let m = c.place_count();
    let n = c.transition_count();

    for (idx, inv) in p_invariants.iter().enumerate() {
        if inv.coefficients.len() != m {
            return Err(format!(
                "P-invariant {} has {} coefficients, expected {}",
                idx,
                inv.coefficients.len(),
                m
            ));
        }
        for j in 0..n {
            let dot: BigInt = inv
                .coefficients
                .iter()
                .enumerate()
                .map(|(i, y)| y * &c.data()[i][j])
                .sum();
            if !dot.is_zero() {
                return Err(format!(
                    "P-invariant {} fails: y^T * C[:,{}] = {}, expected 0",
                    idx, j, dot
                ));
            }
        }
    }
    Ok(())
}

/// Verify that all T-invariants satisfy C * x = 0.
pub fn verify_t_invariants(
    c: &racon_core::matrix::IncidenceMatrix,
    t_invariants: &[TInvariant],
) -> Result<(), String> {
    let m = c.place_count();
    let n = c.transition_count();

    for (idx, inv) in t_invariants.iter().enumerate() {
        if inv.coefficients.len() != n {
            return Err(format!(
                "T-invariant {} has {} coefficients, expected {}",
                idx,
                inv.coefficients.len(),
                n
            ));
        }
        for i in 0..m {
            let dot: BigInt = inv
                .coefficients
                .iter()
                .enumerate()
                .map(|(j, x)| &c.data()[i][j] * x)
                .sum();
            if !dot.is_zero() {
                return Err(format!(
                    "T-invariant {} fails: C[{}] * x = {}, expected 0",
                    idx, i, dot
                ));
            }
        }
    }
    Ok(())
}

/// Derive structural properties (CPI, CTI, conservative) from invariant sets.
pub fn derive_structural_properties(
    net: &PetriNet,
    p_semiflows: &[PInvariant],
    t_semiflows: &[TInvariant],
) -> (bool, bool, bool) {
    let m = net.places().len();
    let n = net.transitions().len();

    // Conservativeness: exists a P-semiflow with all coefficients > 0
    let is_conservative = p_semiflows
        .iter()
        .any(|inv| inv.coefficients.iter().all(|c| c > &BigInt::zero()));

    // CPI: every place in support of at least one P-semiflow
    let mut covered_places = vec![false; m];
    for inv in p_semiflows {
        for &pid in &inv.support {
            covered_places[pid.0 as usize] = true;
        }
    }
    let is_cpi = m > 0 && covered_places.iter().all(|&c| c);

    // CTI: every transition in support of at least one T-semiflow
    let mut covered_trans = vec![false; n];
    for inv in t_semiflows {
        for &tid in &inv.support {
            covered_trans[tid.0 as usize] = true;
        }
    }
    let is_cti = n > 0 && covered_trans.iter().all(|&c| c);

    (is_conservative, is_cpi, is_cti)
}

/// Find a positive integer certificate for Murata's structural boundedness
/// condition C^T y <= 0 with y > 0 (Murata 1989; INA "Fast Boundedness Test").
///
/// The condition is homogeneous, so `y > 0` is equivalent to `y >= 1`. The
/// resulting linear feasibility problem is solved with a floating-point
/// phase-1 simplex (Bland's rule), the solution is scaled to integers, and the
/// integer vector is then **verified exactly** with BigInt arithmetic before it
/// is returned. A solver failure or a failed verification yields `None`,
/// which is inconclusive and never a proof of unboundedness.
pub fn structural_boundedness_certificate(net: &PetriNet) -> Option<Vec<BigInt>> {
    let matrix = net.incidence_matrix();
    let places = matrix.place_count();
    let transitions = matrix.transition_count();
    if places == 0 {
        return None;
    }
    if transitions == 0 {
        return Some(vec![BigInt::one(); places]);
    }
    let coefficient = |p: usize, t: usize| -> f64 {
        matrix
            .get(p, t)
            .and_then(|value| num_traits::ToPrimitive::to_f64(&value))
            .unwrap_or(0.0)
    };

    // z = y - 1 >= 0 :  sum_p C[p][t] z_p <= -sum_p C[p][t]
    let z = feasible_point_simplex(places, transitions, &coefficient)?;
    for scale in [1u32, 2, 3, 4, 6, 8, 12, 16, 24, 48, 120, 720, 5040] {
        let candidate: Vec<BigInt> = z
            .iter()
            .map(|value| {
                let y = (value + 1.0) * f64::from(scale);
                BigInt::from(y.round().max(1.0) as i64)
            })
            .collect();
        if verify_structural_boundedness_certificate(matrix, &candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Phase-1 simplex for `A z <= b, z >= 0` where `A[t][p] = C[p][t]` and
/// `b[t] = -sum_p C[p][t]`. Returns a feasible `z` or `None`.
fn feasible_point_simplex(
    n: usize,
    m: usize,
    c: &dyn Fn(usize, usize) -> f64,
) -> Option<Vec<f64>> {
    let a: Vec<Vec<f64>> = (0..m).map(|t| (0..n).map(|p| c(p, t)).collect()).collect();
    let b: Vec<f64> = (0..m)
        .map(|t| -(0..n).map(|p| c(p, t)).sum::<f64>())
        .collect();
    feasible_dense(n, &a, &b)
}

/// Phase-1 simplex (Bland's rule) for the dense system `A z <= b, z >= 0`
/// with `n` variables. Returns a feasible point or `None` (infeasible or
/// numerically inconclusive; callers must verify results exactly).
fn feasible_dense(n: usize, a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    const EPS: f64 = 1e-9;
    let m = a.len();
    // Columns: z (n) | slack (m) | artificial (<= m) | rhs.
    let mut rows: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut art_rows: Vec<usize> = Vec::new();
    for t in 0..m {
        let sign = if b[t] < 0.0 { -1.0 } else { 1.0 };
        let mut row = vec![0.0; n + m + m + 1];
        for p in 0..n {
            row[p] = sign * a[t][p];
        }
        row[n + t] = sign;
        row[n + 2 * m] = sign * b[t];
        if sign < 0.0 {
            row[n + m + t] = 1.0;
            art_rows.push(t);
        }
        rows.push(row);
    }
    if art_rows.is_empty() {
        return Some(vec![0.0; n]);
    }
    let width = n + 2 * m + 1;
    let mut basis: Vec<usize> = (0..m)
        .map(|t| if art_rows.contains(&t) { n + m + t } else { n + t })
        .collect();
    // Objective: minimise sum of artificials -> reduced costs.
    let mut obj = vec![0.0; width];
    for &t in &art_rows {
        for j in 0..width {
            if j < n + m || j == n + 2 * m {
                obj[j] -= rows[t][j];
            }
        }
    }
    for _ in 0..50_000 {
        // Bland: smallest index with negative reduced cost.
        let Some(enter) = (0..n + 2 * m).find(|&j| obj[j] < -EPS) else {
            break;
        };
        let mut leave: Option<usize> = None;
        let mut best = f64::INFINITY;
        for (r, row) in rows.iter().enumerate() {
            if row[enter] > EPS {
                let ratio = row[width - 1] / row[enter];
                if ratio < best - EPS
                    || ((ratio - best).abs() <= EPS
                        && leave.is_some_and(|l| basis[r] < basis[l]))
                {
                    best = ratio;
                    leave = Some(r);
                }
            }
        }
        let leave = leave?;
        let pivot = rows[leave][enter];
        for value in rows[leave].iter_mut() {
            *value /= pivot;
        }
        let pivot_row = rows[leave].clone();
        for (r, row) in rows.iter_mut().enumerate() {
            if r != leave && row[enter].abs() > 0.0 {
                let factor = row[enter];
                for j in 0..width {
                    row[j] -= factor * pivot_row[j];
                }
            }
        }
        let factor = obj[enter];
        for j in 0..width {
            obj[j] -= factor * pivot_row[j];
        }
        basis[leave] = enter;
    }
    // Phase-1 optimum is -obj[rhs]; feasible iff it is (numerically) zero.
    if obj[width - 1].abs() > 1e-7 {
        return None;
    }
    let mut z = vec![0.0; n];
    for (r, &col) in basis.iter().enumerate() {
        if col < n {
            z[col] = rows[r][width - 1].max(0.0);
        }
    }
    Some(z)
}

/// Find a certificate that the net is **not** structurally bounded.
///
/// By Farkas' lemma there is no `y > 0` with `C^T y <= 0` (Murata) exactly when
/// there is a firing-count vector `x >= 0` with `C x >= 0` and `C x != 0`: a
/// repeatable multiset of transitions that never removes tokens from any place
/// and adds tokens to at least one. This is the decision INA reports as
/// "structurally bounded: N". The condition concerns the incidence matrix
/// (the ordinary-arc projection); inhibitor arcs and priorities are ignored.
///
/// The simplex runs in floating point, but the integer vector is verified
/// exactly with BigInt arithmetic by
/// [`verify_structural_unboundedness_certificate`]; a solver failure yields
/// `None`, which is inconclusive.
pub fn structural_unboundedness_certificate(net: &PetriNet) -> Option<Vec<BigInt>> {
    let matrix = net.incidence_matrix();
    let places = matrix.place_count();
    let transitions = matrix.transition_count();
    if places == 0 || transitions == 0 {
        return None;
    }
    let coefficient = |p: usize, t: usize| -> f64 {
        matrix
            .get(p, t)
            .and_then(|value| num_traits::ToPrimitive::to_f64(&value))
            .unwrap_or(0.0)
    };
    // Variables x_t >= 0.  -(C x)_p <= 0 for every place, and the homogeneous
    // normalisation  -sum_p (C x)_p <= -1  forces C x != 0.
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(places + 1);
    let mut b: Vec<f64> = Vec::with_capacity(places + 1);
    for p in 0..places {
        a.push((0..transitions).map(|t| -coefficient(p, t)).collect());
        b.push(0.0);
    }
    a.push(
        (0..transitions)
            .map(|t| -(0..places).map(|p| coefficient(p, t)).sum::<f64>())
            .collect(),
    );
    b.push(-1.0);
    let x = feasible_dense(transitions, &a, &b)?;
    for scale in [1u32, 2, 3, 4, 6, 8, 12, 16, 24, 48, 120, 720, 5040] {
        let candidate: Vec<BigInt> = x
            .iter()
            .map(|value| BigInt::from((value * f64::from(scale)).round().max(0.0) as i64))
            .collect();
        if verify_structural_unboundedness_certificate(matrix, &candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Validate a certificate of structural unboundedness: `x >= 0`, `C x >= 0`
/// componentwise and `C x != 0`. Never searches for a certificate.
pub fn verify_structural_unboundedness_certificate(
    matrix: &racon_core::matrix::IncidenceMatrix,
    certificate: &[BigInt],
) -> bool {
    if certificate.len() != matrix.transition_count()
        || certificate.iter().any(|value| value < &BigInt::zero())
    {
        return false;
    }
    let effect = matrix.right_multiply(certificate);
    effect.iter().all(|value| value >= &BigInt::zero())
        && effect.iter().any(|value| value > &BigInt::zero())
}

/// Validate Murata's structural-boundedness certificate `Aᵀy <= 0`.
///
/// The certificate is accepted only when it has one strictly positive
/// coefficient per place and every transition column satisfies the inequality.
/// This function deliberately does not search for a certificate; absence of a
/// small certificate is inconclusive and must not be treated as unboundedness.
pub fn verify_structural_boundedness_certificate(
    matrix: &racon_core::matrix::IncidenceMatrix,
    certificate: &[BigInt],
) -> bool {
    certificate.len() == matrix.place_count()
        && certificate.iter().all(|value| value > &BigInt::zero())
        && matrix
            .left_multiply(certificate)
            .iter()
            .all(|value| value <= &BigInt::zero())
}

// =============================================================================
// Place inequality invariants (direction-based one-sided invariants)
// =============================================================================
//
// IMPORTANT: This inner mathematical core is DIRECTION-BASED and must NOT
// contain the public terms SUB/SUR (those live only in the DTO/semantic layer
// of racon-engine / racon-api). The relations here are:
//
//   LessOrEqual    -> place vector y with y^T * C + slack = 0, slack >= 0
//                     (residual y^T*C is componentwise <= 0; non-increasing)
//   GreaterOrEqual -> place vector y with y^T * C - slack = 0, slack >= 0
//                     (residual y^T*C is componentwise >= 0; non-decreasing)
//
// The slack coordinates are certificates for the elimination, but the returned
// residual is NEVER taken from the slack: it is recomputed independently as
// y^T * C by exact arithmetic (spec §9, §10, §13, §20).
//
// The slack-augmented matrix B = [C; slack_block] exploits the same pairwise
// elimination used for P-semiflows: the left kernel of B contains [y; slack]
// with slack >= 0, and y^T*C +- slack = 0 pins the residual sign.

/// Compute minimal non-negative place inequality invariant vectors for a dense
/// incidence matrix `C` (rows = places, cols = transitions) and one relation.
///
/// Slack-augmentation (§15/§16): build B by stacking `C` over the slack block
/// (identity for `LessOrEqual`, negated identity for `GreaterOrEqual`), run the
/// Starke pairwise elimination, project each kernel vector onto its first
/// `place_count` coordinates (dropping the slack coordinates), then re-derive
/// the residual `r = y^T C` exactly and independently and classify proper/
/// exact/proper-support/minimal status.
///
/// The slack coordinates are NOT reported; `support` and `residual` are
/// recomputed from the projected coefficients only.
pub fn compute_minimal_place_inequality_invariants(
    matrix: &[Vec<BigInt>],
    relation: InequalityRelation,
    context: &AnalysisContext,
) -> Result<Vec<PlaceInequalityInvariant>, InvariantError> {
    let rows = matrix.len();
    let cols = if rows == 0 { 0 } else { matrix[0].len() };
    if rows == 0 {
        return Ok(Vec::new());
    }

    let slack_multiplier: BigInt = match relation {
        InequalityRelation::LessOrEqual => BigInt::one(),
        InequalityRelation::GreaterOrEqual => BigInt::from(-1),
    };

    let mut augmented: Vec<Vec<BigInt>> = matrix.to_vec();
    for t in 0..cols {
        let mut slack_row = vec![BigInt::zero(); cols];
        slack_row[t] = slack_multiplier.clone();
        augmented.push(slack_row);
    }

    let candidate_vectors =
        InvariantEngineImpl::compute_minimal_nonnegative_kernel_vectors(&augmented, context)?;

    let mut invariants: Vec<PlaceInequalityInvariant> = Vec::new();
    for kernel in candidate_vectors {
        let mut coefficients: Vec<BigInt> = kernel[..rows].to_vec();
        normalize_bigint(&mut coefficients);
        // Recompute residual independently: r_j = (y^T * C)_j.
        let mut residual = vec![BigInt::zero(); cols];
        for t in 0..cols {
            let mut acc = BigInt::zero();
            for (p, y) in coefficients.iter().enumerate() {
                acc += y * &matrix[p][t];
            }
            residual[t] = acc;
        }
        // Relation enforcement (needed in case elimination emitted a kernel
        // that does not actually satisfy the requested one-sided residual).
        let satisfied = match relation {
            InequalityRelation::LessOrEqual => residual.iter().all(|v| v <= &BigInt::zero()),
            InequalityRelation::GreaterOrEqual => {
                residual.iter().all(|v| v >= &BigInt::zero())
            }
        };
        if !satisfied || coefficients.iter().all(BigInt::is_zero) {
            continue;
        }
        let is_exact = residual.iter().all(|v| v.is_zero());
        let has_strict = match relation {
            InequalityRelation::LessOrEqual => residual.iter().any(|v| v < &BigInt::zero()),
            InequalityRelation::GreaterOrEqual => residual.iter().any(|v| v > &BigInt::zero()),
        };
        let support: Vec<PlaceId> = coefficients
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_zero())
            .map(|(i, _)| PlaceId(i as u32))
            .collect();
        invariants.push(PlaceInequalityInvariant {
            coefficients,
            relation,
            residual,
            is_proper: has_strict,
            is_exact,
            is_minimal: true,
            support,
            initial_value: None,
        });
    }

    invariants.sort_by(|a, b| {
        let a_keys: Vec<u32> = a.support.iter().map(|p| p.0).collect();
        let b_keys: Vec<u32> = b.support.iter().map(|p| p.0).collect();
        a_keys.cmp(&b_keys)
    });
    Ok(invariants)
}

/// Verify a collection of place inequality invariants against a dense
/// incidence matrix and a declared relation.
///
/// Residuals are recomputed independently via exact arithmetic and each
/// returned invariant is re-checked against the declared relation (spec §30):
///
/// - `LessOrEqual`: every residual component must be `<= 0`.
/// - `GreaterOrEqual`: every residual component must be `>= 0`.
///
/// The zero vector is never an acceptable invariant.
#[allow(dead_code)]
pub fn verify_place_inequality_invariants(
    matrix: &[Vec<BigInt>],
    relation: InequalityRelation,
    invariants: &[PlaceInequalityInvariant],
) -> Result<(), String> {
    let rows = matrix.len();
    let cols = if rows == 0 { 0 } else { matrix[0].len() };
    for (idx, inv) in invariants.iter().enumerate() {
        if inv.coefficients.len() != rows {
            return Err(format!(
                "inequality invariant {} has {} coefficients, expected {}",
                idx,
                inv.coefficients.len(),
                rows
            ));
        }
        if inv.coefficients.iter().all(BigInt::is_zero) {
            return Err(format!("inequality invariant {} is the zero vector", idx));
        }
        for t in 0..cols {
            let mut acc = BigInt::zero();
            for (p, y) in inv.coefficients.iter().enumerate() {
                acc += y * &matrix[p][t];
            }
            let target = &inv.residual[t];
            if &acc != target {
                return Err(format!(
                    "inequality invariant {} violates residual at transition {}: got {}, declared {}",
                    idx, t, acc, target
                ));
            }
            let ok = match relation {
                InequalityRelation::LessOrEqual => acc <= BigInt::zero(),
                InequalityRelation::GreaterOrEqual => acc >= BigInt::zero(),
            };
            if !ok {
                return Err(format!(
                    "inequality invariant {} violates relation at transition {} (residual {})",
                    idx, t, acc
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Inequality completeness certificate (bounded Starke on slack-augmented matrix)
// ---------------------------------------------------------------------------

/// Left-multiply raw dense matrix by a vector: result[j] = sum_i(v[i] * m[i][j]).
fn left_multiply_raw(matrix: &[Vec<BigInt>], vector: &[BigInt]) -> Vec<BigInt> {
    if matrix.is_empty() {
        return Vec::new();
    }
    let cols = matrix[0].len();
    let mut result = vec![BigInt::zero(); cols];
    for j in 0..cols {
        let mut acc = BigInt::zero();
        for (i, v) in vector.iter().enumerate() {
            acc += v * &matrix[i][j];
        }
        result[j] = acc;
    }
    result
}

/// Enumerate all non-negative left-kernel vectors of a raw matrix up to a
/// per-component coefficient bound, checking that each discovered support
/// is covered by one of the known minimal supports.
fn enumerate_nonnegative_kernel_vectors_raw(
    matrix: &[Vec<BigInt>],
    candidate: &mut [BigInt],
    index: usize,
    bound: i64,
    known_supports: &[HashSet<usize>],
) -> Result<(), String> {
    if index < candidate.len() {
        for value in 0..=bound {
            candidate[index] = BigInt::from(value);
            enumerate_nonnegative_kernel_vectors_raw(
                matrix,
                candidate,
                index + 1,
                bound,
                known_supports,
            )?;
        }
        return Ok(());
    }
    if candidate.iter().all(|v| v.is_zero()) {
        return Ok(());
    }
    let residual = left_multiply_raw(matrix, candidate);
    if !residual.iter().all(|v| v.is_zero()) {
        return Ok(());
    }
    let candidate_support: HashSet<usize> = candidate
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > BigInt::zero())
        .map(|(i, _)| i)
        .collect();
    if known_supports
        .iter()
        .any(|s| s.is_subset(&candidate_support))
    {
        Ok(())
    } else {
        Err(format!(
            "kernel vector {candidate:?} has no generated minimal support"
        ))
    }
}

/// Verify support-completeness of inequality invariants via bounded
/// enumeration on the slack-augmented matrix.
fn verify_inequality_support_completeness(
    matrix: &[Vec<BigInt>],
    invariants: &[PlaceInequalityInvariant],
    coefficient_bound: i64,
) -> Result<(), String> {
    if coefficient_bound < 1 {
        return Err("coefficient bound must be positive".into());
    }
    let place_count = matrix.len();
    let trans_count = if place_count == 0 {
        0
    } else {
        matrix[0].len()
    };
    let augmented_dim = place_count + trans_count;
    if augmented_dim > 8 {
        return Err(
            "bounded completeness certificate is limited to augmented dimension <= 8".into(),
        );
    }

    // Build slack-augmented matrix B (rows = place_count + trans_count, cols = trans_count).
    let slack_multiplier: BigInt = match invariants.first().map(|i| i.relation) {
        Some(InequalityRelation::LessOrEqual) | None => BigInt::one(),
        Some(InequalityRelation::GreaterOrEqual) => BigInt::from(-1),
    };
    let mut augmented: Vec<Vec<BigInt>> = matrix.to_vec();
    for t in 0..trans_count {
        let mut slack_row = vec![BigInt::zero(); trans_count];
        slack_row[t] = slack_multiplier.clone();
        augmented.push(slack_row);
    }

    // Project invariant supports into augmented-index space (first place_count entries).
    let known_supports: Vec<HashSet<usize>> = invariants
        .iter()
        .map(|inv| inv.support.iter().map(|p| p.0 as usize).collect())
        .collect();

    let mut candidate = vec![BigInt::zero(); augmented_dim];
    enumerate_nonnegative_kernel_vectors_raw(
        &augmented,
        &mut candidate,
        0,
        coefficient_bound,
        &known_supports,
    )
}

/// Bounded completeness certificate for place inequality invariants.
///
/// Enumerates all non-negative left-kernel vectors of the slack-augmented
/// matrix up to `coefficient_bound` and verifies that each has a support
/// covered by one of the reported invariants. Limited to augmented
/// dimension (place_count + transition_count) <= 8.
pub fn certify_place_inequality_completeness(
    matrix: &[Vec<BigInt>],
    invariants: &[PlaceInequalityInvariant],
    coefficient_bound: i64,
) -> Result<InequalityCompletenessCertificate, String> {
    let relation = invariants
        .first()
        .map(|i| i.relation)
        .unwrap_or(InequalityRelation::LessOrEqual);
    verify_inequality_support_completeness(matrix, invariants, coefficient_bound)?;
    Ok(InequalityCompletenessCertificate {
        coefficient_bound,
        dimension: matrix.len(),
        relation,
        verified: true,
    })
}

/// Verify the declared relation of a completeness certificate against
/// a set of invariants (recomputes residual independently).
pub fn verify_place_inequality_completeness(
    matrix: &[Vec<BigInt>],
    invariants: &[PlaceInequalityInvariant],
    certificate: &InequalityCompletenessCertificate,
) -> Result<(), String> {
    verify_inequality_support_completeness(matrix, invariants, certificate.coefficient_bound)?;
    verify_place_inequality_invariants(matrix, certificate.relation, invariants)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// State-equation test (INA "non-reachability test of a partial/full marking")
// ---------------------------------------------------------------------------

/// Outcome of testing the state equation `M = M0 + C·x, x >= 0` for a target
/// marking. The state equation is only a NECESSARY condition for reachability:
/// `Infeasible` is a proof of non-reachability, the other outcomes are not a
/// proof of reachability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateEquationVerdict {
    /// No `x >= 0` solves the equation. `farkas` is an exactly verified vector
    /// `y` with `y^T C >= 0` and `y^T (M - M0) < 0` (Farkas' lemma), so
    /// `y^T (M - M0) = y^T C x >= 0` is contradicted for every firing count `x`.
    Infeasible { farkas: Vec<BigInt> },
    /// An integer firing-count (Parikh) vector `x >= 0` with `C x = M - M0`,
    /// verified exactly. The marking is still only *possibly* reachable.
    Solution { parikh: Vec<BigInt> },
    /// The solver found neither a certificate nor an exact integer solution.
    Inconclusive,
}

fn verify_farkas_certificate(matrix: &racon_core::IncidenceMatrix, delta: &[BigInt], y: &[BigInt]) -> bool {
    if y.iter().all(Zero::is_zero) {
        return false;
    }
    let dot_delta: BigInt = y.iter().zip(delta).map(|(a, b)| a * b).sum();
    if !dot_delta.is_negative() {
        return false;
    }
    (0..matrix.transition_count()).all(|t| {
        let value: BigInt = (0..matrix.place_count())
            .map(|p| &y[p] * matrix.get(p, t).unwrap_or_else(BigInt::zero))
            .sum();
        !value.is_negative()
    })
}

fn verify_state_equation_solution(matrix: &racon_core::IncidenceMatrix, delta: &[BigInt], x: &[BigInt]) -> bool {
    x.iter().all(|v| !v.is_negative())
        && (0..matrix.place_count()).all(|p| {
            let value: BigInt = (0..matrix.transition_count())
                .map(|t| matrix.get(p, t).unwrap_or_else(BigInt::zero) * &x[t])
                .sum();
            value == delta[p]
        })
}

const SCALES: [u32; 13] = [1, 2, 3, 4, 6, 8, 12, 16, 24, 48, 120, 720, 5040];

/// Test the state equation for `target` (one token count per place).
///
/// Both searches use the floating-point phase-1 simplex already used for
/// Murata's criterion; every result is **verified exactly** with BigInt
/// arithmetic before it is reported, so a numeric failure only ever yields
/// `Inconclusive`, never a wrong proof.
pub fn state_equation_test(net: &PetriNet, target: &[BigInt]) -> StateEquationVerdict {
    let matrix = net.incidence_matrix();
    let places = matrix.place_count();
    let transitions = matrix.transition_count();
    if target.len() != places || places == 0 {
        return StateEquationVerdict::Inconclusive;
    }
    let mut delta: Vec<BigInt> = Vec::with_capacity(places);
    for (p, wanted) in target.iter().enumerate() {
        let initial = match net.initial_marking().tokens.get(p) {
            Some(racon_core::TokenValue::Finite(v)) => BigInt::from(*v),
            Some(racon_core::TokenValue::Exact(v)) => BigInt::from(v.clone()),
            _ => return StateEquationVerdict::Inconclusive,
        };
        delta.push(wanted - initial);
    }
    let c = |p: usize, t: usize| -> f64 {
        matrix.get(p, t).and_then(|v| num_traits::ToPrimitive::to_f64(&v)).unwrap_or(0.0)
    };
    let b = |p: usize| -> f64 { num_traits::ToPrimitive::to_f64(&delta[p]).unwrap_or(0.0) };

    // 1) Farkas certificate. y = y+ - y-, with y+, y- >= 0:
    //      -C^T y <= 0   (y^T C >= 0)   and   b^T y <= -1   (y^T b < 0, scaled).
    let n = 2 * places;
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(transitions + 1);
    let mut rhs: Vec<f64> = Vec::with_capacity(transitions + 1);
    for t in 0..transitions {
        let mut row = vec![0.0; n];
        for p in 0..places {
            row[p] = -c(p, t);
            row[places + p] = c(p, t);
        }
        a.push(row);
        rhs.push(0.0);
    }
    let mut row = vec![0.0; n];
    for p in 0..places {
        row[p] = b(p);
        row[places + p] = -b(p);
    }
    a.push(row);
    rhs.push(-1.0);
    if let Some(z) = feasible_dense(n, &a, &rhs) {
        for scale in SCALES {
            let y: Vec<BigInt> = (0..places)
                .map(|p| BigInt::from(((z[p] - z[places + p]) * f64::from(scale)).round() as i64))
                .collect();
            if verify_farkas_certificate(matrix, &delta, &y) {
                return StateEquationVerdict::Infeasible { farkas: y };
            }
        }
    }

    // 2) An exact integer solution of C x = delta, x >= 0 (as C x <= delta and -C x <= -delta).
    if transitions > 0 {
        let mut a: Vec<Vec<f64>> = Vec::with_capacity(2 * places);
        let mut rhs: Vec<f64> = Vec::with_capacity(2 * places);
        for p in 0..places {
            a.push((0..transitions).map(|t| c(p, t)).collect());
            rhs.push(b(p));
        }
        for p in 0..places {
            a.push((0..transitions).map(|t| -c(p, t)).collect());
            rhs.push(-b(p));
        }
        if let Some(x) = feasible_dense(transitions, &a, &rhs) {
            let candidate: Vec<BigInt> = x.iter().map(|v| BigInt::from(v.round() as i64)).collect();
            if verify_state_equation_solution(matrix, &delta, &candidate) {
                return StateEquationVerdict::Solution { parikh: candidate };
            }
        }
    } else if delta.iter().all(Zero::is_zero) {
        return StateEquationVerdict::Solution { parikh: Vec::new() };
    }
    StateEquationVerdict::Inconclusive
}

// ---------------------------------------------------------------------------
// Basis of ALL invariants (INA "compute a basis for all P/T-invariants")
// ---------------------------------------------------------------------------

/// A basis of the rational kernel, scaled to primitive integer vectors.
///
/// Unlike semiflows (non-negative, minimal support), the basis spans **every**
/// integer invariant, including those with negative entries: any invariant is a
/// rational combination of these vectors. `dimension = n - rank`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelBasis {
    pub vectors: Vec<Vec<BigInt>>,
    pub rank: usize,
    pub dimension: usize,
}

/// Kernel basis of `a` (`a · v = 0`), by fraction-free Gauss–Jordan elimination.
///
/// Rows are cross-multiplied and divided by their gcd, so every intermediate
/// value is an exact integer. For each free column `f`, the vector with
/// `v_f = L` (the lcm of the pivot values) and `v_pivot = -a[row][f] · L / pivot`
/// solves the system exactly; it is then divided by its gcd.
fn integer_kernel_basis(a: &[Vec<BigInt>], columns: usize) -> KernelBasis {
    let mut rows: Vec<Vec<BigInt>> = a.to_vec();
    let mut pivot_cols: Vec<usize> = Vec::new();
    let mut r = 0usize;
    for c in 0..columns {
        let Some(found) = (r..rows.len()).find(|&i| !rows[i][c].is_zero()) else { continue };
        rows.swap(r, found);
        for i in 0..rows.len() {
            if i != r && !rows[i][c].is_zero() {
                let (pivot, factor) = (rows[r][c].clone(), rows[i][c].clone());
                let reduced: Vec<BigInt> =
                    (0..columns).map(|j| &pivot * &rows[i][j] - &factor * &rows[r][j]).collect();
                rows[i] = reduced;
                normalize_bigint(&mut rows[i]);
            }
        }
        pivot_cols.push(c);
        r += 1;
        if r == rows.len() {
            break;
        }
    }
    let rank = pivot_cols.len();
    let lcm = pivot_cols
        .iter()
        .enumerate()
        .fold(BigInt::one(), |acc, (row, &col)| acc.lcm(&rows[row][col].abs()));
    let mut vectors = Vec::new();
    for free in (0..columns).filter(|c| !pivot_cols.contains(c)) {
        let mut v = vec![BigInt::zero(); columns];
        v[free] = lcm.clone();
        for (row, &col) in pivot_cols.iter().enumerate() {
            // pivot * v_col + rows[row][free] * v_free = 0
            v[col] = -(&rows[row][free] * &lcm) / &rows[row][col];
        }
        normalize_bigint(&mut v);
        vectors.push(v);
    }
    KernelBasis { vectors, rank, dimension: columns - rank }
}

/// Basis of all P-invariants: the left kernel of `C` (`y^T C = 0`).
pub fn p_invariant_basis(net: &PetriNet) -> KernelBasis {
    let matrix = net.incidence_matrix();
    let (places, transitions) = (matrix.place_count(), matrix.transition_count());
    // y^T C = 0  <=>  C^T y = 0: rows are transitions, columns are places.
    let ct: Vec<Vec<BigInt>> = (0..transitions)
        .map(|t| (0..places).map(|p| matrix.get(p, t).unwrap_or_default()).collect())
        .collect();
    integer_kernel_basis(&ct, places)
}

/// Basis of all T-invariants: the right kernel of `C` (`C x = 0`).
pub fn t_invariant_basis(net: &PetriNet) -> KernelBasis {
    let matrix = net.incidence_matrix();
    let (places, transitions) = (matrix.place_count(), matrix.transition_count());
    let c: Vec<Vec<BigInt>> = (0..places)
        .map(|p| (0..transitions).map(|t| matrix.get(p, t).unwrap_or_default()).collect())
        .collect();
    integer_kernel_basis(&c, transitions)
}
