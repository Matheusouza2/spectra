use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

/// The incidence matrix C = Post - Pre.
///
/// Dimensions: |P| (rows) × |T| (columns).
/// Row i corresponds to place p_i, column j to transition t_j.
/// C[i][j] = net token change in p_i when t_j fires once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidenceMatrix {
    data: Vec<Vec<BigInt>>,
    place_count: usize,
    transition_count: usize,
}

impl IncidenceMatrix {
    /// Create a new incidence matrix from pre- and post-incidence data.
    /// `pre[i][j]` = weight of arc from p_i to t_j (or 0).
    /// `post[i][j]` = weight of arc from t_j to p_i (or 0).
    pub fn new(
        place_count: usize,
        transition_count: usize,
        pre: &[Vec<u32>],
        post: &[Vec<u32>],
    ) -> Self {
        let data: Vec<Vec<BigInt>> = (0..place_count)
            .map(|i| {
                (0..transition_count)
                    .map(|j| BigInt::from(post[i][j]) - BigInt::from(pre[i][j]))
                    .collect()
            })
            .collect();

        Self {
            data,
            place_count,
            transition_count,
        }
    }

    /// Number of places (rows).
    pub fn place_count(&self) -> usize {
        self.place_count
    }

    /// Number of transitions (columns).
    pub fn transition_count(&self) -> usize {
        self.transition_count
    }

    /// Get element at (place_index, transition_index).
    pub fn get(&self, place: usize, transition: usize) -> Option<BigInt> {
        self.data.get(place)?.get(transition).cloned()
    }

    /// Row for a given place (net effect on each transition).
    pub fn row(&self, place: usize) -> Option<&[BigInt]> {
        self.data.get(place).map(|r| r.as_slice())
    }

    /// Column for a given transition (net effect on each place).
    pub fn column(&self, transition: usize) -> Vec<BigInt> {
        self.data
            .iter()
            .map(|row| row[transition].clone())
            .collect()
    }

    /// Compute y^T * C for a P-invariant candidate vector y.
    pub fn left_multiply(&self, y: &[BigInt]) -> Vec<BigInt> {
        (0..self.transition_count)
            .map(|j| {
                (0..self.place_count)
                    .map(|i| &y[i] * &self.data[i][j])
                    .sum()
            })
            .collect()
    }

    /// Compute C * x for a T-invariant candidate vector x (Parikh vector).
    pub fn right_multiply(&self, x: &[BigInt]) -> Vec<BigInt> {
        (0..self.place_count)
            .map(|i| {
                (0..self.transition_count)
                    .map(|j| &self.data[i][j] * &x[j])
                    .sum()
            })
            .collect()
    }

    /// Raw access to the underlying matrix data.
    pub fn data(&self) -> &[Vec<BigInt>] {
        &self.data
    }
}
