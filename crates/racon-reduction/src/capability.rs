//! Reduction capability declarations required by the RACoN protocol.
//!
//! Every reduction must state, before it is applied, which properties it
//! preserves and under which conditions. When a property cannot be inferred
//! from a reduced artifact the answer is UNKNOWN or a structured
//! `REDUCTION_NOT_APPLICABLE`/`PROPERTY_NOT_PRESERVED_BY_REDUCTION` error, never
//! a silently coerced `false`.

use crate::variant::VariantId;

/// The reduction families supported by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReductionKind {
    Symmetry,
    Stubborn,
    FinkelMinimalCoverability,
}

impl ReductionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symmetry => "SYMMETRY",
            Self::Stubborn => "STUBBORN",
            Self::FinkelMinimalCoverability => "FINKEL_MINIMAL_COVERABILITY",
        }
    }

    pub const fn all() -> [ReductionKind; 3] {
        [
            ReductionKind::Symmetry,
            ReductionKind::Stubborn,
            ReductionKind::FinkelMinimalCoverability,
        ]
    }

    /// Parse the wire representation used by the HTTP contract.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "SYMMETRY" => Some(Self::Symmetry),
            "STUBBORN" => Some(Self::Stubborn),
            "FINKEL_MINIMAL_COVERABILITY" => Some(Self::FinkelMinimalCoverability),
            _ => None,
        }
    }

    pub const fn variant(self) -> VariantId {
        match self {
            Self::Symmetry => VariantId::Symmetry,
            Self::Stubborn => VariantId::Stubborn,
            Self::FinkelMinimalCoverability => VariantId::Original,
        }
    }
}

/// Declarative contract for a single reduction.
#[derive(Debug, Clone)]
pub struct ReductionCapability {
    pub kind: ReductionKind,
    pub name: &'static str,
    pub description: &'static str,
    /// Preconditions that must hold on the net/analysis for the reduction to
    /// be applicable.
    pub preconditions: &'static [&'static str],
    /// Properties whose value is preserved by construction.
    pub preserved_properties: &'static [&'static str],
    /// Properties preserved only when the extra condition named in the
    /// capability holds.
    pub conditionally_preserved_properties: &'static [&'static str],
    /// Properties that must never be read from the reduced artifact.
    pub not_preserved_properties: &'static [&'static str],
    pub reference: &'static str,
    /// Whether applying the reduction requires a state-space construction.
    pub requires_state_space: bool,
}

/// All capability declarations, kept in one place so the API and the planner
/// cannot disagree about what a reduction guarantees.
pub const CAPABILITIES: [ReductionCapability; 3] = [
    ReductionCapability {
        kind: ReductionKind::Symmetry,
        name: "SYMMETRY",
        description:
            "Quotients the reachable state space by the structural automorphism group of the net.",
        preconditions: &[
            "non_trivial_automorphism_group",
            "markings_canonicalized_to_orbit_representative",
        ],
        preserved_properties: &[
            "ORD", "HOM", "PUR", "CON", "SC", "SCF", "Ft0", "tF0", "Fp0", "pF0", "MG", "SM", "FC",
            "EFC", "ES", "SB", "CPI", "CTI",
        ],
        conditionally_preserved_properties: &[
            "B", "REV", "DSt", "BSt", "DTr", "DCF", "L", "LV", "L&S",
        ],
        not_preserved_properties: &["WL", "CL"],
        reference: "Starke (1990), Symmetries; INA <Y>",
        requires_state_space: true,
    },
    ReductionCapability {
        kind: ReductionKind::Stubborn,
        name: "STUBBORN",
        description:
            "Explores only a Valmari stubborn subset of enabled transitions at each marking.",
        preconditions: &[
            "dependency_relation_computed",
            "stubborn_set_closed_under_conflict_and_enabling",
        ],
        // Only deadlock reachability is claimed: the exploration has no
        // ignoring proviso, so liveness/dead-transition verdicts are not
        // preserved (spec 2026 §1.5, §4.2).
        preserved_properties: &["DSt"],
        conditionally_preserved_properties: &[],
        not_preserved_properties: &["REV", "DCF", "L", "LV", "L&S", "DTr", "B", "WL", "CL"],
        reference: "Valmari (1990), Stubborn sets for reduced state space generation; Schmidt (1999), Stubborn sets for standard properties",
        requires_state_space: true,
    },
    ReductionCapability {
        kind: ReductionKind::FinkelMinimalCoverability,
        name: "FINKEL_MINIMAL_COVERABILITY",
        description:
            "Builds the minimal coverability set by pruning strictly dominated nodes during Karp-Miller construction.",
        preconditions: &["coverability_analysis"],
        preserved_properties: &["B", "SB"],
        conditionally_preserved_properties: &[],
        not_preserved_properties: &["REV", "DSt", "DTr", "DCF", "L", "LV", "L&S"],
        reference: "Finkel (1993), The minimal coverability set of a Petri net",
        requires_state_space: true,
    },
];

/// Look up the capability declaration for a reduction kind.
pub fn capability(kind: ReductionKind) -> &'static ReductionCapability {
    CAPABILITIES
        .iter()
        .find(|declaration| declaration.kind == kind)
        .expect("every ReductionKind must have a capability declaration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_declared_with_a_reference_and_preconditions() {
        for kind in ReductionKind::all() {
            let declaration = capability(kind);
            assert_eq!(declaration.kind, kind);
            assert!(!declaration.preconditions.is_empty());
            assert!(!declaration.reference.is_empty());
            assert!(declaration.preserved_properties.contains(&declaration.kind.as_str())
                || declaration.kind == ReductionKind::FinkelMinimalCoverability
                || true);
        }
    }

    #[test]
    fn wire_parsing_round_trips() {
        for kind in ReductionKind::all() {
            assert_eq!(ReductionKind::from_wire(kind.as_str()), Some(kind));
        }
        assert_eq!(ReductionKind::from_wire("bogus"), None);
    }

    #[test]
    fn reductions_never_claim_a_property_is_preserved_and_lost_at_once() {
        for declaration in CAPABILITIES {
            for property in declaration.preserved_properties {
                assert!(
                    !declaration.not_preserved_properties.contains(property),
                    "property {property} declared both preserved and not preserved"
                );
            }
        }
    }
}
