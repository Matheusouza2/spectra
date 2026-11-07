use racon_core::net::TransitionId;
use racon_gspn::{GspnSpec, TransitionType};
use std::collections::HashMap;

#[test]
fn test_gspn_spec_empty() {
    let spec = GspnSpec {
        transition_types: HashMap::new(),
    };
    assert!(spec.transition_types.is_empty());
}

#[test]
fn test_gspn_spec_timed_transition() {
    let mut map = HashMap::new();
    map.insert(TransitionId(0), TransitionType::Timed { rate: 2.0 });
    let spec = GspnSpec {
        transition_types: map,
    };
    assert_eq!(spec.transition_types.len(), 1);
    match &spec.transition_types[&TransitionId(0)] {
        TransitionType::Timed { rate } => assert_eq!(*rate, 2.0),
        _ => panic!("expected Timed"),
    }
}

#[test]
fn test_gspn_spec_immediate_transition() {
    let mut map = HashMap::new();
    map.insert(
        TransitionId(1),
        TransitionType::Immediate {
            weight: 3.0,
            priority: 0,
        },
    );
    let spec = GspnSpec {
        transition_types: map,
    };
    match &spec.transition_types[&TransitionId(1)] {
        TransitionType::Immediate { weight, priority } => {
            assert_eq!(*weight, 3.0);
            assert_eq!(*priority, 0);
        }
        _ => panic!("expected Immediate"),
    }
}

#[test]
fn test_gspn_error_display() {
    use racon_gspn::GspnError;
    let err = GspnError::NoTimedTransitions;
    assert!(!err.to_string().is_empty());
    let err = GspnError::NoConvergence {
        iterations: 100,
        residual: 1e-6,
    };
    assert!(err.to_string().contains("100"));
}
