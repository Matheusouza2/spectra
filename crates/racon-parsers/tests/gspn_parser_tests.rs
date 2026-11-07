//! Integration tests for MercuryScript GSPN parser extensions.
//!
//! Tests that parse_gspn correctly extracts GSPN annotation data
//! (timed transitions with rate/delay and immediate transitions with
//! weight/priority) while maintaining backward compatibility.

use racon_parsers::mercury::{MercuryParserConfig, MercuryParserImpl};

#[test]
fn test_parse_gspn_timed_delay() {
    let ms = r#"SPN Model {
        place P0(tokens = 1);
        place P1;
        timedTransition T0(inputs = [P0], outputs = [P1], delay = 2.0);
    }
    main {}"#;
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_gspn(ms).unwrap();
    assert!(result.gspn_spec.is_some());
    let spec = result.gspn_spec.unwrap();
    let tt = spec
        .transition_types
        .get(&racon_core::net::TransitionId(0))
        .unwrap();
    match tt {
        racon_gspn::TransitionType::Timed { rate } => assert!((*rate - 0.5).abs() < 1e-10),
        _ => panic!("expected Timed"),
    }
}

#[test]
fn test_parse_gspn_timed_rate() {
    let ms = r#"SPN Model {
        place P0(tokens = 1);
        place P1;
        timedTransition T0(inputs = [P0], outputs = [P1], rate = 1.5);
    }
    main {}"#;
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_gspn(ms).unwrap();
    let spec = result.gspn_spec.unwrap();
    match &spec.transition_types[&racon_core::net::TransitionId(0)] {
        racon_gspn::TransitionType::Timed { rate } => assert!((*rate - 1.5).abs() < 1e-10),
        _ => panic!("expected Timed"),
    }
}

#[test]
fn test_parse_gspn_immediate() {
    let ms = r#"SPN Model {
        place P0(tokens = 1);
        place P1;
        place P2;
        timedTransition T0(inputs = [P0], outputs = [P1], delay = 1.0);
        immediateTransition IM0(inputs = [P1], outputs = [P2], weight = 2, priority = 1);
    }
    main {}"#;
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_gspn(ms).unwrap();
    let spec = result.gspn_spec.unwrap();
    assert_eq!(spec.transition_types.len(), 2);
    match &spec.transition_types[&racon_core::net::TransitionId(0)] {
        racon_gspn::TransitionType::Timed { rate } => assert!((*rate - 1.0).abs() < 1e-10),
        _ => panic!("expected Timed"),
    }
    match &spec.transition_types[&racon_core::net::TransitionId(1)] {
        racon_gspn::TransitionType::Immediate { weight, priority } => {
            assert!((*weight - 2.0).abs() < 1e-10);
            assert_eq!(*priority, 1);
        }
        _ => panic!("expected Immediate"),
    }
}

#[test]
fn test_parse_gspn_no_gspn_data() {
    // File without any rate/delay — parse_gspn returns None spec
    let ms = r#"SPN Model {
        place P0(tokens = 1);
        place P1;
        timedTransition T0(inputs = [P0], outputs = [P1], delay = 0.0);
    }
    main {}"#;
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_gspn(ms).unwrap();
    assert!(result.gspn_spec.is_none());
}

#[test]
fn test_parse_gspn_immediate_default_weight_priority() {
    // Immediate transition with no weight/priority uses defaults (weight=1, priority=0)
    let ms = r#"SPN Model {
        place P0(tokens = 1);
        place P1;
        immediateTransition IM0(inputs = [P0], outputs = [P1]);
    }
    main {}"#;
    let parser = MercuryParserImpl::new(MercuryParserConfig::default());
    let result = parser.parse_gspn(ms).unwrap();
    let spec = result.gspn_spec.unwrap();
    match &spec.transition_types[&racon_core::net::TransitionId(0)] {
        racon_gspn::TransitionType::Immediate { weight, priority } => {
            assert!((*weight - 1.0).abs() < 1e-10);
            assert_eq!(*priority, 0);
        }
        _ => panic!("expected Immediate"),
    }
}
