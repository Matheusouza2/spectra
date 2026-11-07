//! Structured mathematical evidence produced from RACoN analysis artifacts.

mod types;

pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::marking::TokenValue;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder, TransitionId};
    use racon_invariants::types::{InvariantAnalysisResult, TInvariant};
    use racon_reachability::{ReachabilityEngine, ReachabilityEngineImpl};
    use racon_structural::topology::StructuralAnalyzer;
    use std::collections::HashMap;

    fn ordinary_pure_cycle() -> racon_core::net::PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        let t1 = builder.add_transition("t1".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
        builder.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
        builder
            .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
            .unwrap()
    }

    #[test]
    fn structural_certificate_contains_network_specific_evidence() {
        let net = ordinary_pure_cycle();
        let properties = StructuralAnalyzer::analyze(&net);
        let evidence = build_structural_evidence(&net, &properties);
        let ordinary = evidence
            .iter()
            .find(|e| e.property == PropertyId::Ord)
            .unwrap();

        assert_eq!(ordinary.status, EvidenceStatus::Proven);
        assert!(ordinary.certificate.is_some());
        assert!(ordinary
            .certificate
            .as_ref()
            .unwrap()
            .summary()
            .contains("arcos"));
    }

    #[test]
    fn universal_behavioral_evidence_is_inconclusive_without_exact_graph() {
        let evidence = bounded_evidence(None, None);
        assert_eq!(evidence.status, EvidenceStatus::Inconclusive);
        assert!(evidence.certificate.is_none());
    }

    #[test]
    fn exact_reachability_produces_bounded_and_reversibility_certificates() {
        let net = ordinary_pure_cycle();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = ReachabilityEngineImpl::default()
            .build_complete(&net, &context)
            .unwrap();
        let evidence = build_behavioral_evidence(Some(&graph), None);

        let bounded = evidence
            .iter()
            .find(|e| e.property == PropertyId::B)
            .unwrap();
        assert_eq!(bounded.status, EvidenceStatus::Proven);
        assert!(matches!(bounded.certificate, Some(Certificate::Bounded(_))));
        assert_eq!(bounded.proof.kind, ProofKind::Proof);
        assert!(bounded.proof.conclusion.contains("TRUE"));
        assert_eq!(bounded.proof.tables.len(), 1);

        let reversible = evidence
            .iter()
            .find(|e| e.property == PropertyId::Rev)
            .unwrap();
        assert_eq!(reversible.status, EvidenceStatus::Proven);
        assert!(reversible.metadata.exact_state_space);
        assert_eq!(reversible.proof.kind, ProofKind::Proof);
        assert!(reversible
            .proof
            .facts
            .iter()
            .any(|fact| fact.label == "Estados sem caminho de retorno" && fact.value == "0"));
    }

    #[test]
    fn t_invariant_certificate_preserves_realizability_witness() {
        let net = ordinary_pure_cycle();
        let result = InvariantAnalysisResult {
            p_semiflows: Vec::new(),
            t_semiflows: vec![TInvariant {
                coefficients: vec![1.into(), 1.into()],
                is_semiflow: true,
                is_minimal: true,
                support: vec![TransitionId(0), TransitionId(1)],
                is_realizable: Some(true),
                realization_witness: Some(vec![TransitionId(0), TransitionId(1)]),
            }],
            minimal_p_semiflows: Vec::new(),
            minimal_t_semiflows: vec![vec![1.into(), 1.into()]],
            is_conservative: false,
            is_cpi: false,
            is_cti: true,
            structural_bounds: HashMap::new(),
            structural_bounded_by_inequality: None,
        };

        let evidence = build_invariant_evidence(&net, &result);
        let t = evidence
            .iter()
            .find(|item| item.property == PropertyId::TInvariants)
            .unwrap();
        let certificate = match t.certificate.as_ref().unwrap() {
            Certificate::Invariant(certificate) => certificate,
            _ => panic!("expected invariant certificate"),
        };

        assert_eq!(certificate.realizability, Some(true));
        assert_eq!(
            certificate.realization_witness.as_deref(),
            Some(&[0, 1][..])
        );
    }

    #[test]
    fn mathematical_evidence_serializes_structured_proof_for_frontend() {
        let net = ordinary_pure_cycle();
        let context = AnalysisContext::new(AnalysisConfig::default());
        let graph = ReachabilityEngineImpl::default()
            .build_complete(&net, &context)
            .unwrap();
        let bounded = build_behavioral_evidence(Some(&graph), None)
            .into_iter()
            .find(|item| item.property == PropertyId::B)
            .unwrap();
        let json = serde_json::to_value(bounded).unwrap();

        assert!(json.get("proof").is_some());
        assert!(json["proof"].get("derivation").is_some());
        assert!(json["proof"].get("conclusion").is_some());
    }
}
