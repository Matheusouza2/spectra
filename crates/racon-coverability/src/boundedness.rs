//! Boundedness decision procedure (Starke, *Analyse von Petri-Netz-Modellen*,
//! Satz 4.3 and procedure `Beschraenkt`).
//!
//! A net is unbounded iff some reachable `m'` strictly covers a marking `m*`
//! lying on the path from `m0` to `m'` (`m* <= m'`, `m* != m'`). The procedure
//! builds the ordinary reachability graph and **halts at the first such
//! marking**, so unbounded nets are rejected after a handful of states instead
//! of building a full Karp-Miller graph, and bounded nets never need omega
//! markings.

use std::collections::HashMap;

use racon_core::execution::AnalysisContext;
use racon_core::marking::Marking;
use racon_core::net::PetriNet;

use crate::error::CoverabilityError;

/// Outcome of [`decide_boundedness`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundednessVerdict {
    /// The (finite) reachability set was fully explored; it has this many markings.
    Bounded { markings: usize },
    /// A reachable marking strictly covers an ancestor on its path (Satz 4.2).
    Unbounded { explored: usize },
    /// `max_markings` was reached before a decision. Not a proof either way.
    Inconclusive { explored: usize },
}

impl BoundednessVerdict {
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bounded { .. } => Some(true),
            Self::Unbounded { .. } => Some(false),
            Self::Inconclusive { .. } => None,
        }
    }
}

fn covers(larger: &Marking, smaller: &Marking) -> bool {
    larger
        .tokens
        .iter()
        .zip(smaller.tokens.iter())
        .all(|(a, b)| a.geq(b))
}

pub fn decide_boundedness(
    net: &PetriNet,
    max_markings: usize,
    context: &AnalysisContext,
) -> Result<BoundednessVerdict, CoverabilityError> {
    if net.is_extended() {
        // Satz 4.2 relies on monotonicity: with inhibitors/priorities a marking
        // covering an ancestor need not be able to repeat the ancestor's path.
        return Err(CoverabilityError::Unsupported(
            crate::error::MONOTONICITY_REQUIRED.to_owned(),
        ));
    }
    let initial = net.initial_marking().clone();
    let mut markings: Vec<Marking> = vec![initial.clone()];
    let mut vor: Vec<Option<usize>> = vec![None];
    let mut seen: HashMap<Marking, usize> = HashMap::new();
    seen.insert(initial, 0);

    let mut next = 0;
    while next < markings.len() {
        context
            .check_cancellation()
            .map_err(|_| CoverabilityError::Cancelled)?;
        let current = next;
        next += 1;
        for transition in net.transitions() {
            if !net.is_enabled(transition.id, &markings[current]) {
                continue;
            }
            let successor = net.fire(transition.id, &markings[current]);
            if seen.contains_key(&successor) {
                continue;
            }
            // Walk the Vor chain: m* := m; while m* != NIL and not m* <= m'.
            let mut ancestor = Some(current);
            while let Some(index) = ancestor {
                if covers(&successor, &markings[index]) {
                    // `successor` is new, hence != m*: strict cover (Satz 4.2).
                    return Ok(BoundednessVerdict::Unbounded {
                        explored: markings.len(),
                    });
                }
                ancestor = vor[index];
            }
            if markings.len() >= max_markings {
                return Ok(BoundednessVerdict::Inconclusive {
                    explored: markings.len(),
                });
            }
            seen.insert(successor.clone(), markings.len());
            markings.push(successor);
            vor.push(Some(current));
        }
    }
    Ok(BoundednessVerdict::Bounded {
        markings: markings.len(),
    })
}

#[cfg(test)]
mod tests {
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    use super::*;

    fn ctx() -> AnalysisContext {
        AnalysisContext::new(AnalysisConfig::default())
    }

    #[test]
    fn counter_net_is_unbounded_after_one_step() {
        let mut b = PetriNetBuilder::default();
        let p = b.add_place("p".into(), None);
        let t = b.add_transition("t".into(), None);
        b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
        b.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 2);
        let net = b.build(vec![TokenValue::Finite(1)]).unwrap();
        assert_eq!(
            decide_boundedness(&net, 1000, &ctx()).unwrap(),
            BoundednessVerdict::Unbounded { explored: 1 }
        );
    }

    #[test]
    fn cycle_net_is_bounded() {
        let mut b = PetriNetBuilder::default();
        let p0 = b.add_place("p0".into(), None);
        let p1 = b.add_place("p1".into(), None);
        let t0 = b.add_transition("t0".into(), None);
        let t1 = b.add_transition("t1".into(), None);
        b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
        let net = b
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap();
        assert_eq!(
            decide_boundedness(&net, 1000, &ctx()).unwrap(),
            BoundednessVerdict::Bounded { markings: 2 }
        );
    }
}
