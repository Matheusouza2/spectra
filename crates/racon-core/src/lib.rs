//! RACoN Core — data structures and traits for Petri Net analysis.
//!
//! This crate is the foundation of the RACoN ecosystem. All other crates
//! depend on it. It contains no I/O, no network, no UI dependencies.

pub mod error;
pub mod execution;
pub mod graph;
pub mod marking;
pub mod matrix;
pub mod net;
pub mod validation;

// Re-export key types
pub use error::RaconError;
pub use execution::{
    AnalysisConfig, AnalysisContext, AnalysisEngine, CancellationToken, MemoryBudget, MemoryError,
    ProgressUpdate,
};
pub use graph::{EdgeId, NodeId, StateSpace};
pub use marking::{Marking, TokenValue};
pub use matrix::IncidenceMatrix;
pub use net::{
    Arc, ArcEndpoint, InhibitorArc, PetriNet, PetriNetBuilder, Place, PlaceId, Transition,
    TransitionId, TransitionKind, PRIORITY_POLICY, SEMANTICS_VERSION,
};
