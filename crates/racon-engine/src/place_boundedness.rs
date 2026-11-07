//! Boundedness of a single place through the stubborn-set Karp–Miller tree
//! (Schmidt 1999, §6, Thm 2; ω-accelerated variant of `racon-reduction`).

use racon_core::execution::AnalysisContext;
use racon_core::net::{PetriNet, PlaceId};
use racon_reduction::{
    decide_boundedness_stubborn_omega, BoundSearchConfig, BoundTarget, OmegaVerdict,
    ReductionError,
};
use serde::Serialize;

/// Node budget of the ω tree; overruns are reported as UNKNOWN.
pub const PLACE_BOUNDEDNESS_MAX_NODES: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaceBoundednessDto {
    pub place: String,
    pub place_index: usize,
    pub bounded: bool,
    /// `BOUNDED` or `UNBOUNDED`; inconclusive runs are errors, never a status.
    pub status: &'static str,
    /// How the verdict was obtained.
    pub rule: &'static str,
    pub reference_ids: Vec<&'static str>,
    pub experimental: bool,
    /// Nodes of the ω tree (0 when the verdict came from a known net verdict).
    pub visited: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceBoundednessError {
    PlaceNotFound(String),
    Unsupported(String),
    /// Budget exhausted: the answer is UNKNOWN, not a negative proof.
    Unknown(String),
    Cancelled,
}

impl PlaceBoundednessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlaceNotFound(_) => "PLACE_NOT_FOUND",
            Self::Unsupported(_) => "UNSUPPORTED",
            Self::Unknown(_) => "RESOURCE_LIMIT_EXCEEDED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

impl std::fmt::Display for PlaceBoundednessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceNotFound(place) => write!(f, "place '{place}' was not found"),
            Self::Unsupported(message) => write!(f, "{message}"),
            Self::Unknown(message) => write!(f, "UNKNOWN: {message}"),
            Self::Cancelled => write!(f, "analysis cancelled"),
        }
    }
}

impl std::error::Error for PlaceBoundednessError {}

/// Resolve a place by exact name, falling back to a numeric index.
pub fn resolve_place(net: &PetriNet, reference: &str) -> Option<PlaceId> {
    net.places()
        .iter()
        .find(|place| place.name == reference)
        .map(|place| place.id)
        .or_else(|| {
            reference
                .parse::<usize>()
                .ok()
                .filter(|&index| index < net.place_count())
                .map(|index| PlaceId(index as u32))
        })
}

pub(crate) fn decide_place_boundedness(
    net: &PetriNet,
    reference: &str,
    net_known_bounded: bool,
    context: &AnalysisContext,
) -> Result<PlaceBoundednessDto, PlaceBoundednessError> {
    let place = resolve_place(net, reference)
        .ok_or_else(|| PlaceBoundednessError::PlaceNotFound(reference.to_owned()))?;
    let name = net.places()[place.0 as usize].name.clone();
    let place_index = place.0 as usize;
    let dto = |bounded: bool, rule: &'static str, visited: usize| PlaceBoundednessDto {
        place: name.clone(),
        place_index,
        bounded,
        status: if bounded { "BOUNDED" } else { "UNBOUNDED" },
        rule,
        reference_ids: vec!["R9"],
        experimental: true,
        visited,
    };
    if net_known_bounded {
        // A bounded net bounds every place.
        return Ok(dto(true, "NET_BOUNDED", 0));
    }
    match decide_boundedness_stubborn_omega(
        net,
        BoundTarget::Place(place),
        BoundSearchConfig {
            max_nodes: PLACE_BOUNDEDNESS_MAX_NODES,
            use_stubborn: true,
        },
        context,
    ) {
        Ok(report) => Ok(dto(
            report.verdict == OmegaVerdict::Bounded,
            "STUBBORN_OMEGA_TREE",
            report.visited,
        )),
        Err(ReductionError::Cancelled) => Err(PlaceBoundednessError::Cancelled),
        Err(error @ ReductionError::LimitExceeded { .. }) => {
            Err(PlaceBoundednessError::Unknown(error.to_string()))
        }
        Err(error) => Err(PlaceBoundednessError::Unsupported(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::AnalysisConfig;
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    fn growing_net() -> PetriNet {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let q = b.add_place("q".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(q), 1);
        b.build(vec![TokenValue::Finite(1), TokenValue::Finite(0)]).unwrap()
    }

    #[test]
    fn decides_each_place_and_resolves_names_and_indices() {
        let net = growing_net();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let p = decide_place_boundedness(&net, "p", false, &context).unwrap();
        assert_eq!((p.bounded, p.status), (true, "BOUNDED"));
        let q = decide_place_boundedness(&net, "1", false, &context).unwrap();
        assert_eq!((q.place.as_str(), q.bounded), ("q", false));
        assert_eq!(
            decide_place_boundedness(&net, "zz", false, &context).unwrap_err().code(),
            "PLACE_NOT_FOUND"
        );
        let shortcut = decide_place_boundedness(&net, "q", true, &context).unwrap();
        assert_eq!((shortcut.rule, shortcut.visited), ("NET_BOUNDED", 0));
    }
}
