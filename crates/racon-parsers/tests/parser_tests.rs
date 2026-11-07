//! Integration tests for racon-parsers (PNT, PNML, MercuryScript).

use racon_parsers::{
    MercuryParser, MercuryParserConfig, MercuryParserImpl, PnmlParser, PnmlParserConfig,
    PnmlParserImpl, PntParser, PntParserConfig, PntParserImpl,
};

// =============================================================================
// PNT Parser Tests
// =============================================================================

const VALID_PNT: &str = "\
PLACES
P1: tokens=1
P2: tokens=0
TRANSITIONS
T1
T2
ARCS
P1 -> T1
T1 -> P2
P2 -> T2: weight=2
T2 -> P1
";

#[test]
fn test_pnt_parse_valid_net() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(VALID_PNT)
        .expect("Should parse valid PNT");
    assert_eq!(net.place_count(), 2, "Should have 2 places");
    assert_eq!(net.transition_count(), 2, "Should have 2 transitions");
    assert_eq!(net.arcs().len(), 4, "Should have 4 arcs");
}

#[test]
fn test_pnt_initial_marking() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(VALID_PNT)
        .expect("Should parse valid PNT");
    let marking = net.initial_marking();
    assert_eq!(
        marking.get(0),
        Some(&racon_core::TokenValue::Finite(1)),
        "P1 should have 1 token"
    );
    assert_eq!(
        marking.get(1),
        Some(&racon_core::TokenValue::Finite(0)),
        "P2 should have 0 tokens"
    );
}

#[test]
fn test_pnt_empty_input() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let err = parser
        .parse_string("")
        .expect_err("Should reject empty input");
    assert!(
        err.to_string().contains("No places") || err.to_string().contains("ParseError"),
        "Expected error about missing places or parse error, got: {}",
        err
    );
}

#[test]
fn test_pnt_missing_sections() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let err = parser
        .parse_string("PLACES\nP1: tokens=0\n")
        .expect_err("Should reject missing sections");
    assert!(err.to_string().contains("ParseError") || err.to_string().contains("Invalid"));
}

#[test]
fn test_pnt_transition_id_must_start_with_t() {
    let input = "\
PLACES
P1: tokens=0
TRANSITIONS
X1
ARCS
";
    let parser = PntParserImpl::new(PntParserConfig::default());
    let err = parser
        .parse_string(input)
        .expect_err("Should reject non-T transition");
    assert!(err.to_string().contains("must start with T"));
}

// =============================================================================
// PNML Parser Tests
// =============================================================================

const VALID_PNML: &str = r#"<?xml version="1.0"?>
<pnml>
  <net id="net1">
    <place id="p1">
      <name><text>P1</text></name>
      <initialMarking><text>2</text></initialMarking>
    </place>
    <place id="p2">
      <name><text>P2</text></name>
      <initialMarking><text>0</text></initialMarking>
    </place>
    <transition id="t1">
      <name><text>T1</text></name>
    </transition>
    <transition id="t2">
      <name><text>T2</text></name>
    </transition>
    <arc id="a1" source="p1" target="t1">
      <inscription><text>1</text></inscription>
    </arc>
    <arc id="a2" source="t1" target="p2">
      <inscription><text>1</text></inscription>
    </arc>
    <arc id="a3" source="p2" target="t2">
      <inscription><text>2</text></inscription>
    </arc>
    <arc id="a4" source="t2" target="p1">
      <inscription><text>1</text></inscription>
    </arc>
  </net>
</pnml>"#;

#[test]
fn test_pnml_parse_valid_net() {
    let parser = PnmlParserImpl::new(PnmlParserConfig::default());
    let net = parser
        .parse_string(VALID_PNML)
        .expect("Should parse valid PNML");
    assert_eq!(net.place_count(), 2, "Should have 2 places");
    assert_eq!(net.transition_count(), 2, "Should have 2 transitions");
    assert_eq!(net.arcs().len(), 4, "Should have 4 arcs");
}

#[test]
fn test_pnml_initial_marking() {
    let parser = PnmlParserImpl::new(PnmlParserConfig::default());
    let net = parser
        .parse_string(VALID_PNML)
        .expect("Should parse valid PNML");
    let marking = net.initial_marking();
    assert_eq!(
        marking.get(0),
        Some(&racon_core::TokenValue::Finite(2)),
        "P1 should have 2 tokens"
    );
    assert_eq!(
        marking.get(1),
        Some(&racon_core::TokenValue::Finite(0)),
        "P2 should have 0 tokens"
    );
}

#[test]
fn test_pnml_invalid_xml() {
    let parser = PnmlParserImpl::new(PnmlParserConfig::default());
    let result = parser.parse_string("not xml");
    assert!(result.is_err(), "Should reject invalid XML");
}

#[test]
fn test_pnml_no_places_triggers_error() {
    let xml = r#"<?xml version="1.0"?><pnml><net id="empty"></net></pnml>"#;
    let parser = PnmlParserImpl::new(PnmlParserConfig::default());
    let result = parser.parse_string(xml);
    assert!(
        result.is_err(),
        "PNML with no places should fail validation"
    );
}

// =============================================================================
// MercuryScript Parser Tests
// =============================================================================

const VALID_MERCURY: &str = "\
SPN Model
place P0( tokens = 1 );
place P1( tokens = 0 );
timedTransition T0( inputs = [P0], outputs = [P1], delay = 0.0 );
";

#[test]
fn test_mercury_parse_valid() {
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let net = parser
        .parse_string(VALID_MERCURY)
        .expect("Should parse valid Mercury");
    assert_eq!(net.place_count(), 2, "Should have 2 places");
    assert_eq!(net.transition_count(), 1, "Should have 1 transition");
    assert_eq!(net.arcs().len(), 2, "Should have 2 arcs (input + output)");
}

#[test]
fn test_mercury_initial_marking() {
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let net = parser
        .parse_string(VALID_MERCURY)
        .expect("Should parse valid Mercury");
    let marking = net.initial_marking();
    assert_eq!(
        marking.get(0),
        Some(&racon_core::TokenValue::Finite(1)),
        "P0 should have 1 token"
    );
    assert_eq!(
        marking.get(1),
        Some(&racon_core::TokenValue::Finite(0)),
        "P1 should have 0 tokens"
    );
}

#[test]
fn test_mercury_empty_input() {
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_string("");
    assert!(result.is_err(), "Should reject empty input");
}

#[test]
fn test_mercury_weighted_arcs() {
    let input = "\
SPN Model
place P0( tokens = 3 );
place P1( tokens = 0 );
timedTransition T0( inputs = [P0(2)], outputs = [P1(3)], delay = 0.0 );
";
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let net = parser
        .parse_string(input)
        .expect("Should parse weighted Mercury");
    // Verify net structure: 2 places, 1 transition, 2 arcs
    assert_eq!(net.place_count(), 2, "Should have 2 places");
    assert_eq!(net.transition_count(), 1, "Should have 1 transition");
    assert_eq!(net.arcs().len(), 2, "Should have 2 arcs (input + output)");
    // P0 should have 3 tokens
    let marking = net.initial_marking();
    assert_eq!(
        marking.get(0),
        Some(&racon_core::TokenValue::Finite(3)),
        "P0 should have 3 tokens"
    );
}

#[test]
fn test_mercury_multiline_transition_preserves_arcs() {
    let input = "\
SPN Model{\n
    place p0;\n
    place p1(tokens = 1);\n
    timedTransition move_token(\n
        inputs = [p1],\n
        outputs = [p0],\n
        delay = 0.0\n
    );\n
}\n
main {}\n";
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let net = parser
        .parse_string(input)
        .expect("Should parse multiline Mercury transition");

    assert_eq!(net.place_count(), 2);
    assert_eq!(net.transition_count(), 1);
    assert_eq!(
        net.arcs().len(),
        2,
        "Multiline inputs/outputs must create arcs"
    );

    let gspn = parser
        .parse_gspn(input)
        .expect("GSPN path should parse multiline Mercury transition");
    assert_eq!(gspn.net.arcs().len(), 2);
}

// =============================================================================
// INA (legacy) PNT Format Tests
// =============================================================================

const INA_SIMPLE: &str = "\
P   M   PRE,POST  NETZ 0:test
0 1 0,1
1 0 0,1
@
place nr.             name capacity time
     0: p0                  65535    0
     1: p1                  65535    0
@
trans nr.             name priority time
     0: t0                      0    0
     1: t1                      0    0
@";

const INA_WEIGHTED: &str = "\
P   M   PRE,POST  NETZ 0:w
0 1 0: 5,1
1 0 0,1
@
place nr.             name capacity time
     0: p0                  65535    0
     1: p1                  65535    0
@
trans nr.             name priority time
     0: t0                      0    0
     1: t1                      0    0
@";

const INA_POST_ONLY: &str = "\
P   M   PRE,POST  NETZ 0:post
0 1 , 0 1
@
place nr.             name capacity time
     0: p0                  65535    0
@
trans nr.             name priority time
     0: t0                      0    0
     1: t1                      0    0
@";

#[test]
fn test_ina_simple_net() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(INA_SIMPLE)
        .expect("Should parse INA simple net");
    assert_eq!(net.place_count(), 2, "Should have 2 places");
    assert_eq!(net.transition_count(), 2, "Should have 2 transitions");
    // Arcs: P0→T0, P1→T0 (pre) + T1→P0, T1→P1 (post) = 4
    assert_eq!(net.arcs().len(), 4, "Should have 4 arcs");
    // Check initial marking
    if let racon_core::TokenValue::Finite(v) = net.initial_marking().get(0).unwrap() {
        assert_eq!(*v, 1, "P0 should have 1 token");
    } else {
        panic!("P0 should be finite");
    }
    if let racon_core::TokenValue::Finite(v) = net.initial_marking().get(1).unwrap() {
        assert_eq!(*v, 0, "P1 should have 0 tokens");
    } else {
        panic!("P1 should be finite");
    }
}

#[test]
fn test_ina_weighted_arcs() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(INA_WEIGHTED)
        .expect("Should parse INA weighted");
    assert_eq!(net.place_count(), 2);
    assert_eq!(net.transition_count(), 2);
    // Prelist `0: 5` of P0 = pre-transition T0 with multiplicity 5: T0→P0.
    let arcs = net.arcs();
    let weighted_arc = arcs.iter().find(|a| {
        matches!(&a.source, racon_core::ArcEndpoint::Transition(_))
            && matches!(&a.target, racon_core::ArcEndpoint::Place(_))
            && a.weight == 5
    });
    assert!(
        weighted_arc.is_some(),
        "Should have a Transition→Place arc with weight 5"
    );
}

#[test]
fn test_ina_post_only() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(INA_POST_ONLY)
        .expect("Should parse INA post-only");
    assert_eq!(net.place_count(), 1);
    assert_eq!(net.transition_count(), 2);
    // Only a postlist (p•): both arcs are Place→Transition.
    let arcs = net.arcs();
    assert_eq!(arcs.len(), 2, "Should have 2 arcs, both P→T");
    for a in arcs {
        assert!(
            matches!(&a.source, racon_core::ArcEndpoint::Place(_)),
            "All arcs should be Place→Transition"
        );
        assert!(
            matches!(&a.target, racon_core::ArcEndpoint::Transition(_)),
            "All arcs should be Place→Transition"
        );
    }
}

#[test]
fn test_ina_empty_file_is_error() {
    let parser = PntParserImpl::new(PntParserConfig::default());
    let result = parser.parse_string("");
    assert!(result.is_err(), "Empty file should error");
}

#[test]
fn test_ina_racon_format_still_works() {
    // Ensure RACoN format still parses correctly alongside INA detection
    let parser = PntParserImpl::new(PntParserConfig::default());
    let net = parser
        .parse_string(VALID_PNT)
        .expect("RACoN format should still parse");
    assert_eq!(net.place_count(), 2);
    assert_eq!(net.transition_count(), 2);
}

/// INA manual §2.3.4, net 1 "Three programmers sharing two terminals": the
/// prelist is •p (T→P) and the postlist is p• (P→T).
#[test]
fn test_ina_manual_example_prelist_is_preset_of_place() {
    let input = "\
P   M   PRE,POST  NETZ 1:3_prog_2_term
0 2 4: 2 5 6, 1: 2 2 3
1 0 1, 4
2 0 2, 5
3 0 3, 6
4 1 4, 1
5 1 5, 2
6 1 6, 3
@
place nr.             name capacity time
     0: terminal_free        oo    0
     1: prog1_at_term        oo    0
     2: prog2_at_term        oo    0
     3: prog3_at_term        oo    0
     4: prog1_on_break       oo    0
     5: prog2_on_break       oo    0
     6: prog3_on_break       oo    0
@
trans nr.             name priority time
     1: login_prog1          0    0
     2: login_prog2          0    0
     3: login_prog3          0    0
     4: logout_prog1         0    0
     5: logout_prog2         0    0
     6: logout_prog3         0    0
@";
    let net = PntParserImpl::new(PntParserConfig::default())
        .parse_string(input)
        .expect("manual example parses");
    // login_prog1 (t1) consumes terminal_free (P→T, weight 2 from `1: 2`).
    let consumes_terminal = net.arcs().iter().any(|a| {
        matches!(&a.source, racon_core::ArcEndpoint::Place(p) if p.0 == 0)
            && matches!(&a.target, racon_core::ArcEndpoint::Transition(_))
            && a.weight == 2
    });
    assert!(consumes_terminal, "`1: 2` in the postlist of terminal_free is P0→t1 (weight 2)");
    // Net is conservative-like: terminal_free + prog_at_term is invariant.
    assert_eq!(net.place_count(), 7);
}
