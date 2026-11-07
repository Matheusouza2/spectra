//! The Mercury model (inhibitor arcs + immediate transitions) and its INA export
//! (complement places as test arcs + priority column) must describe the same
//! labelled transition system.

use std::collections::BTreeSet;

use racon_core::execution::{AnalysisConfig, AnalysisContext};
use racon_core::PetriNet;
use racon_parsers::{
    petri_net_to_ina_string, MercuryParser, MercuryParserConfig, MercuryParserImpl, PntParser,
    PntParserConfig, PntParserImpl,
};
use racon_reachability::{ReachabilityConfig, ReachabilityEngine, ReachabilityEngineImpl};

const MERCURY: &str = include_str!("fixtures/water_pumps.mercury");
const INA: &str = include_str!("fixtures/water_pumps_ina.pnt");

type Graph = (BTreeSet<String>, BTreeSet<(String, String, String)>);

fn graph(net: &PetriNet) -> Graph {
    let ctx = AnalysisContext::new(AnalysisConfig::default());
    let g = ReachabilityEngineImpl { config: ReachabilityConfig::default() }
        .build_complete(net, &ctx)
        .unwrap();
    let key = |i: usize| format!("{:?}", g.nodes[i].marking.tokens);
    let states = (0..g.nodes.len()).map(key).collect();
    let edges = g
        .edges
        .iter()
        .map(|e| {
            (
                key(e.source.0 as usize),
                net.transitions()[e.transition.0 as usize].name.clone(),
                key(e.target.0 as usize),
            )
        })
        .collect();
    (states, edges)
}

fn mercury() -> PetriNet {
    MercuryParserImpl::new(MercuryParserConfig::default()).parse_string(MERCURY).unwrap()
}

fn pnt(text: &str) -> PetriNet {
    PntParserImpl::new(PntParserConfig::default()).parse_string(text).unwrap()
}

#[test]
fn ina_glued_weights_and_priority_column_are_understood() {
    let net = pnt(INA);
    // 5 transitions carry priority 1 (the former immediate ones); 0:3 style weights parse.
    assert_eq!(net.immediate_transitions().len(), 5);
    assert!(net.is_extended());
    assert_eq!(net.place_count(), 12);
    assert_eq!(net.transition_count(), 15);
}

#[test]
fn ina_export_and_mercury_model_have_the_same_state_space() {
    let (ms, me) = graph(&mercury());
    let (ps, pe) = graph(&pnt(INA));
    assert_eq!((ms.len(), me.len()), (111, 374));
    assert_eq!(ms, ps);
    assert_eq!(me, pe);
}

#[test]
fn written_pnt_keeps_immediate_priority_and_round_trips() {
    let original = mercury();
    let text = petri_net_to_ina_string(&original);
    let reloaded = pnt(&text);
    assert_eq!(reloaded.immediate_transitions().len(), original.immediate_transitions().len());
    assert_eq!(graph(&original), graph(&reloaded));
}

#[test]
fn more_than_two_priority_levels_are_rejected() {
    let text = "P   M   PRE,POST  NETZ 0:t\n0 1 1 , 0\n1 0 0 , 1\n2 0 , 2\n@\nplace nr.             name capacity time\n  0: a  65535 0\n  1: b  65535 0\n  2: c  65535 0\n@\ntrans nr.             name priority time\n  0: x  0 0\n  1: y  1 0\n  2: z  2 0\n@\n";
    let err = PntParserImpl::new(PntParserConfig::default()).parse_string(text).unwrap_err();
    assert!(err.to_string().contains("priority"), "{err}");
}
