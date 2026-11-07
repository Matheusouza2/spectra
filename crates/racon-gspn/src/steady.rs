// Steady-state solver — stationary distribution π·Q = 0 via power method
//
// Algorithm: power method on the embedded DTMC.
//   π·Q = 0  ⇔  π = π·(I + Q/q_max) = π·P
// where q_max = max_i |Q_ii| is the uniformization rate.
// P = I + Q/q_max is row-stochastic and aperiodic (for ergodic CTMCs),
// so π^(k+1) = π^(k)·P converges to the stationary distribution.

use crate::graph::QMatrix;
use crate::GspnError;

/// Result of steady-state analysis via power method.
#[derive(Debug, Clone)]
pub struct SteadyStateResult {
    /// Steady-state probability for each tangible state.
    pub probabilities: Vec<f64>,
    /// Whether the iterative solver converged.
    pub converged: bool,
    /// Number of iterations performed.
    pub iterations: u32,
}

/// Default convergence tolerance (L∞ norm).
const DEFAULT_TOLERANCE: f64 = 1e-10;

/// Maximum power-method iterations before giving up.
const MAX_ITERATIONS: u32 = 10_000;

/// Solve π·Q = 0 using the power method on the embedded DTMC.
///
/// Converts π·Q = 0 to π = π·P where P = I + Q/q_max and q_max = max_i |Q_ii|.
/// Since P is row-stochastic and aperiodic (for ergodic CTMCs), the power
/// method π^(k+1) = π^(k)·P converges to the stationary distribution.
///
/// # Arguments
/// * `q` — CTMC generator matrix (row-wise adjacency list)
/// * `tolerance` — Optional convergence threshold (default: 1e-10)
///
/// # Errors
/// Returns `GspnError::SingularMatrix` if q_max = 0 (all-diagonal-zero matrix).
/// Returns `GspnError::NoConvergence` after `MAX_ITERATIONS` without convergence.
pub fn solve_steady_state(
    q: &QMatrix,
    tolerance: Option<f64>,
) -> Result<SteadyStateResult, GspnError> {
    let tol = tolerance.unwrap_or(DEFAULT_TOLERANCE);
    let n = q.n;

    // Empty CTMC — trivially solved
    if n == 0 {
        return Ok(SteadyStateResult {
            probabilities: vec![],
            converged: true,
            iterations: 0,
        });
    }

    // Uniformization rate: q_max = max_i |Q_ii|
    let q_max = q.diagonal.iter().map(|d| d.abs()).fold(0.0_f64, f64::max);
    if q_max == 0.0 {
        return Err(GspnError::SingularMatrix);
    }

    // Initial guess: uniform distribution
    let mut pi = vec![1.0 / n as f64; n];

    for iter in 1..=MAX_ITERATIONS {
        // Compute π·Q using row-wise access to QMatrix
        // (π·Q)[j] = Σ_i π[i] * Q[i][j]
        //   = Σ_{i≠j} π[i] * transitions[i→j] + π[j] * Q[j][j]
        let mut piq = vec![0.0; n];
        for i in 0..n {
            // Off-diagonal: Q[i][j] for j ≠ i
            for &(j, rate) in &q.transitions[i] {
                piq[j] += pi[i] * rate;
            }
            // Diagonal: Q[i][i] = diagonal[i] (already negative)
            piq[i] += pi[i] * q.diagonal[i];
        }

        // Power iteration: π_new = π + (1/q_max) · π·Q
        //   which is equivalent to π_new = π · (I + Q/q_max)
        let mut pi_new = vec![0.0; n];
        let mut max_diff = 0.0;
        for j in 0..n {
            pi_new[j] = pi[j] + piq[j] / q_max;
            let diff = (pi_new[j] - pi[j]).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }

        pi = pi_new;

        // Check convergence (L∞ norm of change)
        if max_diff < tol {
            // Renormalize to eliminate floating-point drift
            let sum: f64 = pi.iter().sum();
            for p in &mut pi {
                *p /= sum;
            }
            return Ok(SteadyStateResult {
                probabilities: pi,
                converged: true,
                iterations: iter,
            });
        }
    }

    Err(GspnError::NoConvergence {
        iterations: MAX_ITERATIONS,
        residual: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_qmatrix() {
        let q = QMatrix {
            n: 0,
            transitions: vec![],
            diagonal: vec![],
        };
        let result = solve_steady_state(&q, None).unwrap();
        assert!(result.converged);
        assert!(result.probabilities.is_empty());
        assert_eq!(result.iterations, 0);
    }

    #[test]
    fn test_singular_matrix() {
        let q = QMatrix {
            n: 2,
            transitions: vec![vec![], vec![]],
            diagonal: vec![0.0, 0.0],
        };
        let result = solve_steady_state(&q, None);
        assert!(matches!(result, Err(GspnError::SingularMatrix)));
    }
}
