//! Conformance tests for the polyhedral abstraction (R11).
//!
//! Oracle: exhaustive exploration of the original net. For every random net
//! and property:
//! * the markings obtained by lifting all reachable reduced markings through
//!   `E` are exactly the reachable markings of the original net (the sets
//!   "partition" `R(N1)`, Sect. 3);
//! * `check_reachable` agrees with `EF F` on the original net (Thm 4.3).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue};
use racon_reduction::polyhedral::{reduce, CheckConfig, Cmp, Formula, Verdict};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

type Arcs = Vec<(Vec<(usize, u64)>, Vec<(usize, u64)>)>;

fn explore(m0: &[u64], arcs: &Arcs, cap: usize) -> Option<BTreeSet<Vec<u64>>> {
    let mut seen = BTreeSet::from([m0.to_vec()]);
    let mut queue = VecDeque::from([m0.to_vec()]);
    while let Some(m) = queue.pop_front() {
        for (pre, post) in arcs {
            if pre.iter().all(|&(p, w)| m[p] >= w) {
                let mut n = m.clone();
                for &(p, w) in pre {
                    n[p] -= w;
                }
                for &(p, w) in post {
                    n[p] += w;
                }
                if seen.insert(n.clone()) {
                    if seen.len() > cap {
                        return None;
                    }
                    queue.push_back(n);
                }
            }
        }
    }
    Some(seen)
}

fn build(pre: &[Vec<u32>], post: &[Vec<u32>], m0: &[u64]) -> Option<PetriNet> {
    let mut b = PetriNetBuilder::default();
    for p in 0..m0.len() {
        b.add_place(format!("p{p}"), None);
    }
    let mut arcs = 0;
    for t in 0..pre.len() {
        let tid = b.add_transition(format!("t{t}"), None);
        for p in 0..m0.len() {
            if pre[t][p] > 0 {
                b.add_arc(ArcEndpoint::Place(PlaceId(p as u32)), ArcEndpoint::Transition(tid), pre[t][p]);
                arcs += 1;
            }
            if post[t][p] > 0 {
                b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(p as u32)), post[t][p]);
                arcs += 1;
            }
        }
    }
    if arcs == 0 {
        return None;
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect()).ok()
}

fn arcs_of(pre: &[Vec<u32>], post: &[Vec<u32>]) -> Arcs {
    (0..pre.len())
        .map(|t| {
            let f = |row: &Vec<u32>| row.iter().enumerate().filter(|(_, w)| **w > 0).map(|(p, w)| (p, u64::from(*w))).collect();
            (f(&pre[t]), f(&post[t]))
        })
        .collect()
}

/// Random net with injected chains (tau y1 -> y2), loops and redundant places.
fn random_net(rng: &mut Lcg) -> (Vec<Vec<u32>>, Vec<Vec<u32>>, Vec<u64>) {
    let mut places = 2 + rng.next(4) as usize;
    let mut pre: Vec<Vec<u32>> = Vec::new();
    let mut post: Vec<Vec<u32>> = Vec::new();
    for _ in 0..(2 + rng.next(4)) {
        let mut a = vec![0u32; places];
        let mut b = vec![0u32; places];
        for p in 0..places {
            if rng.next(100) < 28 {
                a[p] = 1 + (rng.next(10) / 8) as u32;
            }
            if rng.next(100) < 28 {
                b[p] = 1 + (rng.next(10) / 8) as u32;
            }
        }
        pre.push(a);
        post.push(b);
    }
    let mut m0: Vec<u64> = (0..places).map(|_| if rng.next(100) < 50 { 0 } else { 1 + rng.next(3) }).collect();
    if rng.next(100) < 55 {
        // silent chain/loop between two existing places
        let (a, b) = (rng.next(places as u64) as usize, rng.next(places as u64) as usize);
        if a != b {
            for (x, y) in [(a, b)].into_iter().chain((rng.next(100) < 40).then_some((b, a))) {
                let mut r = vec![0u32; places];
                let mut s = vec![0u32; places];
                r[x] = 1;
                s[y] = 1;
                pre.push(r);
                post.push(s);
            }
        }
    }
    if rng.next(100) < 45 {
        // redundant copy of a place (z = y + C)
        let y = rng.next(places as u64) as usize;
        for t in 0..pre.len() {
            let (a, b) = (pre[t][y], post[t][y]);
            pre[t].push(a);
            post[t].push(b);
        }
        m0.push(m0[y] + rng.next(3));
        places += 1;
    }
    let _ = places;
    (pre, post, m0)
}

fn random_formula(rng: &mut Lcg, places: usize) -> Formula {
    let atom = |rng: &mut Lcg| {
        let n = 1 + rng.next(2) as usize;
        let terms = (0..n)
            .map(|_| (PlaceId(rng.next(places as u64) as u32), if rng.next(4) == 0 { -1 } else { 1 + rng.next(2) as i64 }))
            .collect();
        let op = [Cmp::Le, Cmp::Lt, Cmp::Ge, Cmp::Gt, Cmp::Eq, Cmp::Ne][rng.next(6) as usize];
        Formula::Atom { terms, op, rhs: rng.next(5) as i64 - 1 }
    };
    match rng.next(3) {
        0 => atom(rng),
        1 => Formula::And(vec![atom(rng), atom(rng)]),
        _ => Formula::Or(vec![atom(rng), Formula::Not(Box::new(atom(rng)))]),
    }
}

#[test]
fn lifted_reduced_markings_equal_the_original_reachability_set() {
    let mut rng = Lcg(0x5EED_0001);
    let mut coverage: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut checked, mut reachable, mut unreachable) = (0, 0, 0);
    for case in 0..3500 {
        let (pre, post, m0) = random_net(&mut rng);
        let Some(net) = build(&pre, &post, &m0) else { continue };
        let Some(original) = explore(&m0, &arcs_of(&pre, &post), 3_000) else { continue };
        let red = reduce(&net).expect("plain net");
        for (rule, n) in &red.applications {
            *coverage.entry(rule).or_insert(0) += n;
        }

        // Reachable reduced markings, lifted through E.
        let reduced_m0 = red.initial_marking();
        let remaining = red.remaining_places();
        let idx: BTreeMap<&str, usize> = remaining.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        let reduced_arcs: Arcs = match red.reduced_net() {
            Some(rn) => (0..rn.transitions().len())
                .map(|t| {
                    let id = rn.transitions()[t].id;
                    let conv = |s: &[(PlaceId, u32)]| s.iter().map(|&(p, w)| (idx[rn.places()[p.0 as usize].name.as_str()], u64::from(w))).collect();
                    (conv(rn.preset(id)), conv(rn.postset(id)))
                })
                .collect(),
            None => Vec::new(),
        };
        let reduced_states = explore(&reduced_m0, &reduced_arcs, 100_000).expect("reduced net is not larger");
        let lifted: BTreeSet<Vec<u64>> = reduced_states.iter().flat_map(|m| red.lift(m)).collect();
        assert_eq!(lifted, original, "case {case}: E {:?} {:?}", red.equation_strings(), red.applications);

        // EF F agrees.
        for _ in 0..3 {
            let f = random_formula(&mut rng, m0.len());
            let expected = original.iter().any(|m| f.eval(m));
            match red.check_reachable(&f, &CheckConfig::default()) {
                Verdict::Reachable { original_marking, .. } => {
                    assert!(expected && original.contains(&original_marking) && f.eval(&original_marking), "case {case}");
                    reachable += 1;
                }
                Verdict::Unreachable => {
                    assert!(!expected, "case {case}: EF {f:?} lost");
                    unreachable += 1;
                }
                Verdict::Unknown { .. } => panic!("case {case}: unexpected Unknown"),
            }
        }
        checked += 1;
    }
    println!("checked={checked} reachable={reachable} unreachable={unreachable} coverage={coverage:?}");
    assert!(checked > 1000 && reachable > 500 && unreachable > 500);
    for rule in ["DEADP", "CONSTANT", "CONCAT", "AGG", "RED"] {
        assert!(coverage.get(rule).copied().unwrap_or(0) > 0, "{rule} never fired: {coverage:?}");
    }
}

#[test]
fn concat_matches_the_paper_equation() {
    // y1 -tau-> y2, y1 fed by a, drained by b; y2 drained by c.
    let net = build(
        &[vec![0, 0], vec![1, 0], vec![1, 0], vec![0, 1]],
        &[vec![1, 0], vec![0, 1], vec![0, 0], vec![0, 0]],
        &[2, 0],
    )
    .unwrap();
    let red = reduce(&net).unwrap();
    assert_eq!(red.applications.get("CONCAT"), Some(&1));
    assert_eq!(red.equation_strings(), vec!["a1 = p0 + p1".to_owned()]);
    assert_eq!(red.remaining_places(), vec!["a1"]);
    assert_eq!(red.initial_marking(), vec![2]);
    // 3 tokens in p1 and none in p0 is reachable in the original net.
    let f = Formula::And(vec![
        Formula::Atom { terms: vec![(PlaceId(1), 1)], op: Cmp::Ge, rhs: 3 },
        Formula::Atom { terms: vec![(PlaceId(0), 1)], op: Cmp::Eq, rhs: 0 },
    ]);
    assert!(matches!(red.check_reachable(&f, &CheckConfig { max_states: 200, ..Default::default() }), Verdict::Reachable { .. }));
}

#[test]
fn bmc_script_has_one_check_per_depth_and_the_equations() {
    let net = build(
        &[vec![0, 0], vec![1, 0], vec![1, 0], vec![0, 1]],
        &[vec![1, 0], vec![0, 1], vec![0, 0], vec![0, 0]],
        &[2, 0],
    )
    .unwrap();
    let red = reduce(&net).unwrap();
    let f = Formula::Atom { terms: vec![(PlaceId(1), -1)], op: Cmp::Le, rhs: -3 };
    let smt = red.bmc_smtlib(&f, 3);
    assert_eq!(smt.matches("(check-sat)").count(), 4);
    assert_eq!(smt.matches("(push 1)").count(), 4);
    assert!(smt.contains("(= |a1#2| (+ |p0#2| |p1#2|))") || smt.contains("(= |a1@2|"), "{smt}");
    assert!(smt.contains("(* (- 1)"), "{smt}");
    assert!(smt.starts_with("(set-logic QF_LIA)"));
}
