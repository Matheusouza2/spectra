//! End-to-end behavior of `racon-extended-pt-v1` through the analysis engine
//! (spec §7: PROP-01, PROP-03, COMP-01 and the safety rules of §5.3).

use std::sync::Arc;

use racon_core::TransitionId;
use racon_engine::{AnalysisRequest, AnalysisSession, SessionRepository};
use racon_reduction::ReductionKind;

fn model_path(name: &str) -> String {
    format!(
        "{}/../../tests/models/{name}.mercury",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn run(
    name: &str,
    request: AnalysisRequest,
) -> (Arc<AnalysisSession>, racon_engine::AnalysisSnapshot) {
    let repo = SessionRepository::default();
    let session = repo.insert(AnalysisSession::load(model_path(name)).expect("model loads"));
    let id = repo.create_state_space_analysis(Arc::clone(&session), request);
    assert!(repo.run_state_space_analysis(&id));
    repo.finish_state_space_analysis(&id);
    let snapshot = repo.analysis_status(&id).expect("snapshot");
    (session, snapshot)
}

fn status(session: &AnalysisSession, code: &str) -> String {
    session
        .properties()
        .into_iter()
        .find(|p| p.code == code)
        .map(|p| p.status.as_str().to_owned())
        .unwrap_or_else(|| format!("MISSING({code})"))
}

#[test]
fn prop_01_deadlock_caused_by_inhibitor_uses_extended_enabling() {
    let (session, snapshot) = run("inhibitor_deadlock", AnalysisRequest::default());
    assert_eq!(snapshot.status(), "COMPLETED", "{:?}", snapshot.error());
    let metadata = snapshot.metadata().unwrap();
    assert_eq!(metadata.semantics_version(), "racon-extended-pt-v1");
    assert_eq!(metadata.analysis_scope(), "extended_behavior");
    assert_eq!(metadata.algorithm(), "BREADTH_FIRST_SEARCH");
    assert_eq!(metadata.nodes(), 1);
    assert_eq!(status(&session, "DSt"), "TRUE");
    assert_eq!(status(&session, "L"), "FALSE");
    // the net summary reports the extension
    let summary = session.load_response().net;
    assert!(summary.extended);
    assert_eq!(summary.inhibitor_arcs, 1);
    assert_eq!(summary.arcs, 2);
}

#[test]
fn immediate_precedence_makes_the_normal_transition_dead() {
    let (session, snapshot) = run("immediate_precedence", AnalysisRequest::default());
    assert_eq!(snapshot.status(), "COMPLETED");
    assert_eq!(status(&session, "DTr"), "TRUE");
    let net = session.net();
    assert!(net.is_basic_enabled(TransitionId(1), net.initial_marking()));
    assert!(!net.is_enabled(TransitionId(1), net.initial_marking()));
    assert_eq!(status(&session, "REV"), "TRUE");
}

#[test]
fn prop_03_karp_miller_and_finkel_are_unsupported_for_extended_nets() {
    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_coverability::{
        decide_boundedness, CoverabilityEngine, CoverabilityEngineImpl, CoverabilityError,
        CoverabilityStrategy,
    };
    let session = AnalysisSession::load(model_path("inhibitor_unbounded")).unwrap();
    let net = session.net();
    let context = AnalysisContext::new(AnalysisConfig::default());
    for strategy in [
        CoverabilityStrategy::KarpMiller,
        CoverabilityStrategy::FinkelMinimalCoverability,
    ] {
        let result = CoverabilityEngineImpl::default().build_complete(&net, strategy, &context);
        assert!(matches!(result, Err(CoverabilityError::Unsupported(_))));
    }
    assert!(matches!(
        decide_boundedness(&net, 1000, &context),
        Err(CoverabilityError::Unsupported(_))
    ));
}

#[test]
fn reductions_are_rejected_for_extended_nets() {
    let (_, snapshot) = run(
        "inhibitor_bounded_cycle",
        AnalysisRequest {
            reductions: vec![ReductionKind::Stubborn],
            ..AnalysisRequest::default()
        },
    );
    assert_eq!(snapshot.status(), "FAILED");
    assert!(snapshot.error().unwrap().starts_with("UNSUPPORTED"));
}

#[test]
fn truncated_exploration_is_unknown_not_a_proof() {
    let (session, snapshot) = run("inhibitor_unbounded", AnalysisRequest::default());
    assert_eq!(snapshot.status(), "FAILED");
    assert!(snapshot
        .error()
        .unwrap()
        .starts_with("UNKNOWN (state_limit_reached)"));
    // nothing behavioral was concluded
    assert_eq!(status(&session, "DSt"), "NOT_COMPUTED");
    assert_eq!(status(&session, "B"), "NOT_COMPUTED");
}

#[test]
fn extended_properties_carry_their_analysis_scope() {
    let (session, _) = run("inhibitor_bounded_cycle", AnalysisRequest::default());
    let scopes: std::collections::HashMap<_, _> = session
        .properties()
        .into_iter()
        .map(|p| (p.code, p.analysis_scope))
        .collect();
    assert_eq!(scopes["ORD"].as_deref(), Some("structural_projection"));
    assert_eq!(scopes["L"].as_deref(), Some("extended_behavior"));
}

#[test]
fn comp_01_plain_nets_keep_plain_scope_and_coverability() {
    let (session, snapshot) = run("bounded", AnalysisRequest::default());
    assert_eq!(snapshot.status(), "COMPLETED");
    assert_eq!(
        snapshot.metadata().unwrap().analysis_scope(),
        "plain_pt_behavior"
    );
    assert!(session
        .properties()
        .iter()
        .all(|p| p.analysis_scope.is_none()));
}

// ---------------------------------------------------------------------------
// Differential test (spec §7.1): the engine's exact BFS against an independent,
// naive reference written directly from the definitions of §2 on raw matrices.
// ---------------------------------------------------------------------------

mod differential {
    use std::collections::{BTreeSet, HashMap, VecDeque};

    use racon_core::execution::{AnalysisConfig, AnalysisContext};
    use racon_core::graph::StateSpace;
    use racon_core::{
        ArcEndpoint, NodeId, PetriNetBuilder, PlaceId, TokenValue, TransitionId, TransitionKind,
    };
    use racon_reachability::{ReachabilityEngine, ReachabilityEngineImpl};

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % bound
        }
    }

    struct Raw {
        pre: Vec<Vec<u64>>,  // [t][p]
        post: Vec<Vec<u64>>, // [t][p]
        inh: Vec<Vec<u64>>,  // [t][p], 0 = no arc
        imm: Vec<bool>,
        m0: Vec<u64>,
    }

    /// Reference: Fireable(M) straight from §2.1-§2.2.
    fn reference_fireable(raw: &Raw, m: &[u64]) -> Vec<usize> {
        let basic = |t: usize| {
            (0..m.len()).all(|p| m[p] >= raw.pre[t][p])
                && (0..m.len()).all(|p| raw.inh[t][p] == 0 || m[p] < raw.inh[t][p])
        };
        let enabled: Vec<usize> = (0..raw.pre.len()).filter(|&t| basic(t)).collect();
        let imm: Vec<usize> = enabled.iter().copied().filter(|&t| raw.imm[t]).collect();
        if imm.is_empty() {
            enabled
        } else {
            imm
        }
    }

    fn reference_graph(raw: &Raw, cap: usize) -> Option<BTreeSet<(Vec<u64>, usize, Vec<u64>)>> {
        let mut seen: HashMap<Vec<u64>, ()> = HashMap::new();
        let mut edges = BTreeSet::new();
        let mut queue = VecDeque::from([raw.m0.clone()]);
        seen.insert(raw.m0.clone(), ());
        while let Some(m) = queue.pop_front() {
            for t in reference_fireable(raw, &m) {
                let next: Vec<u64> = (0..m.len()).map(|p| m[p] - raw.pre[t][p] + raw.post[t][p]).collect();
                edges.insert((m.clone(), t, next.clone()));
                if seen.insert(next.clone(), ()).is_none() {
                    if seen.len() > cap {
                        return None;
                    }
                    queue.push_back(next);
                }
            }
        }
        // isolated initial marking still counts as a state
        edges.insert((raw.m0.clone(), usize::MAX, raw.m0.clone()));
        Some(edges)
    }

    fn random_raw(rng: &mut Lcg) -> Raw {
        let places = 2 + rng.next(3) as usize;
        let transitions = 2 + rng.next(3) as usize;
        let mut raw = Raw {
            pre: vec![vec![0; places]; transitions],
            post: vec![vec![0; places]; transitions],
            inh: vec![vec![0; places]; transitions],
            imm: vec![false; transitions],
            m0: (0..places).map(|_| rng.next(3)).collect(),
        };
        for t in 0..transitions {
            for p in 0..places {
                if rng.next(3) == 0 {
                    raw.pre[t][p] = 1 + rng.next(2);
                }
                if rng.next(3) == 0 {
                    raw.post[t][p] = 1 + rng.next(2);
                }
                if rng.next(4) == 0 {
                    raw.inh[t][p] = 1 + rng.next(3);
                }
            }
            raw.imm[t] = rng.next(3) == 0;
            // keep every element incident to an arc so the builder accepts it
            if raw.pre[t].iter().chain(&raw.post[t]).all(|&w| w == 0) {
                raw.post[t][0] = 1;
            }
        }
        for p in 0..places {
            let touched = (0..transitions).any(|t| raw.pre[t][p] + raw.post[t][p] + raw.inh[t][p] > 0);
            if !touched {
                raw.post[0][p] = 1;
            }
        }
        raw
    }

    #[test]
    fn engine_bfs_matches_independent_reference_on_random_extended_nets() {
        let mut rng = Lcg(0xC0FFEE);
        let (mut compared, mut with_inhibitors, mut with_preemption) = (0, 0, 0);
        for _ in 0..400 {
            let raw = random_raw(&mut rng);
            let Some(reference) = reference_graph(&raw, 400) else {
                continue; // unbounded or too large: not comparable
            };

            let mut b = PetriNetBuilder::default();
            for p in 0..raw.m0.len() {
                b.add_place(format!("p{p}"), None);
            }
            for t in 0..raw.pre.len() {
                let tid = b.add_transition(format!("t{t}"), None);
                if raw.imm[t] {
                    b.set_transition_kind(tid, TransitionKind::Immediate);
                }
                for p in 0..raw.m0.len() {
                    if raw.pre[t][p] > 0 {
                        b.add_arc(
                            ArcEndpoint::Place(PlaceId(p as u32)),
                            ArcEndpoint::Transition(tid),
                            raw.pre[t][p] as u32,
                        );
                    }
                    if raw.post[t][p] > 0 {
                        b.add_arc(
                            ArcEndpoint::Transition(tid),
                            ArcEndpoint::Place(PlaceId(p as u32)),
                            raw.post[t][p] as u32,
                        );
                    }
                    if raw.inh[t][p] > 0 {
                        b.add_inhibitor_arc(PlaceId(p as u32), tid, raw.inh[t][p]);
                    }
                }
            }
            let net = b
                .build(raw.m0.iter().map(|&n| TokenValue::Finite(n)).collect())
                .expect("random net is valid");

            let context = AnalysisContext::new(AnalysisConfig::default());
            let graph = ReachabilityEngineImpl::default()
                .build_complete(&net, &context)
                .expect("bounded by the reference check");

            let mut engine_edges = BTreeSet::new();
            engine_edges.insert((raw.m0.clone(), usize::MAX, raw.m0.clone()));
            let to_vec = |node: NodeId| -> Vec<u64> {
                graph
                    .marking(node)
                    .tokens
                    .iter()
                    .map(|t| match t {
                        TokenValue::Finite(n) => *n,
                        other => panic!("unexpected token {other:?}"),
                    })
                    .collect()
            };
            for edge in &graph.edges {
                engine_edges.insert((
                    to_vec(edge.source),
                    edge.transition.0 as usize,
                    to_vec(edge.target),
                ));
            }
            assert_eq!(engine_edges, reference, "labelled graphs differ for {:?}", raw.m0);

            // simulation uses the very same relation as the graph
            for node in 0..graph.node_count() {
                let marking = graph.marking(NodeId(node as u32));
                let fireable: Vec<usize> = net
                    .fireable_transitions(marking)
                    .into_iter()
                    .map(|t: TransitionId| t.0 as usize)
                    .collect();
                let from_graph: Vec<usize> = engine_edges
                    .iter()
                    .filter(|(src, t, _)| *src == to_vec(NodeId(node as u32)) && *t != usize::MAX)
                    .map(|(_, t, _)| *t)
                    .collect();
                assert_eq!(fireable, from_graph);
            }

            compared += 1;
            with_inhibitors += usize::from(net.has_inhibitor_arcs());
            with_preemption +=
                usize::from((0..graph.node_count()).any(|n| {
                    !net.preempted_transitions(graph.marking(NodeId(n as u32))).is_empty()
                }));
        }
        assert!(compared >= 100, "only {compared} nets were comparable");
        assert!(with_inhibitors >= 50 && with_preemption >= 10,
            "generator too weak: inh={with_inhibitors}, preempt={with_preemption}");
    }
}
