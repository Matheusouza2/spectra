use racon_invariants::InvariantAnalysisResult;
use racon_structural::StructuralProperties;
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PropertySource {
    Structural,
    Invariant,
    StateSpace,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertyDto {
    pub code: String,
    pub name: String,
    pub status: PropertyStatus,
    pub value: Option<bool>,
    pub source: PropertySource,
    pub witness: Option<String>,
}

impl PropertyDto {
    fn structural(code: &str, name: &str, value: bool) -> Self {
        Self {
            code: code.to_owned(),
            name: name.to_owned(),
            status: if value {
                PropertyStatus::True
            } else {
                PropertyStatus::False
            },
            value: Some(value),
            source: PropertySource::Structural,
            witness: None,
        }
    }

    fn unavailable(code: &str, name: &str, status: PropertyStatus, source: PropertySource) -> Self {
        Self {
            code: code.to_owned(),
            name: name.to_owned(),
            status,
            value: None,
            source,
            witness: None,
        }
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
            PropertySource::Invariant,
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

pub(crate) fn properties_with_invariants(
    properties: &[PropertyDto],
    invariants: Option<&InvariantAnalysisResult>,
) -> Vec<PropertyDto> {
    let mut properties = properties.to_vec();
    let Some(invariants) = invariants else {
        return properties;
    };

    if let Some(conservative) = properties
        .iter_mut()
        .find(|property| property.code == "CSV")
    {
        conservative.status = if invariants.is_conservative {
            PropertyStatus::True
        } else {
            PropertyStatus::False
        };
        conservative.value = Some(invariants.is_conservative);
    }

    properties
}
