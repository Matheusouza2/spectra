#![allow(unused)]
use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::PlaceId;
use racon_ctl::*;

// =============================================================================
// Mock State Space: 4 nodes
//   n0(init,[1,0,0]) → n1([0,1,0]), n3([0,0,0])
//   n1([0,1,0]) → n2([0,0,1])
//   n2([0,0,1]) → n1 (cycle)
//   n3([0,0,0]) → n3 (self-loop)
// =============================================================================

struct MockStateSpace {
    successors: Vec<Vec<NodeId>>,
    predecessors: Vec<Vec<NodeId>>,
    markings: Vec<Marking>,
    initial_node: NodeId,
}

impl MockStateSpace {
    fn new() -> Self {
        // n0 → n1, n3
        // n1 → n2
        // n2 → n1
        // n3 → n3
        let succ: Vec<Vec<usize>> = vec![vec![1, 3], vec![2], vec![1], vec![3]];
        let n = succ.len();
        let mut pred: Vec<Vec<NodeId>> = vec![vec![]; n];
        for (u, s) in succ.iter().enumerate() {
            for &v in s {
                pred[v].push(NodeId(u as u32));
            }
        }
        let succ_native: Vec<Vec<NodeId>> = succ
            .iter()
            .map(|s| s.iter().map(|&i| NodeId(i as u32)).collect())
            .collect();
        let markings = vec![
            Marking::new(vec![
                TokenValue::Finite(1),
                TokenValue::Finite(0),
                TokenValue::Finite(0),
            ]),
            Marking::new(vec![
                TokenValue::Finite(0),
                TokenValue::Finite(1),
                TokenValue::Finite(0),
            ]),
            Marking::new(vec![
                TokenValue::Finite(0),
                TokenValue::Finite(0),
                TokenValue::Finite(1),
            ]),
            Marking::new(vec![
                TokenValue::Finite(0),
                TokenValue::Finite(0),
                TokenValue::Finite(0),
            ]),
        ];
        Self {
            successors: succ_native,
            predecessors: pred,
            markings,
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
        self.successors.len()
    }
    fn edge_count(&self) -> usize {
        self.successors.iter().map(|s| s.len()).sum()
    }
    fn initial_node(&self) -> NodeId {
        self.initial_node
    }
    fn is_finite(&self) -> bool {
        true
    }
}

fn make_context() -> AnalysisContext {
    AnalysisContext::new(AnalysisConfig::default())
}

fn make_engine() -> CTLEngineImpl {
    CTLEngineImpl {
        config: CTLConfig {
            max_iterations: 100,
            extract_witness: true,
        },
    }
}

fn p0_gt_0() -> CTLFormula {
    CTLFormula::AtomicProposition(AtomicPredicate {
        terms: vec![PredicateTerm {
            place_id: PlaceId(0),
            coefficient: 1,
        }],
        operator: ComparisonOp::Gt,
        constant: 0,
    })
}

fn p1_eq_1() -> CTLFormula {
    CTLFormula::AtomicProposition(AtomicPredicate {
        terms: vec![PredicateTerm {
            place_id: PlaceId(1),
            coefficient: 1,
        }],
        operator: ComparisonOp::Eq,
        constant: 1,
    })
}

fn p2_gt_0() -> CTLFormula {
    CTLFormula::AtomicProposition(AtomicPredicate {
        terms: vec![PredicateTerm {
            place_id: PlaceId(2),
            coefficient: 1,
        }],
        operator: ComparisonOp::Gt,
        constant: 0,
    })
}

#[test]
fn ctl_formula_display_uses_readable_ctl_syntax() {
    let formula = CTLFormula::EF(Box::new(p1_eq_1()));

    assert_eq!(formula.to_string(), "EF(p1 = 1)");
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_ctl_true() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    let result = engine.evaluate(&CTLFormula::True, &ss, &ctx).unwrap();
    assert!(result.satisfied);
    assert_eq!(result.satisfying_states.len(), ss.node_count());
}

#[test]
fn test_ctl_false() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    let result = engine.evaluate(&CTLFormula::False, &ss, &ctx).unwrap();
    assert!(!result.satisfied);
    assert_eq!(result.satisfying_states.len(), 0);
}

#[test]
fn test_ctl_atomic() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // p0 > 0 should only match n0
    let result = engine.evaluate(&p0_gt_0(), &ss, &ctx).unwrap();
    assert!(result.satisfied); // initial n0 has p0=1 > 0
    assert_eq!(result.satisfying_states.len(), 1);
}

#[test]
fn test_ctl_not() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // not (p0 > 0) → all states except n0
    let formula = CTLFormula::Not(Box::new(p0_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(!result.satisfied); // n0 violates NOT
    assert_eq!(result.satisfying_states.len(), 3); // n1, n2, n3
}

#[test]
fn test_ctl_and() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // p0 > 0 AND p1 = 1 → no state has both
    let formula = CTLFormula::And(Box::new(p0_gt_0()), Box::new(p1_eq_1()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert_eq!(result.satisfying_states.len(), 0);
}

#[test]
fn test_ctl_or() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // p0 > 0 OR p1 = 1 → n0 (p0) and n1 (p1)
    let formula = CTLFormula::Or(Box::new(p0_gt_0()), Box::new(p1_eq_1()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(result.satisfied);
    assert_eq!(result.satisfying_states.len(), 2);
}

#[test]
fn test_ctl_implies() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // Implies = (NOT p0>0) OR (p1=1)
    // n0: p0>0 is true, p1=1 is false → false. So initial state fails.
    let formula = CTLFormula::Implies(Box::new(p0_gt_0()), Box::new(p1_eq_1()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(!result.satisfied);
    // n1, n2, n3 satisfy (p0=0 → antecedent false, so implies is true)
    assert_eq!(result.satisfying_states.len(), 3);
}

#[test]
fn test_ctl_ex() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // EX p2 > 0: states with successor where p2>0 (only n2 has p2>0)
    // n1 → n2, so n1 satisfies EX(p2>0)
    let formula = CTLFormula::EX(Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    // n1 has successor n2 where p2>0
    // n3 has successor n3 where p2=0 → no
    assert_eq!(result.satisfying_states.len(), 1); // just n1
}

#[test]
fn test_ctl_ax() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    // AX (p0 = 0): all successors have p0=0
    // n0 → n1 (p0=0), n3 (p0=0) → all successors pass → satisfied in n0
    let p0_eq_0 = CTLFormula::AtomicProposition(AtomicPredicate {
        terms: vec![PredicateTerm {
            place_id: PlaceId(0),
            coefficient: 1,
        }],
        operator: ComparisonOp::Eq,
        constant: 0,
    });
    let formula = CTLFormula::AX(Box::new(p0_eq_0));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(result.satisfying_states.len() >= 1); // n0 should satisfy
                                                  // n0 is initial, result.satisfied depends on init satisfying it
    assert!(result.satisfied);
}

#[test]
fn test_ctl_ef() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // EF p2 > 0: states that can reach p2>0
    // n0 → n1 → n2, n0 → n3. n0, n1 can reach n2
    let formula = CTLFormula::EF(Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(result.satisfied);
    // n0 and n1 can reach n2
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(0)));
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(1)));
}

#[test]
fn test_ctl_eg() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // EG (p1 > 0 OR p2 > 0): infinite path staying in {n1, n2}
    // n1 has p1=1, n2 has p2=1, they form a cycle
    let phi = CTLFormula::Or(Box::new(p1_eq_1()), Box::new(p2_gt_0()));
    let formula = CTLFormula::EG(Box::new(phi));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    // n1 and n2 satisfy EG φ (they are in the cycle)
    // n0 does not satisfy EG because n0 → n3 and n3 has a self-loop but n3 doesn't satisfy φ
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(1)));
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(2)));
}

#[test]
fn test_ctl_ag() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // AG (p0 = 0 OR p1 = 0): all states reachable from init have either p0=0 or p1=0
    // n0 violates p0=0 because p0=1, and p1=0, so it satisfies the OR? No, n0 has p0=1 ≠0, p1=0
    // Wait: p0=0 OR p1=0 → n0: p0=1≠0, p1=0 → true (p1=0 holds)
    // n1: p0=0, p1=1 → true (p0=0 holds)
    // n2: p0=0, p1=0 → true
    // n3: p0=0, p1=0 → true
    // All states satisfy it → AG holds
    let p0_eq_0_or_p1_eq_0 = CTLFormula::Or(
        Box::new(CTLFormula::AtomicProposition(AtomicPredicate {
            terms: vec![PredicateTerm {
                place_id: PlaceId(0),
                coefficient: 1,
            }],
            operator: ComparisonOp::Eq,
            constant: 0,
        })),
        Box::new(CTLFormula::AtomicProposition(AtomicPredicate {
            terms: vec![PredicateTerm {
                place_id: PlaceId(1),
                coefficient: 1,
            }],
            operator: ComparisonOp::Eq,
            constant: 0,
        })),
    );
    let formula = CTLFormula::AG(Box::new(p0_eq_0_or_p1_eq_0));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(result.satisfied);
}

#[test]
fn test_ctl_af() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // AF p2 > 0: all paths eventually reach p2>0
    // n0 → n1 → n2 (reaches) or n0 → n3 (never reaches p2>0)
    // So AF should NOT hold because n3 doesn't reach p2>0
    let formula = CTLFormula::AF(Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(!result.satisfied);
}

#[test]
fn test_ctl_eu() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // E[p1=1 U p2>0]: exists path where p1=1 holds until p2>0
    // n1(n0s successor) → n2(p2>0)
    // n0 does NOT satisfy because at n0, p1=0 (not p1=1)
    let formula = CTLFormula::EU(Box::new(p1_eq_1()), Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    // n0 is not in the satisfying set (it does not have p1=1)
    assert!(!result.satisfied);
    // n1 and n2 should satisfy
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(1)));
    assert!(result.satisfying_states.iter().any(|&n| n == NodeId(2)));
}

#[test]
fn test_ctl_au() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    // A[true U p2>0]: all paths eventually reach p2>0
    // n0 → n1 → n2 works, but n0 → n3 never reaches → fails
    let formula = CTLFormula::AU(Box::new(CTLFormula::True), Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(!result.satisfied);
}

#[test]
fn test_witness_extraction_ef() {
    let ss = MockStateSpace::new();
    let engine = make_engine();
    let ctx = make_context();
    let formula = CTLFormula::EF(Box::new(p2_gt_0()));
    let result = engine.evaluate(&formula, &ss, &ctx).unwrap();
    assert!(result.witness.is_some());
    let witness = result.witness.unwrap();
    assert!(!witness.nodes.is_empty());
    // Path should start at n0 and end at the state satisfying p2>0 (n2)
    assert_eq!(witness.nodes.first().unwrap().0, 0);
    assert_eq!(witness.nodes.last().unwrap().0, 2);
    assert_eq!(witness.transitions.len(), 2);
}

#[test]
fn terminal_states_use_logical_self_loop_for_ctl() {
    struct TerminalStateSpace {
        marking: Marking,
    }

    impl StateSpace for TerminalStateSpace {
        fn successors(&self, _node: NodeId) -> &[NodeId] {
            &[]
        }
        fn predecessors(&self, _node: NodeId) -> &[NodeId] {
            &[]
        }
        fn marking(&self, _node: NodeId) -> &Marking {
            &self.marking
        }
        fn node_count(&self) -> usize {
            1
        }
        fn edge_count(&self) -> usize {
            0
        }
        fn initial_node(&self) -> NodeId {
            NodeId(0)
        }
        fn is_finite(&self) -> bool {
            true
        }
    }

    let state_space = TerminalStateSpace {
        marking: Marking::new(vec![TokenValue::Finite(0)]),
    };
    let engine = make_engine();
    let context = make_context();
    let predicate = CTLFormula::AtomicProposition(AtomicPredicate {
        terms: vec![PredicateTerm {
            place_id: PlaceId(0),
            coefficient: 1,
        }],
        operator: ComparisonOp::Gt,
        constant: 0,
    });

    assert!(
        !engine
            .evaluate(
                &CTLFormula::EX(Box::new(predicate.clone())),
                &state_space,
                &context
            )
            .unwrap()
            .satisfied
    );
    assert!(
        !engine
            .evaluate(&CTLFormula::AX(Box::new(predicate)), &state_space, &context)
            .unwrap()
            .satisfied
    );
}

#[test]
fn test_ctl_evaluation_time() {
    let ss = MockStateSpace::new();
    let engine = CTLEngineImpl::default();
    let ctx = make_context();
    let result = engine.evaluate(&CTLFormula::True, &ss, &ctx).unwrap();
    assert!(result.evaluation_time_ms < 10000); // sanity check
}
