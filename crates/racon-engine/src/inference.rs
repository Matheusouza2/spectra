use std::collections::HashSet;

use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::TokenValue;
use racon_core::net::{PetriNet, TransitionId};
use racon_invariants::{
    structural_unboundedness_certificate, verify_structural_boundedness_certificate,
    InvariantAnalysisResult,
};
use racon_structural::{ExpertSystem, LivenessLevel, StructuralProperties};

use crate::properties::{PropertyDto, PropertySource, PropertyStatus};
use crate::state_space::StateSpaceHandle;

/// A single inference rule: an identifier, its premises, and the property it
/// concludes (spec §29).
#[derive(Debug, Clone, Copy)]
pub struct InferenceRule {
    pub id: &'static str,
    pub premises: &'static [&'static str],
    pub conclusion: &'static str,
}

pub const RULES: &[InferenceRule] = &[
    InferenceRule {
        id: "CPI_IMPLIES_BOUNDED",
        premises: &["CPI"],
        conclusion: "B",
    },
    InferenceRule {
        id: "CONSERVATIVE_IMPLIES_BOUNDED",
        premises: &["CSV"],
        conclusion: "B",
    },
    InferenceRule {
        id: "MURATA_INEQUALITY_IMPLIES_BOUNDED",
        premises: &["MURATA_INEQUALITY"],
        conclusion: "B",
    },
    InferenceRule {
        id: "STRUCTURAL_BOUND_IMPLIES_SB",
        premises: &["CPI", "CSV", "MURATA_INEQUALITY"],
        conclusion: "SB",
    },
    InferenceRule {
        id: "STATE_SPACE_BOUNDEDNESS",
        premises: &["STATE_SPACE"],
        conclusion: "B",
    },
    InferenceRule {
        id: "STATE_SPACE_REACHABILITY",
        premises: &["STATE_SPACE"],
        conclusion: "REV,DSt,DTr,DCF,L,LV",
    },
    InferenceRule {
        id: "LIVE_AND_SAFE",
        premises: &["L", "SAFE"],
        conclusion: "LS",
    },
];

/// Centralizes the theorem-driven implications between properties so that
/// controllers and serializers never encode reasoning themselves (spec §29).
pub struct PropertyInferenceEngine;

impl PropertyInferenceEngine {
    pub fn rules() -> &'static [InferenceRule] {
        RULES
    }

    pub fn infer(
        net: &PetriNet,
        structural: &StructuralProperties,
        invariants: Option<&InvariantAnalysisResult>,
        state_space: Option<&StateSpaceHandle>,
    ) -> Vec<PropertyDto> {
        let mut properties = algebraic(net, invariants);
        properties.push(bounded(net, invariants, state_space));
        properties.extend(behavioral(net, structural, state_space));
        properties
    }
}

fn algebraic(net: &PetriNet, invariants: Option<&InvariantAnalysisResult>) -> Vec<PropertyDto> {
    let Some(invariants) = invariants else {
        return vec![
            PropertyDto::unavailable(
                "CPI",
                "Covered by P-Invariants",
                PropertyStatus::NotComputed,
                PropertySource::Algebraic,
            ),
            PropertyDto::unavailable(
                "CTI",
                "Covered by T-Invariants",
                PropertyStatus::NotComputed,
                PropertySource::Algebraic,
            ),
            PropertyDto::unavailable(
                "SB",
                "Structurally Bounded",
                PropertyStatus::NotComputed,
                PropertySource::Algebraic,
            ),
        ];
    };

    let murata = murata_certificate(net, invariants);
    let structurally_bounded = invariants.is_cpi || invariants.is_conservative || murata;
    let rule = if invariants.is_cpi {
        "STRUCTURAL_BOUND_IMPLIES_SB"
    } else if invariants.is_conservative {
        "STRUCTURAL_BOUND_IMPLIES_SB"
    } else if murata {
        "STRUCTURAL_BOUND_IMPLIES_SB"
    } else {
        "STRUCTURAL_BOUND_NOT_PROVEN"
    };

    vec![
        PropertyDto::boolean(
            "CPI",
            "Covered by P-Invariants",
            invariants.is_cpi,
            PropertySource::Algebraic,
        )
        .with_rule("P_INVARIANT_COVERAGE"),
        PropertyDto::boolean(
            "CTI",
            "Covered by T-Invariants",
            invariants.is_cti,
            PropertySource::Algebraic,
        )
        .with_rule("T_INVARIANT_COVERAGE"),
        if structurally_bounded {
            PropertyDto::boolean(
                "SB",
                "Structurally Bounded",
                true,
                PropertySource::Algebraic,
            )
            .with_rule(rule)
        } else if structural_unboundedness_certificate(net).is_some() {
            // Farkas dual of Murata's criterion: an exactly verified firing
            // count vector x >= 0 with C x >= 0, C x != 0 exists, so no
            // y > 0 with C^T y <= 0 does. Decided on the incidence matrix
            // (the ordinary-arc projection for extended nets).
            PropertyDto::boolean(
                "SB",
                "Structurally Bounded",
                false,
                PropertySource::Algebraic,
            )
            .with_rule("STRUCTURAL_UNBOUNDED_CERTIFICATE")
        } else {
            // No certificate either way (solver inconclusive): stay UNKNOWN.
            PropertyDto::unavailable(
                "SB",
                "Structurally Bounded",
                PropertyStatus::Unknown,
                PropertySource::Algebraic,
            )
            .with_rule(rule)
        },
    ]
}

fn bounded(
    net: &PetriNet,
    invariants: Option<&InvariantAnalysisResult>,
    state_space: Option<&StateSpaceHandle>,
) -> PropertyDto {
    if let Some(invariants) = invariants {
        if invariants.is_cpi {
            return PropertyDto::boolean("B", "Bounded", true, PropertySource::Algebraic)
                .with_rule("CPI_IMPLIES_BOUNDED");
        }
        if invariants.is_conservative {
            return PropertyDto::boolean("B", "Bounded", true, PropertySource::Algebraic)
                .with_rule("CONSERVATIVE_IMPLIES_BOUNDED");
        }
        if murata_certificate(net, invariants) {
            return PropertyDto::boolean("B", "Bounded", true, PropertySource::Algebraic)
                .with_rule("MURATA_INEQUALITY_IMPLIES_BOUNDED");
        }
    }

    if let Some(handle) = state_space {
        let source = if handle.reduced() {
            PropertySource::ReducedStateSpace
        } else {
            PropertySource::StateSpace
        };
        return match handle.bounded_status() {
            Some(value) => {
                PropertyDto::boolean("B", "Bounded", value, source)
                    .with_rule("STATE_SPACE_BOUNDEDNESS")
            }
            None => {
                PropertyDto::unavailable("B", "Bounded", PropertyStatus::Unknown, source)
                    .with_rule("INEXACT_STATE_SPACE")
            }
        };
    }

    PropertyDto::unavailable(
        "B",
        "Bounded",
        PropertyStatus::NotComputed,
        PropertySource::StateSpace,
    )
}

fn behavioral(
    net: &PetriNet,
    _structural: &StructuralProperties,
    state_space: Option<&StateSpaceHandle>,
) -> Vec<PropertyDto> {
    let Some(handle) = state_space else {
        return not_computed_behavioral();
    };
    if handle.reduced() {
        return reduced_behavioral();
    }
    if !handle.is_exact() {
        return unknown_behavioral();
    }

    let ss = handle.as_state_space();
    let dead: HashSet<TransitionId> = ExpertSystem::dead_transitions(net, ss).into_iter().collect();
    let is_live = ExpertSystem::is_live(net, ss);
    let live_ignoring_dead = net
        .transitions()
        .iter()
        .filter(|transition| !dead.contains(&transition.id))
        .all(|transition| ExpertSystem::is_transition_live(net, transition.id, ss));
    let safe = is_safe(ss);
    let liveness = weakest_liveness_level(net, ss);

    let mut properties = vec![
        PropertyDto::boolean(
            "REV",
            "Reversible",
            is_reversible(ss),
            PropertySource::StateSpace,
        )
        .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::optional(
            "DSt",
            "Dead State Reachable",
            ExpertSystem::has_reachable_dead_state(ss),
            PropertySource::StateSpace,
        )
        .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::unavailable(
            "BSt",
            "Bad State Reachable",
            PropertyStatus::NotComputed,
            PropertySource::StateSpace,
        ),
        PropertyDto::optional(
            "DTr",
            "Dead Transition",
            ExpertSystem::has_dead_transition(net, ss),
            PropertySource::StateSpace,
        )
        .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::boolean(
            "DCF",
            "Dynamically Conflict Free",
            is_dynamically_conflict_free(net, ss),
            PropertySource::StateSpace,
        )
        .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::boolean("L", "Live", is_live, PropertySource::StateSpace)
            .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::boolean(
            "LV",
            "Live ignoring Dead Transitions",
            live_ignoring_dead,
            PropertySource::StateSpace,
        )
        .with_rule("STATE_SPACE_REACHABILITY"),
        PropertyDto::boolean(
            "L&S",
            "Live and Safe",
            is_live && safe,
            PropertySource::StateSpace,
        )
        .with_rule("LIVE_AND_SAFE"),
    ];
    properties.push(liveness_property(liveness));
    properties.extend(colored_liveness());
    properties
}

fn not_computed_behavioral() -> Vec<PropertyDto> {
    let mut properties: Vec<PropertyDto> = [
        ("REV", "Reversible"),
        ("DSt", "Dead State Reachable"),
        ("BSt", "Bad State Reachable"),
        ("DTr", "Dead Transition"),
        ("DCF", "Dynamically Conflict Free"),
        ("L", "Live"),
        ("LV", "Live ignoring Dead Transitions"),
        ("L&S", "Live and Safe"),
        ("LL", "Liveness Classification"),
    ]
    .into_iter()
    .map(|(code, name)| {
        PropertyDto::unavailable(code, name, PropertyStatus::NotComputed, PropertySource::StateSpace)
    })
    .collect();
    properties.extend(colored_liveness());
    properties
}

/// Reduced graphs only certify the properties their capabilities declare as
/// preserved. Everything else is reported UNKNOWN with an explicit source so a
/// reduction can never turn an unproven property into FALSE (spec §36).
fn reduced_behavioral() -> Vec<PropertyDto> {
    let mut properties: Vec<PropertyDto> = [
        ("REV", "Reversible"),
        ("DSt", "Dead State Reachable"),
        ("BSt", "Bad State Reachable"),
        ("DTr", "Dead Transition"),
        ("DCF", "Dynamically Conflict Free"),
        ("L", "Live"),
        ("LV", "Live ignoring Dead Transitions"),
        ("L&S", "Live and Safe"),
        ("LL", "Liveness Classification"),
    ]
    .into_iter()
    .map(|(code, name)| {
        PropertyDto::unavailable(
            code,
            name,
            PropertyStatus::Unknown,
            PropertySource::ReducedStateSpace,
        )
        .with_rule("STATE_SPACE_PROPERTY_NOT_PRESERVED_BY_REDUCTION")
    })
    .collect();
    properties.extend(colored_liveness());
    properties
}

fn unknown_behavioral() -> Vec<PropertyDto> {
    let mut properties: Vec<PropertyDto> = [
        ("REV", "Reversible"),
        ("DSt", "Dead State Reachable"),
        ("BSt", "Bad State Reachable"),
        ("DTr", "Dead Transition"),
        ("DCF", "Dynamically Conflict Free"),
        ("L", "Live"),
        ("LV", "Live ignoring Dead Transitions"),
        ("L&S", "Live and Safe"),
        ("LL", "Liveness Classification"),
    ]
    .into_iter()
    .map(|(code, name)| {
        PropertyDto::unavailable(code, name, PropertyStatus::Unknown, PropertySource::StateSpace)
    })
    .collect();
    properties.extend(colored_liveness());
    properties
}

fn colored_liveness() -> Vec<PropertyDto> {
    // WL and CL are defined for colored nets; P/T nets are not applicable.
    vec![
        PropertyDto::unavailable(
            "WL",
            "Weakly Live (colored only)",
            PropertyStatus::NotApplicable,
            PropertySource::StateSpace,
        ),
        PropertyDto::unavailable(
            "CL",
            "Collectively Live (colored only)",
            PropertyStatus::NotApplicable,
            PropertySource::StateSpace,
        ),
    ]
}

fn liveness_property(level: LivenessLevel) -> PropertyDto {
    match level {
        LivenessLevel::Unknown => PropertyDto::unavailable(
            "LL",
            "Liveness Classification",
            PropertyStatus::Unknown,
            PropertySource::StateSpace,
        ),
        other => PropertyDto::unavailable(
            "LL",
            "Liveness Classification",
            PropertyStatus::True,
            PropertySource::StateSpace,
        )
        .with_witness(liveness_label(other)),
    }
}

const fn liveness_label(level: LivenessLevel) -> &'static str {
    match level {
        LivenessLevel::L0 => "L0",
        LivenessLevel::L1 => "L1",
        LivenessLevel::L2 => "L2",
        LivenessLevel::L3 => "L3",
        LivenessLevel::L4 => "L4",
        LivenessLevel::Unknown => "UNKNOWN",
    }
}

fn weakest_liveness_level(net: &PetriNet, ss: &dyn StateSpace) -> LivenessLevel {
    net.transitions()
        .iter()
        .map(|transition| ExpertSystem::liveness_level(net, transition.id, ss))
        .min_by_key(level_rank)
        .unwrap_or(LivenessLevel::Unknown)
}

const fn level_rank(level: &LivenessLevel) -> i32 {
    match level {
        LivenessLevel::L0 => 0,
        LivenessLevel::L1 => 1,
        LivenessLevel::L2 => 2,
        LivenessLevel::L3 => 3,
        LivenessLevel::L4 => 4,
        LivenessLevel::Unknown => -1,
    }
}

fn is_reversible(ss: &dyn StateSpace) -> bool {
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::from([ss.initial_node()]);
    visited.insert(ss.initial_node());
    while let Some(node) = queue.pop_front() {
        for &predecessor in ss.predecessors(node) {
            if visited.insert(predecessor) {
                queue.push_back(predecessor);
            }
        }
    }
    visited.len() == ss.node_count()
}

fn is_safe(ss: &dyn StateSpace) -> bool {
    (0..ss.node_count()).all(|index| {
        ss.marking(NodeId(index as u32))
            .tokens
            .iter()
            .all(|token| matches!(token, TokenValue::Finite(value) if *value <= 1))
    })
}

fn is_dynamically_conflict_free(net: &PetriNet, ss: &dyn StateSpace) -> bool {
    if net.is_extended() {
        // Inhibitors and immediate precedence create conflicts without shared
        // input places: test whether firing one fireable transition disables
        // another (interleaving semantics).
        return (0..ss.node_count()).all(|index| {
            let marking = ss.marking(NodeId(index as u32));
            let fireable = net.fireable_transitions(marking);
            fireable.iter().enumerate().all(|(position, &first)| {
                fireable
                    .iter()
                    .skip(position + 1)
                    .all(|&second| !net.is_dynamic_conflict(first, second, marking))
            })
        });
    }
    (0..ss.node_count()).all(|index| {
        let marking = ss.marking(NodeId(index as u32));
        let enabled: Vec<TransitionId> = net
            .transitions()
            .iter()
            .map(|transition| transition.id)
            .filter(|&transition| net.is_enabled(transition, marking))
            .collect();
        enabled.iter().enumerate().all(|(position, &first)| {
            enabled.iter().skip(position + 1).all(|&second| {
                !net.preset(first)
                    .iter()
                    .any(|(place, _)| net.preset(second).iter().any(|(other, _)| place == other))
            })
        })
    })
}

fn murata_certificate(net: &PetriNet, invariants: &InvariantAnalysisResult) -> bool {
    invariants
        .structural_bounded_by_inequality
        .as_ref()
        .is_some_and(|certificate| {
            verify_structural_boundedness_certificate(net.incidence_matrix(), certificate)
        })
}
