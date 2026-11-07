#![allow(unused)]
use num_bigint::BigInt;
use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{PetriNet, PetriNetBuilder, PlaceId};
use racon_ctl::{CTLFormula, ComparisonOp};
use racon_invariants::types::{InvariantAnalysisResult, PInvariant};
use racon_property::*;
use racon_structural::topology::StructuralProperties;

// =============================================================================
// Mock State Space: 3 nodes
//   n0(init,[1,0]) → n1([0,1]) → n2([0,0])
// =============================================================================

struct MockStateSpace {
    successors: Vec<Vec<NodeId>>,
    predecessors: Vec<Vec<NodeId>>,
    markings: Vec<Marking>,
    node_count: usize,
    edge_count: usize,
    initial_node: NodeId,
}

impl MockStateSpace {
    fn new() -> Self {
        let succ: Vec<Vec<usize>> = vec![vec![1], vec![2], vec![]];
        let n = succ.len();
        let mut pred: Vec<Vec<NodeId>> = vec![vec![]; n];
        let mut edge_count = 0;
        for (u, s) in succ.iter().enumerate() {
            for &v in s {
                pred[v].push(NodeId(u as u32));
                edge_count += 1;
            }
        }
        let succ_native: Vec<Vec<NodeId>> = succ
            .iter()
            .map(|s| s.iter().map(|&i| NodeId(i as u32)).collect())
            .collect();
        let markings = vec![
            Marking::new(vec![TokenValue::Finite(1), TokenValue::Finite(0)]),
            Marking::new(vec![TokenValue::Finite(0), TokenValue::Finite(1)]),
            Marking::new(vec![TokenValue::Finite(0), TokenValue::Finite(0)]),
        ];
        Self {
            successors: succ_native,
            predecessors: pred,
            markings,
            node_count: n,
            edge_count,
            initial_node: NodeId(0),
        }
    }
}

impl StateSpace for MockStateSpace {
    fn successors(&self, node: NodeId) -> &[NodeId] {
        &self.successors[node.0 as usize]
    }
    fn predecessors(&self, node: NodeId) -> &[NodeId] {
        &self.predecessors[node.0 as usize]
    }
    fn marking(&self, node: NodeId) -> &Marking {
        &self.markings[node.0 as usize]
    }
    fn node_count(&self) -> usize {
        self.node_count
    }
    fn edge_count(&self) -> usize {
        self.edge_count
    }
    fn initial_node(&self) -> NodeId {
        self.initial_node
    }
    fn is_finite(&self) -> bool {
        true
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_net() -> PetriNet {
    use racon_core::net::ArcEndpoint;
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".to_string(), None);
    let _p1 = builder.add_place("P1".to_string(), None);
    let t0 = builder.add_transition("T0".to_string(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    // m0 = [1, 0] for invariant tests
    builder
        .build(vec![TokenValue::Finite(1), TokenValue::Finite(0)])
        .expect("valid net")
}

fn make_invariant(coefficients: Vec<i64>, constant: i64) -> PInvariant {
    PInvariant {
        coefficients: coefficients.into_iter().map(BigInt::from).collect(),
        is_semiflow: true,
        is_minimal: true,
        support: vec![],
        constant_value: Some(BigInt::from(constant)),
    }
}

fn make_context() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

fn make_default_engine() -> PropertyEngineImpl {
    PropertyEngineImpl::default()
}

// ── Marking Test ─────────────────────────────────────────────────────────────

#[test]
fn test_marking_test_p_invariant_pass() {
    let net = make_net();
    let engine = make_default_engine();
    let ctx = make_context();
    // invariant [1, 1] with m0=[1,0] → sum=1
    // target=[0,1] → sum=1 → passes invariant check
    let invariants = vec![make_invariant(vec![1, 1], 1)];
    let target = Marking::new(vec![TokenValue::Finite(0), TokenValue::Finite(1)]);
    let result = engine
        .marking_test(&net, &target, &invariants, &ctx)
        .unwrap();
    match result {
        MarkingTestResult::PossiblyReachable { .. } => {} // expected
        other => panic!("Expected PossiblyReachable, got {:?}", other),
    }
}

#[test]
fn test_marking_test_p_invariant_fail() {
    let net = make_net();
    let engine = make_default_engine();
    let ctx = make_context();
    // invariant [1, 1] with m0=[1,0] → sum=1
    // target=[0,2] → sum=2 ≠ 1 → unreachable
    let invariants = vec![make_invariant(vec![1, 1], 1)];
    let target = Marking::new(vec![TokenValue::Finite(0), TokenValue::Finite(2)]);
    let result = engine
        .marking_test(&net, &target, &invariants, &ctx)
        .unwrap();
    match result {
        MarkingTestResult::Unreachable {
            reason: UnreachabilityReason::PInvariantViolation { .. },
            ..
        } => {} // expected
        other => panic!("Expected Unreachable(PInvariantViolation), got {:?}", other),
    }
}

#[test]
fn test_marking_test_no_invariants() {
    let net = make_net();
    let engine = make_default_engine();
    let ctx = make_context();
    let invariants = vec![];
    let target = Marking::new(vec![TokenValue::Finite(5), TokenValue::Finite(5)]);
    let result = engine
        .marking_test(&net, &target, &invariants, &ctx)
        .unwrap();
    match result {
        MarkingTestResult::PossiblyReachable { .. } => {} // expected (no invariants to check)
        other => panic!("Expected PossiblyReachable, got {:?}", other),
    }
}

#[test]
fn test_marking_test_multiple_invariants() {
    let net = make_net();
    let engine = make_default_engine();
    let ctx = make_context();
    // invariant [1, 0] for m0=[1,0] → sum=1
    // target=[0,0] → sum=0 ≠ 1 → unreachable
    let invariants = vec![
        make_invariant(vec![1, 0], 1), // place_0 always 1
        make_invariant(vec![0, 1], 0), // place_1 always 0
    ];
    let target = Marking::new(vec![TokenValue::Finite(1), TokenValue::Finite(0)]);
    let result = engine
        .marking_test(&net, &target, &invariants, &ctx)
        .unwrap();
    match result {
        MarkingTestResult::PossiblyReachable { .. } => {} // both invariants match
        other => panic!("Expected PossiblyReachable, got {:?}", other),
    }
}

#[test]
fn test_marking_test_returns_nonnegative_parikh_solution_when_found() {
    let net = make_net();
    let engine = make_default_engine();
    let ctx = make_context();
    let target = net.initial_marking().clone();
    let result = engine.marking_test(&net, &target, &[], &ctx).unwrap();
    match result {
        MarkingTestResult::PossiblyReachable {
            parikh_vector,
            state_equation_searched_within_bound,
        } => {
            assert_eq!(parikh_vector, Some(vec![0]));
            assert!(state_equation_searched_within_bound);
        }
        other => panic!("Expected PossiblyReachable, got {:?}", other),
    }
}

// ── Bad State Evaluation ─────────────────────────────────────────────────────

#[test]
fn test_bad_state_safe() {
    let ss = MockStateSpace::new();
    let engine = make_default_engine();
    let ctx = make_context();
    let net = make_net();
    // bad state: place_0 > 5, which no state satisfies
    let defs = vec![BadStateDefinition {
        id: "bad1".to_string(),
        description: "Place 0 > 5".to_string(),
        predicate: StatePredicate::LinearConstraint {
            terms: vec![(PlaceId(0), BigInt::from(1))],
            op: ComparisonOp::Gt,
            constant: BigInt::from(5),
        },
    }];
    let report = engine.evaluate_bad_states(&net, &defs, &ss, &ctx).unwrap();
    assert!(report.is_safe);
    assert_eq!(report.violations.len(), 1);
    assert!(!report.violations[0].reached);
}

#[test]
fn bad_state_is_proven_unreachable_by_p_invariant() {
    let net = make_net();
    let def = BadStateDefinition {
        id: "unsafe-token-count".into(),
        description: "place 0 has more than one token".into(),
        predicate: StatePredicate::LinearConstraint {
            terms: vec![(PlaceId(0), BigInt::from(1))],
            op: ComparisonOp::Gt,
            constant: BigInt::from(1),
        },
    };
    let invariant = PInvariant {
        coefficients: vec![BigInt::from(1), BigInt::from(1)],
        is_semiflow: true,
        is_minimal: true,
        support: vec![PlaceId(0), PlaceId(1)],
        constant_value: Some(BigInt::from(1)),
    };
    let result = PropertyEngineImpl::default()
        .analyze_bad_states_algebraic(&net, &[def], &[invariant], &make_context())
        .unwrap();
    assert!(result[0].proven_unreachable);
}

#[test]
fn test_bad_state_unsafe() {
    let ss = MockStateSpace::new();
    let engine = make_default_engine();
    let ctx = make_context();
    let net = make_net();
    // bad state: place_0 > 0 → n0 has place_0=1 → EF(bad) is satisfied
    let defs = vec![BadStateDefinition {
        id: "bad1".to_string(),
        description: "Place 0 > 0".to_string(),
        predicate: StatePredicate::LinearConstraint {
            terms: vec![(PlaceId(0), BigInt::from(1))],
            op: ComparisonOp::Gt,
            constant: BigInt::from(0),
        },
    }];
    let report = engine.evaluate_bad_states(&net, &defs, &ss, &ctx).unwrap();
    assert!(!report.is_safe);
    assert_eq!(report.violations.len(), 1);
    assert!(report.violations[0].reached);
}

#[test]
fn test_bad_state_multiple_defs() {
    let ss = MockStateSpace::new();
    let engine = make_default_engine();
    let ctx = make_context();
    let net = make_net();
    let defs = vec![
        BadStateDefinition {
            id: "safe".to_string(),
            description: "Place 1 > 10".to_string(),
            predicate: StatePredicate::LinearConstraint {
                terms: vec![(PlaceId(1), BigInt::from(1))],
                op: ComparisonOp::Gt,
                constant: BigInt::from(10),
            },
        },
        BadStateDefinition {
            id: "unsafe".to_string(),
            description: "Place 0 > 0".to_string(),
            predicate: StatePredicate::LinearConstraint {
                terms: vec![(PlaceId(0), BigInt::from(1))],
                op: ComparisonOp::Gt,
                constant: BigInt::from(0),
            },
        },
    ];
    let report = engine.evaluate_bad_states(&net, &defs, &ss, &ctx).unwrap();
    assert!(!report.is_safe);
    assert!(!report.violations[0].reached); // safe
    assert!(report.violations[1].reached); // unsafe
}

// ── Expert System ────────────────────────────────────────────────────────────

#[test]
fn test_infer_properties_cpi() {
    let engine = make_default_engine();
    let invariants = InvariantAnalysisResult {
        p_semiflows: vec![],
        t_semiflows: vec![],
        minimal_p_semiflows: vec![],
        minimal_t_semiflows: vec![],
        is_conservative: false,
        is_cpi: true,
        is_cti: false,
        structural_bounds: std::collections::HashMap::new(),
        structural_bounded_by_inequality: None,
    };
    let structural = StructuralProperties::default();
    let report = engine.infer_properties(&invariants, &structural, None);
    let bounded_conclusions: Vec<_> = report
        .conclusions
        .iter()
        .filter(|c| c.property == "Boundedness")
        .collect();
    assert!(
        !bounded_conclusions.is_empty(),
        "Expected boundedness conclusions"
    );
    assert_eq!(bounded_conclusions[0].value, "True (Structural)");
}

#[test]
fn test_infer_properties_conservative() {
    let engine = make_default_engine();
    let invariants = InvariantAnalysisResult {
        p_semiflows: vec![],
        t_semiflows: vec![],
        minimal_p_semiflows: vec![],
        minimal_t_semiflows: vec![],
        is_conservative: true,
        is_cpi: false,
        is_cti: false,
        structural_bounds: std::collections::HashMap::new(),
        structural_bounded_by_inequality: None,
    };
    let structural = StructuralProperties::default();
    let report = engine.infer_properties(&invariants, &structural, None);
    let bounded: Vec<_> = report
        .conclusions
        .iter()
        .filter(|c| c.property == "Boundedness")
        .collect();
    assert!(!bounded.is_empty());
    assert_eq!(bounded[0].value, "True (Conservative)");
}

#[test]
fn test_infer_properties_free_choice() {
    let engine = make_default_engine();
    let invariants = InvariantAnalysisResult {
        p_semiflows: vec![],
        t_semiflows: vec![],
        minimal_p_semiflows: vec![],
        minimal_t_semiflows: vec![],
        is_conservative: false,
        is_cpi: true,
        is_cti: false,
        structural_bounds: std::collections::HashMap::new(),
        structural_bounded_by_inequality: None,
    };
    let mut structural = StructuralProperties::default();
    structural.is_free_choice = true;
    let report = engine.infer_properties(&invariants, &structural, None);
    let liveness: Vec<_> = report
        .conclusions
        .iter()
        .filter(|c| c.property == "Liveness")
        .collect();
    assert!(!liveness.is_empty(), "Expected liveness conclusions");
    assert_eq!(
        liveness[0].confidence,
        racon_property::Confidence::Heuristic
    );
}

#[test]
fn test_infer_properties_empty() {
    let engine = make_default_engine();
    let invariants = InvariantAnalysisResult {
        p_semiflows: vec![],
        t_semiflows: vec![],
        minimal_p_semiflows: vec![],
        minimal_t_semiflows: vec![],
        is_conservative: false,
        is_cpi: false,
        is_cti: false,
        structural_bounds: std::collections::HashMap::new(),
        structural_bounded_by_inequality: None,
    };
    let structural = StructuralProperties::default();
    let report = engine.infer_properties(&invariants, &structural, None);
    // With no CPI and no conservativeness and no state space, should be few conclusions.
    // The free-choice rule won't fire because !is_structurally_bounded
    // The CPI and conservative rules won't fire.
    // No state space → no SCC analysis.
    assert!(report.conclusions.is_empty());
}

// ── Predicate to CTL ─────────────────────────────────────────────────────────

#[test]
fn test_predicate_to_ctl_conversion() {
    let engine = make_default_engine();
    // Test that internal predicate_to_ctl works correctly
    let pred = StatePredicate::And(
        Box::new(StatePredicate::LinearConstraint {
            terms: vec![(PlaceId(0), BigInt::from(1))],
            op: ComparisonOp::Gt,
            constant: BigInt::from(0),
        }),
        Box::new(StatePredicate::LinearConstraint {
            terms: vec![(PlaceId(1), BigInt::from(1))],
            op: ComparisonOp::Eq,
            constant: BigInt::from(1),
        }),
    );
    // We can't call predicate_to_ctl directly (it's private),
    // but we can verify via bad state evaluation
    let ss = MockStateSpace::new();
    let ctx = make_context();
    let net = make_net();
    let def = BadStateDefinition {
        id: "test".to_string(),
        description: "complex predicate".to_string(),
        predicate: pred,
    };
    let report = engine.evaluate_bad_states(&net, &[def], &ss, &ctx).unwrap();
    // n0 has p0=1 but p1=0 → AND fails
    // n1 has p0=0 but p1=1 → AND fails
    // n2 has p0=0, p1=0 → AND fails
    // No state satisfies both p0>0 and p1=1
    assert!(report.is_safe);
}
