use serde::{Deserialize, Serialize};

mod advanced;
mod dead_transition;
mod inference;
mod invariants;
mod place_boundedness;
mod properties;
mod session;
mod state_space;

pub use advanced::{run_advanced, AdvancedError, AdvancedParams, ADVANCED_ALGORITHMS};
pub use dead_transition::{
    resolve_transition, DeadTransitionDto, DeadTransitionError, DEAD_TRANSITION_MAX_NODES,
};
pub use inference::{InferenceRule, PropertyInferenceEngine, RULES};
pub use invariants::{
    InvariantCache, InvariantDto, InvariantInteger, InvariantKind, InvariantQuery,
    InvariantQueryError, InvariantTermDto,
};
pub use racon_invariants::{InequalityRelation, PlaceInequalityInvariant};
pub use place_boundedness::{
    resolve_place, PlaceBoundednessDto, PlaceBoundednessError, PLACE_BOUNDEDNESS_MAX_NODES,
};
pub use properties::{PropertyDto, PropertySource, PropertyStatus};
pub use session::{
    AnalysisRequest, AnalysisSession, ExplorationChoice, AnalysisSnapshot, LoadSessionResponse, NetSummaryDto,
    SessionRepository, SessionTimingsDto, SourceDto,
};
pub use state_space::{StateSpaceCounts, StateSpaceHandle};

/// Version of the public RACoN protocol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolVersion {
    V1,
}

impl ProtocolVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "1.0",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProtocolVersion;

    #[test]
    fn protocol_version_v1_uses_one_zero() {
        assert_eq!(ProtocolVersion::V1.as_str(), "1.0");
    }
}
