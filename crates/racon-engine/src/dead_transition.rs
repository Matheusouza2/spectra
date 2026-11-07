//! Dead-transition decision through stubborn sets (Schmidt 1999, §11, Thm 6).

use racon_core::execution::AnalysisContext;
use racon_core::net::{PetriNet, TransitionId};
use racon_reduction::{decide_dead_transition, DeadSearchConfig, DeadVerdict, ReductionError};
use serde::Serialize;

/// Node budget of the search; overruns are reported as UNKNOWN.
pub const DEAD_TRANSITION_MAX_NODES: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadTransitionDto {
    pub transition: String,
    pub transition_index: usize,
    pub dead: bool,
    /// `DEAD` or `NOT_DEAD`; inconclusive runs are errors, never a status.
    pub status: &'static str,
    pub rule: &'static str,
    pub reference_ids: Vec<&'static str>,
    pub experimental: bool,
    /// Shortest firing sequence (transition names) that enables the
    /// transition; empty when it is dead or enabled initially.
    pub witness: Vec<String>,
    pub visited: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadTransitionError {
    TransitionNotFound(String),
    Unsupported(String),
    /// Budget exhausted: UNKNOWN, not a negative proof.
    Unknown(String),
    Cancelled,
}

impl DeadTransitionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TransitionNotFound(_) => "TRANSITION_NOT_FOUND",
            Self::Unsupported(_) => "UNSUPPORTED",
            Self::Unknown(_) => "RESOURCE_LIMIT_EXCEEDED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

impl std::fmt::Display for DeadTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransitionNotFound(t) => write!(f, "transition '{t}' was not found"),
            Self::Unsupported(message) => write!(f, "{message}"),
            Self::Unknown(message) => write!(f, "UNKNOWN: {message}"),
            Self::Cancelled => write!(f, "analysis cancelled"),
        }
    }
}

impl std::error::Error for DeadTransitionError {}

/// Resolve a transition by exact name, falling back to a numeric index.
pub fn resolve_transition(net: &PetriNet, reference: &str) -> Option<TransitionId> {
    net.transitions()
        .iter()
        .find(|transition| transition.name == reference)
        .map(|transition| transition.id)
        .or_else(|| {
            reference
                .parse::<usize>()
                .ok()
                .filter(|&index| index < net.transition_count())
                .map(|index| TransitionId(index as u32))
        })
}

pub(crate) fn decide_transition_dead(
    net: &PetriNet,
    reference: &str,
    context: &AnalysisContext,
) -> Result<DeadTransitionDto, DeadTransitionError> {
    let transition = resolve_transition(net, reference)
        .ok_or_else(|| DeadTransitionError::TransitionNotFound(reference.to_owned()))?;
    let name = net.transitions()[transition.0 as usize].name.clone();
    match decide_dead_transition(
        net,
        transition,
        DeadSearchConfig {
            max_nodes: DEAD_TRANSITION_MAX_NODES,
            use_stubborn: true,
        },
        context,
    ) {
        Ok(report) => {
            let (dead, witness) = match report.verdict {
                DeadVerdict::Dead => (true, Vec::new()),
                DeadVerdict::Live { path } => (
                    false,
                    path.iter()
                        .map(|t| net.transitions()[t.0 as usize].name.clone())
                        .collect(),
                ),
            };
            Ok(DeadTransitionDto {
                transition: name,
                transition_index: transition.0 as usize,
                dead,
                status: if dead { "DEAD" } else { "NOT_DEAD" },
                rule: "STUBBORN_ATTRACTOR_T",
                reference_ids: vec!["R9"],
                experimental: true,
                witness,
                visited: report.visited,
            })
        }
        Err(ReductionError::Cancelled) => Err(DeadTransitionError::Cancelled),
        Err(error @ ReductionError::LimitExceeded { .. }) => {
            Err(DeadTransitionError::Unknown(error.to_string()))
        }
        Err(error) => Err(DeadTransitionError::Unsupported(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::AnalysisConfig;
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    #[test]
    fn reports_dead_and_live_transitions_with_a_shortest_witness() {
        let mut b = PetriNetBuilder::default();
        let p: Vec<_> = (0..3).map(|i| b.add_place(format!("p{i}"), None)).collect();
        let go = b.add_transition("go".into(), None);
        let then = b.add_transition("then".into(), None);
        let never = b.add_transition("never".into(), None);
        b.add_arc(ArcEndpoint::Place(p[0]), ArcEndpoint::Transition(go), 1);
        b.add_arc(ArcEndpoint::Transition(go), ArcEndpoint::Place(p[1]), 1);
        b.add_arc(ArcEndpoint::Place(p[1]), ArcEndpoint::Transition(then), 1);
        b.add_arc(ArcEndpoint::Place(p[2]), ArcEndpoint::Transition(never), 1);
        let net = b
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0), TokenValue::Finite(0)])
            .unwrap();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let then = decide_transition_dead(&net, "then", &context).unwrap();
        assert_eq!((then.dead, then.status), (false, "NOT_DEAD"));
        assert_eq!(then.witness, vec!["go".to_owned()]);
        let never = decide_transition_dead(&net, "2", &context).unwrap();
        assert_eq!((never.transition.as_str(), never.dead), ("never", true));
        assert_eq!(
            decide_transition_dead(&net, "zz", &context).unwrap_err().code(),
            "TRANSITION_NOT_FOUND"
        );
    }
}
