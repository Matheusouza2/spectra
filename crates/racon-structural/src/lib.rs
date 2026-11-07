//! RACoN Structural — structural analysis engine.
//!
//! Analyzes topological properties (ordinary, pure, state machine, free-choice,
//! etc.) and combines with invariant data to derive behavioral properties.

pub mod expert;
pub mod topology;

pub use expert::{BehavioralProperties, ExpertSystem, LivenessLevel};
pub use topology::{StructuralAnalyzer, StructuralProperties};
