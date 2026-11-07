//! Mercury syntax for inhibitor arcs and immediate transitions.

use racon_core::{PlaceId, TransitionId, TransitionKind};
use racon_parsers::{MercuryParser, MercuryParserConfig, MercuryParserImpl};

fn parse(input: &str) -> Result<racon_core::PetriNet, racon_core::RaconError> {
    MercuryParserImpl::new(MercuryParserConfig::default()).parse_string(input)
}

#[test]
fn parses_inhibitors_with_default_and_explicit_thresholds() {
    let net = parse(
        "SPN Model
place A( tokens = 1 );
place B( tokens = 0 );
place C( tokens = 0 );
timedTransition T0( inputs = [A], outputs = [B], inhibitors = [C, B(3)], delay = 0.0 );
",
    )
    .unwrap();
    let inh = net.inhibitors(TransitionId(0));
    assert_eq!(inh.len(), 2);
    assert!(inh.contains(&(PlaceId(2), 1)));
    assert!(inh.contains(&(PlaceId(1), 3)));
    // inhibitors are not counted as ordinary arcs and do not enter C
    assert_eq!(net.arcs().len(), 2);
}

#[test]
fn parses_immediate_transitions_and_their_precedence() {
    let net = parse(
        "SPN Model
place A( tokens = 1 );
place B( tokens = 0 );
timedTransition N( inputs = [A], outputs = [B] );
immediateTransition I( inputs = [A], outputs = [B], weight = 2.5 );
",
    )
    .unwrap();
    assert_eq!(net.transition_kind(TransitionId(0)), TransitionKind::Normal);
    assert_eq!(net.transition_kind(TransitionId(1)), TransitionKind::Immediate);
    assert_eq!(
        net.fireable_transitions(net.initial_marking()),
        vec![TransitionId(1)]
    );
}

#[test]
fn multiline_declarations_are_supported() {
    let net = parse(
        "SPN Model
place A( tokens = 0 );
place B( tokens = 0 );
immediateTransition I(
    inputs = [B],
    outputs = [A],
    inhibitors = [A(2)]
);
",
    )
    .unwrap();
    assert_eq!(net.inhibitors(TransitionId(0)), &[(PlaceId(0), 2)]);
}

#[test]
fn unknown_inhibitor_place_is_an_error_not_a_silent_drop() {
    let err = parse(
        "SPN Model
place A( tokens = 1 );
timedTransition T( inputs = [A], outputs = [A], inhibitors = [Z] );
",
    );
    assert!(err.is_err());
}

#[test]
fn zero_or_malformed_threshold_is_rejected() {
    for bad in ["A(0)", "A(x)", "A(1"] {
        let src = format!(
            "SPN Model
place A( tokens = 1 );
timedTransition T( inputs = [A], outputs = [A], inhibitors = [{bad}] );
"
        );
        assert!(parse(&src).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn numeric_immediate_priority_is_rejected() {
    let err = parse(
        "SPN Model
place A( tokens = 1 );
immediateTransition I( inputs = [A], outputs = [A], priority = 2 );
",
    );
    assert!(err.is_err());
}

#[test]
fn legacy_scripts_still_parse_as_plain_pt() {
    let net = parse(
        "SPN Model
place A( tokens = 1 );
place B( tokens = 0 );
timedTransition T( inputs = [A], outputs = [B], delay = 1.0 );
",
    )
    .unwrap();
    assert!(!net.is_extended());
}

#[test]
fn ina_round_trip_preserves_inhibitors_kinds_and_marking() {
    use racon_parsers::{petri_net_to_ina_string, PntParser, PntParserConfig, PntParserImpl};
    let net = parse(
        "SPN Model
place A( tokens = 2 );
place B( tokens = 0 );
place G( tokens = 0 );
timedTransition N( inputs = [A(2)], outputs = [B], inhibitors = [G(3), B(5)] );
immediateTransition I( inputs = [B], outputs = [A], inhibitors = [G] );
",
    )
    .unwrap();
    let text = petri_net_to_ina_string(&net);
    let back = PntParserImpl::new(PntParserConfig::default())
        .parse_string(&text)
        .unwrap();
    assert_eq!(back.inhibitor_arcs(), net.inhibitor_arcs());
    assert_eq!(back.immediate_transitions(), net.immediate_transitions());
    assert_eq!(back.initial_marking(), net.initial_marking());
    assert_eq!(back.incidence_matrix().data(), net.incidence_matrix().data());
    // and the serialization is a fixed point
    assert_eq!(petri_net_to_ina_string(&back), text);
}

#[test]
fn plain_net_serialization_has_no_extension_block() {
    use racon_parsers::petri_net_to_ina_string;
    let net = parse(
        "SPN Model
place A( tokens = 1 );
timedTransition T( inputs = [A], outputs = [A] );
",
    )
    .unwrap();
    assert!(!petri_net_to_ina_string(&net).contains("racon-extended"));
}

#[test]
fn inhibitor_only_transition_survives_the_round_trip() {
    use racon_parsers::{petri_net_to_ina_string, PntParser, PntParserConfig, PntParserImpl};
    let net = parse(
        "SPN Model
place G( tokens = 0 );
place O( tokens = 0 );
timedTransition T( inputs = [], outputs = [O], inhibitors = [G] );
",
    )
    .unwrap();
    let back = PntParserImpl::new(PntParserConfig::default())
        .parse_string(&petri_net_to_ina_string(&net))
        .unwrap();
    assert_eq!(back.inhibitor_arcs(), net.inhibitor_arcs());
}

#[test]
fn ina_export_omits_comma_for_sink_places() {
    use racon_parsers::petri_net_to_ina_string;
    // INA reads "<pre> ," (empty post-list) as a post-list, turning a sink
    // place into a consumed one; the exporter must drop the trailing comma.
    let net = parse(
        "SPN Model
place A( tokens = 1 );
place Sink;
timedTransition T( inputs = [A], outputs = [A, Sink] );
",
    )
    .unwrap();
    let text = petri_net_to_ina_string(&net);
    let sink_row = text.lines().nth(2).unwrap();
    assert_eq!(sink_row, "1 0 0");
}
