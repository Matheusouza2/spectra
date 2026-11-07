use racon_core::net::PetriNet;
use racon_invariants::InvariantAnalysisResult;
use racon_structural::{ExpertSystem, StructuralProperties};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PropertyStatus {
    True,
    False,
    Unknown,
    NotComputed,
    NotApplicable,
    NotImplemented,
}

impl PropertyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::True => "TRUE",
            Self::False => "FALSE",
            Self::Unknown => "UNKNOWN",
            Self::NotComputed => "NOT_COMPUTED",
            Self::NotApplicable => "NOT_APPLICABLE",
            Self::NotImplemented => "NOT_IMPLEMENTED",
        }
    }
}

/// How a property value was obtained.
///
/// The variants mirror the protocol specification (§28): structural topology,
/// algebraic (invariant/incidence) reasoning, explicit state-space exploration,
/// expert inference rules, and reasoning over a reduced state space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PropertySource {
    Structural,
    Algebraic,
    StateSpace,
    ExpertRule,
    ReducedStateSpace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertyDto {
    pub code: String,
    pub name: String,
    pub status: PropertyStatus,
    pub value: Option<bool>,
    pub source: PropertySource,
    /// Identifier of the inference rule that produced the value, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub witness: Option<String>,
    /// `structural_projection` or `extended_behavior`; present only for nets
    /// with inhibitor arcs or immediate transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_scope: Option<String>,
}

impl PropertyDto {
    pub fn boolean(
        code: &str,
        name: &str,
        value: bool,
        source: PropertySource,
    ) -> Self {
        Self {
            code: code.to_owned(),
            name: name.to_owned(),
            status: if value {
                PropertyStatus::True
            } else {
                PropertyStatus::False
            },
            value: Some(value),
            source,
            rule: None,
            witness: None,
            analysis_scope: None,
        }
    }

    /// Build from an optional tri-state decision. `None` becomes `UNKNOWN`.
    pub fn optional(
        code: &str,
        name: &str,
        value: Option<bool>,
        source: PropertySource,
    ) -> Self {
        match value {
            Some(value) => Self::boolean(code, name, value, source),
            None => Self::unavailable(code, name, PropertyStatus::Unknown, source),
        }
    }

    pub fn unavailable(
        code: &str,
        name: &str,
        status: PropertyStatus,
        source: PropertySource,
    ) -> Self {
        Self {
            code: code.to_owned(),
            name: name.to_owned(),
            status,
            value: None,
            source,
            rule: None,
            witness: None,
            analysis_scope: None,
        }
    }

    pub fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    pub fn with_witness(mut self, witness: impl Into<String>) -> Self {
        self.witness = Some(witness.into());
        self
    }

    fn structural(code: &str, name: &str, value: bool) -> Self {
        Self::boolean(code, name, value, PropertySource::Structural)
    }
}

pub(crate) fn properties_from_structural(properties: &StructuralProperties) -> Vec<PropertyDto> {
    vec![
        PropertyDto::structural("ORD", "Ordinary", properties.is_ordinary),
        PropertyDto::structural("HOM", "Homogeneous", properties.is_homogeneous),
        PropertyDto::structural(
            "NBM",
            "Non-blocking Multiplicity",
            properties.is_non_blocking,
        ),
        PropertyDto::structural("PUR", "Pure", properties.is_pure),
        PropertyDto::unavailable(
            "CSV",
            "Conservative",
            PropertyStatus::NotComputed,
            PropertySource::Algebraic,
        ),
        PropertyDto::structural(
            "SCF",
            "Static Conflict Free",
            properties.is_static_conflict_free,
        ),
        PropertyDto::structural("CON", "Connected", properties.is_connected),
        PropertyDto::unavailable(
            "SC",
            "Strongly Connected",
            PropertyStatus::NotImplemented,
            PropertySource::Structural,
        ),
        PropertyDto::structural(
            "Ft0",
            "No Transitions without Pre-set",
            !properties.has_transitions_without_preset,
        ),
        PropertyDto::structural(
            "tF0",
            "No Transitions without Post-set",
            !properties.has_transitions_without_postset,
        ),
        PropertyDto::structural(
            "Fp0",
            "No Places without Pre-set",
            !properties.has_places_without_preset,
        ),
        PropertyDto::structural(
            "pF0",
            "No Places without Post-set",
            !properties.has_places_without_postset,
        ),
        PropertyDto::structural("MG", "Marked Graph", properties.is_marked_graph),
        PropertyDto::structural("SM", "State Machine", properties.is_state_machine),
        PropertyDto::structural("FC", "Free Choice", properties.is_free_choice),
        PropertyDto::structural(
            "EFC",
            "Extended Free Choice",
            properties.is_extended_free_choice,
        ),
        PropertyDto::structural("ES", "Extended Simple", properties.is_extended_simple),
    ]
}

/// Structural properties that require the net itself (DTP and the
/// chapter-16 state-machine coverability family).
pub(crate) fn structural_derived_properties(
    net: &PetriNet,
    properties: &StructuralProperties,
) -> Vec<PropertyDto> {
    vec![
        PropertyDto::optional(
            "DTP",
            "Deadlock-Trap Property",
            ExpertSystem::deadlock_trap_property(net),
            PropertySource::Structural,
        ),
        PropertyDto::optional(
            "SMC",
            "State Machine Coverable",
            properties.state_machine_coverable,
            PropertySource::Structural,
        ),
        PropertyDto::optional(
            "SMD",
            "State Machine Decomposable",
            properties.state_machine_decomposable,
            PropertySource::Structural,
        ),
        PropertyDto::optional(
            "SMA",
            "State Machine Allocatable",
            properties.state_machine_allocatable,
            PropertySource::Structural,
        ),
    ]
}

/// Fill in `CSV` (conservative) from cached invariant data, if available.
pub(crate) fn apply_conservativeness(
    properties: &mut [PropertyDto],
    invariants: Option<&InvariantAnalysisResult>,
) {
    let Some(invariants) = invariants else {
        return;
    };
    if let Some(conservative) = properties.iter_mut().find(|property| property.code == "CSV") {
        conservative.status = if invariants.is_conservative {
            PropertyStatus::True
        } else {
            PropertyStatus::False
        };
        conservative.value = Some(invariants.is_conservative);
        conservative.rule = Some("STRUCTURAL_BOUNDING_CERTIFICATE".to_owned());
    }
}
