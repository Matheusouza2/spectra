use serde::{Deserialize, Serialize};

mod properties;
mod session;

pub use properties::{PropertyDto, PropertySource, PropertyStatus};
pub use session::{
    AnalysisSession, LoadSessionResponse, NetSummaryDto, SessionRepository, SourceDto,
};

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
