//! RACoN Invariants — P/T-invariant computation engine.
//!
//! Computes minimal P-semiflows and T-semiflows via rational null space
//! minimal non-negative semiflow extraction using integer elimination.

pub mod engine;
pub mod types;

pub use engine::{
    certify_p_semiflow_completeness, certify_t_semiflow_completeness,
    certify_place_inequality_completeness,
    compute_minimal_place_inequality_invariants, derive_structural_properties,
    structural_boundedness_certificate, structural_unboundedness_certificate,
    verify_structural_unboundedness_certificate, verify_place_inequality_invariants,
    verify_place_inequality_completeness,
    verify_p_invariants, verify_p_semiflow_completeness,
    verify_structural_boundedness_certificate, verify_t_invariants, verify_t_semiflow_completeness,
    p_invariant_basis, state_equation_test, t_invariant_basis, KernelBasis, InvariantEngine, InvariantEngineImpl, InvariantError, StateEquationVerdict,
};
pub use types::*;
