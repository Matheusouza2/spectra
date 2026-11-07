use crate::error::RaconError;
use crate::marking::Marking;
use crate::net::PetriNet;

/// Validate the structural integrity of a PetriNet.
/// Called automatically by PetriNetBuilder::build().
/// This function is exposed for programmatic re-validation if needed.
pub fn validate_petri_net(net: &PetriNet) -> Result<(), RaconError> {
    let place_count = net.place_count();
    let trans_count = net.transition_count();

    // All places have unique IDs (guaranteed by builder, but verify)
    for i in 0..place_count {
        if net.places()[i].id.0 as usize != i {
            return Err(RaconError::StructuralViolation(format!(
                "Place at index {} has non-sequential ID {:?}",
                i,
                net.places()[i].id
            )));
        }
    }

    // All transitions have unique IDs
    for i in 0..trans_count {
        if net.transitions()[i].id.0 as usize != i {
            return Err(RaconError::StructuralViolation(format!(
                "Transition at index {} has non-sequential ID {:?}",
                i,
                net.transitions()[i].id
            )));
        }
    }

    let mut place_incident = vec![false; place_count];
    let mut transition_incident = vec![false; trans_count];
    for arc in net.arcs() {
        match (arc.source, arc.target) {
            (
                crate::net::ArcEndpoint::Place(place),
                crate::net::ArcEndpoint::Transition(transition),
            )
            | (
                crate::net::ArcEndpoint::Transition(transition),
                crate::net::ArcEndpoint::Place(place),
            ) => {
                place_incident[place.0 as usize] = true;
                transition_incident[transition.0 as usize] = true;
            }
            _ => {
                return Err(RaconError::StructuralViolation(
                    "Arc is not bipartite".into(),
                ));
            }
        }
    }
    // Inhibitor arcs are test arcs but still connect their endpoints.
    for inh in net.inhibitor_arcs() {
        if inh.threshold == 0 {
            return Err(RaconError::StructuralViolation(
                "Inhibitor arc threshold must be >= 1".into(),
            ));
        }
        let (p, t) = (inh.place.0 as usize, inh.transition.0 as usize);
        if p >= place_count || t >= trans_count {
            return Err(RaconError::StructuralViolation(
                "Inhibitor arc endpoint out of range".into(),
            ));
        }
        place_incident[p] = true;
        transition_incident[t] = true;
    }
    if let Some(index) = place_incident.iter().position(|incident| !incident) {
        return Err(RaconError::StructuralViolation(format!(
            "Place {} ('{}') has no incident arc; declare an arc or remove the place",
            index,
            net.places()[index].name
        )));
    }
    if let Some(index) = transition_incident.iter().position(|incident| !incident) {
        return Err(RaconError::StructuralViolation(format!(
            "Transition {} ('{}') has no incident arc (no inputs, outputs or inhibitors); \
             declare at least one arc or remove the transition",
            index,
            net.transitions()[index].name
        )));
    }

    // Incidence matrix dimensions match
    if net.incidence_matrix().place_count() != place_count {
        return Err(RaconError::StructuralViolation(
            "Incidence matrix row count mismatch".into(),
        ));
    }
    if net.incidence_matrix().transition_count() != trans_count {
        return Err(RaconError::StructuralViolation(
            "Incidence matrix column count mismatch".into(),
        ));
    }

    Ok(())
}

/// Validate a marking against a PetriNet.
pub fn validate_marking(marking: &Marking, net: &PetriNet) -> Result<(), RaconError> {
    if marking.len() != net.place_count() {
        return Err(RaconError::InvalidInput(format!(
            "Marking size {} != place count {}",
            marking.len(),
            net.place_count()
        )));
    }

    if marking.has_omega() {
        return Err(RaconError::InvalidInput(
            "Omega not allowed in this marking".into(),
        ));
    }

    Ok(())
}
