//! Stubborn set computation (Valmari).
//!
//! At a marking, a stubborn set is a subset of transitions that is closed under
//! conflict and under the enablers of its disabled members. Exploring only the
//! enabled transitions of a stubborn set preserves **deadlock reachability**
//! while avoiding the exploration of independent interleavings. No ignoring
//! proviso is implemented, so liveness, dead transitions, reversibility and
//! CTL must not be read from a graph built this way.

use std::collections::HashSet;

use racon_core::marking::Marking;
use racon_core::net::{PetriNet, TransitionId};

/// Compute a stubborn set seeded by `seed`.
pub fn stubborn_set(net: &PetriNet, marking: &Marking, seed: TransitionId) -> Vec<TransitionId> {
    let transition_count = net.transition_count();
    let mut included = vec![false; transition_count];
    let mut stack = vec![seed];
    if (seed.0 as usize) < transition_count {
        included[seed.0 as usize] = true;
    }

    while let Some(transition) = stack.pop() {
        let preset: HashSet<u32> = net
            .preset(transition)
            .iter()
            .map(|&(place, _)| place.0)
            .collect();

        // Enabler closure: if the transition is disabled, every transition that
        // can add tokens to one of its input places must be included.
        if !net.is_enabled(transition, marking) {
            for candidate in net.transitions() {
                let index = candidate.id.0 as usize;
                if included[index] {
                    continue;
                }
                let enables = net
                    .postset(candidate.id)
                    .iter()
                    .any(|&(place, _)| preset.contains(&place.0));
                if enables {
                    included[index] = true;
                    stack.push(candidate.id);
                }
            }
        }

        // Conflict closure: transitions competing for an input place.
        for candidate in net.transitions() {
            let index = candidate.id.0 as usize;
            if included[index] {
                continue;
            }
            let conflicts = net
                .preset(candidate.id)
                .iter()
                .any(|&(place, _)| preset.contains(&place.0));
            if conflicts {
                included[index] = true;
                stack.push(candidate.id);
            }
        }
    }

    net.transitions()
        .iter()
        .map(|transition| transition.id)
        .filter(|transition| included[transition.0 as usize])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::marking::Marking;
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNet, PetriNetBuilder};

    fn independent_loops() -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        let t1 = builder.add_transition("t1".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
        builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p1), 1);
        builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(1)])
            .unwrap()
    }

    fn conflicting_transitions() -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        let t1 = builder.add_transition("t1".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t1), 1);
        builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p1), 1);
        builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap()
    }

    #[test]
    fn independent_transitions_are_not_forced_together() {
        let net = independent_loops();
        let marking = Marking::from_finite(&[1, 1]);
        let set = stubborn_set(&net, &marking, TransitionId(0));
        assert_eq!(set, vec![TransitionId(0)]);
    }

    #[test]
    fn conflicting_transitions_enter_the_stubborn_set() {
        let net = conflicting_transitions();
        let marking = Marking::from_finite(&[1, 0]);
        let set = stubborn_set(&net, &marking, TransitionId(0));
        assert!(set.contains(&TransitionId(0)));
        assert!(set.contains(&TransitionId(1)));
    }

    #[test]
    fn disabled_transition_pulls_in_its_enablers() {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        let t1 = builder.add_transition("t1".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
        let net = builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap();
        // t1 is disabled at the initial marking; its enabler is t0.
        let marking = Marking::from_finite(&[1, 0]);
        let set = stubborn_set(&net, &marking, TransitionId(1));
        assert!(set.contains(&TransitionId(0)));
        assert!(set.contains(&TransitionId(1)));
    }
}
