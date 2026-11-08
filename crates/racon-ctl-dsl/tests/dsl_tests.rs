#![allow(unused)]
use racon_core::marking::TokenValue;
use racon_core::net::{PetriNet, PetriNetBuilder, PlaceId};
use racon_ctl::{AtomicPredicate, CTLFormula, ComparisonOp, PredicateTerm};
use racon_ctl_dsl::{parse_ctl_query, parse_dsl, DslExpr, DslParseError, Translator};

fn make_test_net() -> PetriNet {
    use racon_core::net::ArcEndpoint;
    let mut builder = PetriNetBuilder::default();
    let p0 = builder.add_place("P0".to_string(), None);
    let _p1 = builder.add_place("P1".to_string(), None);
    let _p2 = builder.add_place("P2".to_string(), None);
    let t0 = builder.add_transition("T0".to_string(), None);
    builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
    builder
        .build(vec![
            TokenValue::Finite(0),
            TokenValue::Finite(0),
            TokenValue::Finite(0),
        ])
        .expect("valid net")
}

fn assert_atomic_prop(
    formula: &CTLFormula,
    expected_place: u32,
    expected_op: ComparisonOp,
    expected_const: i64,
) {
    match formula {
        CTLFormula::AtomicProposition(pred) => {
            assert_eq!(pred.terms.len(), 1);
            assert_eq!(pred.terms[0].place_id.0, expected_place);
            assert_eq!(pred.operator, expected_op);
            assert_eq!(pred.constant, expected_const);
        }
        other => panic!("Expected AtomicProposition, got {:?}", other),
    }
}

// ── Parser Tests ─────────────────────────────────────────────────────────────

#[test]
fn test_parse_bare_place() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Gt, 0);
}

#[test]
fn parse_false_and_implies_as_ctl_ast() {
    let net = make_test_net();
    assert!(matches!(
        parse_ctl_query("false", &net).unwrap(),
        CTLFormula::False
    ));
    assert!(matches!(
        parse_ctl_query("P1 > 0 se P2 > 0", &net).unwrap(),
        CTLFormula::Implies(_, _)
    ));
}

#[test]
fn test_parse_place_with_eq() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 = 5", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Eq, 5);
}

#[test]
fn incomplete_comparison_returns_error_instead_of_panicking() {
    let net = make_test_net();
    let result =
        std::panic::catch_unwind(|| parse_ctl_query("existe caminho eventualmente P1 =", &net));

    assert!(result.is_ok(), "incomplete CTL input must not panic");
    assert!(result.unwrap().is_err());
}

#[test]
fn rejects_preposition_between_temporal_operator_and_condition() {
    let net = make_test_net();

    assert!(parse_ctl_query("existe caminho eventualmente em P1 = 0", &net).is_err());
}

#[test]
fn test_parse_place_with_gt() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 3", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Gt, 3);
}

#[test]
fn test_parse_place_with_lt() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 < 2", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Lt, 2);
}

#[test]
fn test_parse_place_with_ge() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 >= 1", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Ge, 1);
}

#[test]
fn test_parse_place_with_le() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 <= 4", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Le, 4);
}

#[test]
fn test_parse_place_with_neq() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 != 0", &net).unwrap();
    assert_atomic_prop(&formula, 1, ComparisonOp::Neq, 0);
}

#[test]
fn test_parse_interval() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 between 2 and 5", &net).unwrap();
    // Should be And(P1 >= 2, P1 <= 5)
    match &formula {
        CTLFormula::And(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Ge, 2);
            assert_atomic_prop(r, 1, ComparisonOp::Le, 5);
        }
        other => panic!("Expected And, got {:?}", other),
    }
}

#[test]
fn test_parse_not() {
    let net = make_test_net();
    // "not P1 > 0" should parse as Not(AtomicProposition)
    let formula = parse_ctl_query("not P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::Not(inner) => {
            assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0);
        }
        other => panic!("Expected Not, got {:?}", other),
    }
}

#[test]
fn test_parse_and() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 and P2 = 1", &net).unwrap();
    match &formula {
        CTLFormula::And(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Gt, 0);
            assert_atomic_prop(r, 2, ComparisonOp::Eq, 1);
        }
        other => panic!("Expected And, got {:?}", other),
    }
}

#[test]
fn test_parse_or() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 or P2 = 1", &net).unwrap();
    match &formula {
        CTLFormula::Or(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Gt, 0);
            assert_atomic_prop(r, 2, ComparisonOp::Eq, 1);
        }
        other => panic!("Expected Or, got {:?}", other),
    }
}

// ── Temporal Operator Tests ──────────────────────────────────────────────────

#[test]
fn test_parse_ef() {
    let net = make_test_net();
    let formula = parse_ctl_query("exists eventually P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::EF(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected EF, got {:?}", other),
    }
}

#[test]
fn test_parse_ef_of_parenthesised_conjunction_and_until_separator() {
    let net = make_test_net();
    // AND inside the parentheses is a real conjunction under EF...
    for query in [
        "exists path eventually (P1 = 1 AND P2 = 0)",
        "existe caminho eventualmente (P1 = 1 E P2 = 0)",
    ] {
        match &parse_ctl_query(query, &net).unwrap() {
            CTLFormula::EF(inner) => assert!(matches!(**inner, CTLFormula::And(_, _)), "{query}: {inner:?}"),
            other => panic!("{query}: expected EF(And), got {other:?}"),
        }
    }
    // ...while after `until` the AND only separates the two sides: E[φ U ψ].
    match &parse_ctl_query("exists path until (P1 = 1) AND (P2 = 0)", &net).unwrap() {
        CTLFormula::EU(_, _) => {}
        other => panic!("expected EU, got {other:?}"),
    }
}

#[test]
fn test_parse_ag_default() {
    let net = make_test_net();
    // No quantifier → defaults to ForAll
    let formula = parse_ctl_query("always P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::AG(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected AG, got {:?}", other),
    }
}

#[test]
fn test_parse_ax() {
    let net = make_test_net();
    let formula = parse_ctl_query("forall next P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::AX(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected AX, got {:?}", other),
    }
}

#[test]
fn test_parse_exists_next() {
    let net = make_test_net();
    let formula = parse_ctl_query("exists next P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::EX(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected EX, got {:?}", other),
    }
}

#[test]
fn test_parse_eg() {
    let net = make_test_net();
    let formula = parse_ctl_query("exists always P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::EG(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected EG, got {:?}", other),
    }
}

#[test]
fn test_parse_eu() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 until P2 = 1", &net).unwrap();
    match &formula {
        CTLFormula::EU(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Gt, 0);
            assert_atomic_prop(r, 2, ComparisonOp::Eq, 1);
        }
        other => panic!("Expected EU, got {:?}", other),
    }
}

#[test]
fn test_parse_quantified_until_prefix() {
    let net = make_test_net();
    let formula = parse_ctl_query("existe caminho até P1 = 1 e P2 = 0", &net).unwrap();

    assert!(matches!(formula, CTLFormula::EU(_, _)));
}

#[test]
fn frontend_ctl_examples_are_parseable() {
    let net = make_test_net();
    let examples = [
        "existe caminho eventualmente P1 = 1",
        "para todos os caminhos sempre P1 >= 0",
        "existe caminho (P1 = 1) até (P2 = 0)",
        "para todo caminho (P1 >= 1) até (P2 = 0)",
    ];

    for example in examples {
        parse_ctl_query(example, &net)
            .unwrap_or_else(|error| panic!("frontend example should parse: {example}: {error}"));
    }
}

#[test]
fn test_parse_au() {
    let net = make_test_net();
    let formula = parse_ctl_query("for-all P1 > 0 until P2 = 1", &net).unwrap();
    match &formula {
        CTLFormula::AU(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Gt, 0);
            assert_atomic_prop(r, 2, ComparisonOp::Eq, 1);
        }
        other => panic!("Expected AU, got {:?}", other),
    }
}

#[test]
fn test_parse_af() {
    let net = make_test_net();
    let formula = parse_ctl_query("forall eventually P1 > 0", &net).unwrap();
    match &formula {
        CTLFormula::AF(inner) => assert_atomic_prop(inner, 1, ComparisonOp::Gt, 0),
        other => panic!("Expected AF, got {:?}", other),
    }
}

// ── Portuguese Keywords ──────────────────────────────────────────────────────

#[test]
fn test_parse_portuguese_and() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 e P2 = 1", &net).unwrap();
    assert!(matches!(formula, CTLFormula::And(_, _)));
}

#[test]
fn test_parse_portuguese_or() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 ou P2 = 1", &net).unwrap();
    assert!(matches!(formula, CTLFormula::Or(_, _)));
}

#[test]
fn test_parse_portuguese_not() {
    let net = make_test_net();
    let formula = parse_ctl_query("nao P1 > 0", &net).unwrap();
    assert!(matches!(formula, CTLFormula::Not(_)));
}

#[test]
fn test_parse_portuguese_eventually() {
    let net = make_test_net();
    let formula = parse_ctl_query("existe eventualmente P1 > 0", &net).unwrap();
    assert!(matches!(formula, CTLFormula::EF(_)));
}

#[test]
fn test_parse_portuguese_always() {
    let net = make_test_net();
    let formula = parse_ctl_query("para todos sempre P1 > 0", &net).unwrap();
    assert!(matches!(formula, CTLFormula::AG(_)));
}

#[test]
fn test_parse_portuguese_until() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 > 0 ate P2 = 1", &net).unwrap();
    assert!(matches!(formula, CTLFormula::EU(_, _)));
}

#[test]
fn test_parse_portuguese_interval() {
    let net = make_test_net();
    let formula = parse_ctl_query("P1 entre 2 e 5", &net).unwrap();
    match &formula {
        CTLFormula::And(l, r) => {
            assert_atomic_prop(l, 1, ComparisonOp::Ge, 2);
            assert_atomic_prop(r, 1, ComparisonOp::Le, 5);
        }
        other => panic!("Expected And, got {:?}", other),
    }
}

// ── Before (derived operator) ────────────────────────────────────────────────

#[test]
fn test_parse_before() {
    let net = make_test_net();
    // "P1 before P2 > 5" → E[(not P2>5) U (P1 and not P2>5)]
    let formula = parse_ctl_query("P1 before P2 > 5", &net).unwrap();
    // Before → EU(not ψ, φ and not ψ)
    // where φ = P1(>0), ψ = P2>5
    match &formula {
        CTLFormula::EU(not_psi, phi_and_not_psi) => {
            // not_psi should be Not(P2 > 5)
            match not_psi.as_ref() {
                CTLFormula::Not(inner) => {
                    assert_atomic_prop(inner, 2, ComparisonOp::Gt, 5);
                }
                other => panic!("Expected Not, got {:?}", other),
            }
            // phi_and_not_psi should be And(P1 > 0, Not(P2 > 5))
            match phi_and_not_psi.as_ref() {
                CTLFormula::And(phi, not_psi2) => {
                    assert_atomic_prop(phi, 1, ComparisonOp::Gt, 0);
                    match not_psi2.as_ref() {
                        CTLFormula::Not(inner2) => {
                            assert_atomic_prop(inner2, 2, ComparisonOp::Gt, 5);
                        }
                        other => panic!("Expected Not, got {:?}", other),
                    }
                }
                other => panic!("Expected And, got {:?}", other),
            }
        }
        other => panic!("Expected EU, got {:?}", other),
    }
}

// ── Error Cases ──────────────────────────────────────────────────────────────

#[test]
fn test_unknown_place_error() {
    let net = make_test_net();
    let result = parse_ctl_query("P99 > 0", &net);
    assert!(result.is_err());
}

#[test]
fn test_syntax_error() {
    let net = make_test_net();
    let result = parse_ctl_query("P1 >>> 0", &net);
    assert!(result.is_err());
}

#[test]
fn test_empty_query_error() {
    let net = make_test_net();
    let result = parse_ctl_query("", &net);
    assert!(result.is_err());
}

// ── Pure Parser (no net needed) ──────────────────────────────────────────────

#[test]
fn test_pure_parse_structure() {
    let expr = parse_dsl("exists eventually P1 > 0").unwrap();
    match &expr {
        DslExpr::TemporalUnary {
            quantifier,
            temporal_op,
            expr: _,
        } => {
            use racon_ctl_dsl::{DslQuantifier, DslTemporalOp};
            assert_eq!(*quantifier, DslQuantifier::Exists);
            assert_eq!(*temporal_op, DslTemporalOp::Eventually);
        }
        other => panic!("Expected TemporalUnary, got {:?}", other),
    }
}

// ── English DSL (UI language en-US) ──────────────────────────────────────────

#[test]
fn test_english_dsl_forms() {
    let net = make_test_net();
    let cases: &[(&str, fn(&CTLFormula) -> bool)] = &[
        ("for all paths always P1 >= 0", |f| matches!(f, CTLFormula::AG(_))),
        ("always P1 >= 0", |f| matches!(f, CTLFormula::AG(_))),
        ("exists path eventually P1 = 1", |f| matches!(f, CTLFormula::EF(_))),
        ("exists always P1 > 0", |f| matches!(f, CTLFormula::EG(_))),
        ("exists next P1 > 0", |f| matches!(f, CTLFormula::EX(_))),
        ("for all next P1 > 0", |f| matches!(f, CTLFormula::AX(_))),
        ("eventually P1 > 0", |f| matches!(f, CTLFormula::AF(_))),
        ("P1 > 0 until P2 = 1", |f| matches!(f, CTLFormula::EU(_, _))),
        ("for all P1 > 0 until P2 = 1", |f| matches!(f, CTLFormula::AU(_, _))),
        ("exists path until (P1 = 1) and (P2 = 0)", |f| matches!(f, CTLFormula::EU(_, _))),
        ("not P1 > 0", |f| matches!(f, CTLFormula::Not(_))),
        ("P1 > 0 and P2 = 0", |f| matches!(f, CTLFormula::And(_, _))),
        ("P1 > 0 or P2 = 0", |f| matches!(f, CTLFormula::Or(_, _))),
        ("P1 > 0 implies P2 = 0", |f| matches!(f, CTLFormula::Implies(_, _))),
        ("P1 between 2 and 5", |f| matches!(f, CTLFormula::And(_, _))),
        ("true", |f| matches!(f, CTLFormula::True)),
    ];
    for (query, check) in cases {
        let formula = parse_ctl_query(query, &net)
            .unwrap_or_else(|e| panic!("`{query}` should parse: {e:?}"));
        assert!(check(&formula), "`{query}` parsed to {formula:?}");
    }
}

// ── enabled(...) predicate ───────────────────────────────────────────────────

fn net_with_weights_and_inhibitor() -> PetriNet {
    use racon_core::net::ArcEndpoint;
    let mut b = PetriNetBuilder::default();
    let p0 = b.add_place("P0".to_string(), None);
    let p1 = b.add_place("P1".to_string(), None);
    let t0 = b.add_transition("T0".to_string(), None);
    let t1 = b.add_transition("T1".to_string(), None);
    // t0 consumes 2 tokens of P0 and is inhibited while P1 holds 3 or more.
    b.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 2);
    b.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
    b.add_inhibitor_arc(p1, t0, 3);
    // t1 consumes 1 token of P1.
    b.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t1), 1);
    b.add_arc(ArcEndpoint::Transition(t1), ArcEndpoint::Place(p0), 1);
    b.build(vec![racon_core::TokenValue::Finite(2), racon_core::TokenValue::Finite(0)]).unwrap()
}

#[test]
fn enabled_expands_to_input_weights_and_inhibitor_thresholds() {
    let net = net_with_weights_and_inhibitor();
    // enabled(T0)  =  (P0 >= 2) AND (P1 < 3)
    let formula = parse_ctl_query("exists path eventually enabled(T0)", &net).unwrap();
    assert_eq!(formula.to_string(), "EF((p0 >= 2) AND (p1 < 3))");
    // Several transitions are alternatives; both languages and the comma/OR separators work.
    for query in [
        "exists path eventually enabled(T0, T1)",
        "exists path eventually enabled(T0 OR T1)",
        "existe caminho eventualmente habilitada(T0, T1)",
    ] {
        match &parse_ctl_query(query, &net).unwrap() {
            CTLFormula::EF(inner) => assert!(matches!(**inner, CTLFormula::Or(_, _)), "{query}: {inner:?}"),
            other => panic!("{query}: expected EF(Or), got {other:?}"),
        }
    }
    // It composes with the rest of the language.
    assert!(parse_ctl_query("for all paths always not (enabled(T1) AND P0 = 0)", &net).is_ok());
}

#[test]
fn enabled_rejects_unknown_transitions_and_empty_lists() {
    let net = net_with_weights_and_inhibitor();
    match parse_ctl_query("exists path eventually enabled(T9)", &net) {
        Err(racon_ctl_dsl::DslParseError::UnknownTransition(name)) => assert_eq!(name, "T9"),
        other => panic!("expected UnknownTransition, got {other:?}"),
    }
    assert!(parse_ctl_query("exists path eventually enabled()", &net).is_err());
}

/// `exists path (φ) until (ψ)` is the documented form of E[φ U ψ]; `for all paths (φ) until (ψ)` is A[φ U ψ].
/// The "and" of the older prefix form (`exists path until (φ) and (ψ)`) is only a separator and is no longer taught.
#[test]
fn test_parse_quantified_infix_until_in_both_languages() {
    let net = make_test_net();
    for query in [
        "exists path (P1 = 1) until (P2 = 0)",
        "exists (P1 = 1) until (P2 = 0)",
        "existe caminho (P1 = 1) até (P2 = 0)",
        "existe (P1 = 1) até (P2 = 0)",
        "exists path P1 = 1 until P2 = 0",
    ] {
        match &parse_ctl_query(query, &net).unwrap_or_else(|e| panic!("{query}: {e}")) {
            CTLFormula::EU(_, _) => {}
            other => panic!("{query}: expected EU, got {other:?}"),
        }
    }
    for query in [
        "for all paths (P1 = 1) until (P2 = 0)",
        "para todo caminho (P1 = 1) até (P2 = 0)",
        "para todos os caminhos (P1 = 1) até (P2 = 0)",
    ] {
        match &parse_ctl_query(query, &net).unwrap_or_else(|e| panic!("{query}: {e}")) {
            CTLFormula::AU(_, _) => {}
            other => panic!("{query}: expected AU, got {other:?}"),
        }
    }
}

#[test]
fn test_infix_until_keeps_a_compound_left_side_in_parentheses() {
    let net = make_test_net();
    match &parse_ctl_query("existe caminho (P1 = 1 e P2 = 0) até (P1 = 0)", &net).unwrap() {
        CTLFormula::EU(left, _) => assert!(matches!(**left, CTLFormula::And(_, _))),
        other => panic!("expected EU with a conjunction on the left, got {other:?}"),
    }
}

#[test]
fn test_forall_until_with_comparison_on_the_left() {
    let net = make_test_net();
    for query in [
        "para todo caminho (P1 >= 1) até (P2 = 0)",
        "para todo caminho (P1 >= 1) ate (P2 = 0)",
        "for all paths (P1 >= 1) until (P2 = 0)",
    ] {
        assert!(matches!(parse_ctl_query(query, &net).unwrap_or_else(|e| panic!("{query}: {e}")), CTLFormula::AU(_, _)), "{query}");
    }
}

#[test]
fn deadlock_atom_and_quoted_names_parse() {
    let net = make_test_net();
    for q in ["exists path eventually deadlock", "existe caminho eventualmente bloqueio", "for all paths always not deadlock"] {
        parse_ctl_query(q, &net).unwrap_or_else(|e| panic!("{q}: {e:?}"));
    }
    // Reserved words become places when quoted or followed by a comparison.
    for q in ["exists path eventually \"P1\" = 1", "exists path eventually `P0` >= 1"] {
        parse_ctl_query(q, &net).unwrap_or_else(|e| panic!("{q}: {e:?}"));
    }
    assert!(parse_dsl("exists path eventually B = 1").is_ok());
    assert!(parse_dsl("exists path eventually `x` > 0").is_ok());
    assert!(parse_dsl("exists path eventually \"unterminated").is_err());
}

fn linear(formula: &CTLFormula) -> (Vec<(u32, i64)>, ComparisonOp, i64) {
    match formula {
        CTLFormula::AtomicProposition(p) => (
            p.terms.iter().map(|t| (t.place_id.0, t.coefficient)).collect(),
            p.operator,
            p.constant,
        ),
        other => panic!("expected atom, got {other:?}"),
    }
}

#[test]
fn linear_arithmetic_atoms_are_normalised() {
    let net = make_test_net();
    let f = parse_ctl_query("P0 + P1 >= 2", &net).unwrap();
    assert_eq!(linear(&f), (vec![(0, 1), (1, 1)], ComparisonOp::Ge, 2));
    // right-hand places move to the left
    let f = parse_ctl_query("P0 <= P1", &net).unwrap();
    assert_eq!(linear(&f), (vec![(0, 1), (1, -1)], ComparisonOp::Le, 0));
    let f = parse_ctl_query("2*P0 - P1 = 3 + 1", &net).unwrap();
    assert_eq!(linear(&f), (vec![(0, 2), (1, -1)], ComparisonOp::Eq, 4));
    let f = parse_ctl_query("P0 * 3 > 1", &net).unwrap();
    assert_eq!(linear(&f), (vec![(0, 3)], ComparisonOp::Gt, 1));
    // constants on the left, repeated places merge, cancelled places fold to a constant
    let f = parse_ctl_query("1 <= P0 + P0", &net).unwrap();
    assert_eq!(linear(&f), (vec![(0, -2)], ComparisonOp::Le, -1));
    assert!(matches!(parse_ctl_query("P0 - P0 = 0", &net).unwrap(), CTLFormula::True));
    assert!(matches!(parse_ctl_query("P0 - P0 > 0", &net).unwrap(), CTLFormula::False));
    // inside temporal operators and boolean connectives
    parse_ctl_query("exists path eventually (P0 + P1 >= 2 and P2 <= P0)", &net).unwrap();
    parse_ctl_query("existe caminho eventualmente P0 + P1 >= 2", &net).unwrap();
}

#[test]
fn nonlinear_and_malformed_arithmetic_is_rejected() {
    let net = make_test_net();
    assert!(parse_ctl_query("P0 * P1 >= 1", &net).is_err());
    assert!(parse_ctl_query("P0 + >= 1", &net).is_err());
    assert!(parse_ctl_query("P0 + P1", &net).is_err());
    assert!(parse_ctl_query("P0 + Nope >= 1", &net).is_err());
    // legacy forms keep working
    assert_atomic_prop(&parse_ctl_query("P1 >= 3", &net).unwrap(), 1, ComparisonOp::Ge, 3);
    parse_ctl_query("-(P0 = 1)", &net).unwrap();
}
