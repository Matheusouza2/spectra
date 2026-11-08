//! Conformance tests for the Bryant-1986 BDD engine.
//!
//! Oracles: truth-table evaluation for the Boolean kernel, breadth-first
//! enumeration (and the MDD saturation engine) for 1-safe reachability.

use std::collections::{BTreeSet, VecDeque};

use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, PlaceId, TokenValue};
use racon_reduction::bdd::{generate, Bdd, BddConfig, FALSE, TRUE};
use racon_reduction::decision_diagrams as mdd;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

#[test]
fn canonical_form_and_boolean_kernel() {
    let mut b = Bdd::new(10_000);
    let (x0, x1, x2) = (b.var(0), b.var(1), b.var(2));
    // De Morgan and distributivity give the *same vertex* (Theorem 1).
    let a = b.and(x0, x1);
    let na = b.not(a);
    let (n0, n1) = (b.not(x0), b.not(x1));
    assert_eq!(na, b.or(n0, n1));
    let y = b.or(x1, x2);
    let lhs = b.and(x0, y);
    let (p, q) = (b.and(x0, x1), b.and(x0, x2));
    assert_eq!(lhs, b.or(p, q));
    // x ∨ ¬x = 1, x ∧ ¬x = 0.
    assert_eq!(b.or(x0, n0), TRUE);
    assert_eq!(b.and(x0, n0), FALSE);
    // Satisfy-count: x0·(x1+x2) over 3 vars has 3 models.
    assert_eq!(b.satisfy_count(lhs, 3), Some(3));
    assert_eq!(b.satisfy_count(TRUE, 5), Some(32));
    assert_eq!(b.satisfy_count(x2, 3), Some(4));
    // Restrict and exists.
    assert_eq!(b.restrict(lhs, 0, false), FALSE);
    assert_eq!(b.restrict(lhs, 0, true), y);
    let mut q1 = vec![false; 3];
    q1[1] = true;
    q1[2] = true;
    assert_eq!(b.exists(lhs, &q1), x0);
    // Satisfy-one returns a model.
    let m = b.satisfy_one(lhs, 3).unwrap();
    assert!(b.eval(lhs, &m));
    assert!(b.satisfy_one(FALSE, 3).is_none());
}

#[test]
fn random_functions_match_truth_tables() {
    let mut rng = Lcg(0xB4_1986);
    for _ in 0..300 {
        let n = 1 + rng.next(5) as usize;
        let mut b = Bdd::new(100_000);
        let mut pool: Vec<usize> = (0..n as u32).map(|i| b.var(i)).collect();
        let mut tables: Vec<Vec<bool>> =
            (0..n).map(|i| (0..1u32 << n).map(|m| m >> i & 1 == 1).collect()).collect();
        for _ in 0..8 {
            let (i, j) = (rng.next(pool.len() as u64) as usize, rng.next(pool.len() as u64) as usize);
            let (f, g, tf, tg) = (pool[i], pool[j], tables[i].clone(), tables[j].clone());
            let (r, t) = match rng.next(3) {
                0 => (b.and(f, g), tf.iter().zip(&tg).map(|(a, c)| *a && *c).collect()),
                1 => (b.or(f, g), tf.iter().zip(&tg).map(|(a, c)| *a || *c).collect()),
                _ => (b.not(f), tf.iter().map(|a| !a).collect()),
            };
            pool.push(r);
            tables.push(t);
        }
        for (f, t) in pool.iter().zip(&tables) {
            let models = t.iter().filter(|&&v| v).count() as u128;
            assert_eq!(b.satisfy_count(*f, n), Some(models));
            for (m, &expected) in t.iter().enumerate() {
                let a: Vec<bool> = (0..n).map(|i| m >> i & 1 == 1).collect();
                assert_eq!(b.eval(*f, &a), expected);
            }
            assert_eq!(b.satisfy_all(*f, n, usize::MAX).unwrap().len() as u128, models);
        }
    }
}

type Arcs = Vec<(Vec<usize>, Vec<usize>)>;

fn build(arcs: &Arcs, m0: &[u64]) -> Option<PetriNet> {
    let mut b = PetriNetBuilder::default();
    for p in 0..m0.len() {
        b.add_place(format!("p{p}"), None);
    }
    let mut any = false;
    for (t, (pre, post)) in arcs.iter().enumerate() {
        let tid = b.add_transition(format!("t{t}"), None);
        for &p in pre {
            b.add_arc(ArcEndpoint::Place(PlaceId(p as u32)), ArcEndpoint::Transition(tid), 1);
            any = true;
        }
        for &p in post {
            b.add_arc(ArcEndpoint::Transition(tid), ArcEndpoint::Place(PlaceId(p as u32)), 1);
            any = true;
        }
    }
    if !any {
        return None;
    }
    b.build(m0.iter().map(|&n| TokenValue::Finite(n)).collect()).ok()
}

/// BFS; `None` if some firing would exceed one token (net not 1-safe).
fn bfs_safe(arcs: &Arcs, m0: &[u64]) -> Option<BTreeSet<Vec<u64>>> {
    let mut seen = BTreeSet::from([m0.to_vec()]);
    let mut queue = VecDeque::from([m0.to_vec()]);
    while let Some(m) = queue.pop_front() {
        for (pre, post) in arcs {
            if pre.iter().all(|&p| m[p] >= 1) {
                let mut n = m.clone();
                for &p in pre {
                    n[p] -= 1;
                }
                for &p in post {
                    n[p] += 1;
                }
                if n.iter().any(|&v| v > 1) {
                    return None;
                }
                if seen.insert(n.clone()) {
                    queue.push_back(n);
                }
            }
        }
    }
    Some(seen)
}

#[test]
fn bdd_reachability_matches_bfs_and_mdd() {
    let mut rng = Lcg(0xBDD_5AFE);
    let (mut safe, mut unsafe_refused) = (0, 0);
    for case in 0..4000 {
        let places = 2 + rng.next(6) as usize;
        let pick = |rng: &mut Lcg| (0..places).filter(|_| rng.next(100) < 30).collect::<Vec<_>>();
        let arcs: Arcs = (0..2 + rng.next(6)).map(|_| (pick(&mut rng), pick(&mut rng))).collect();
        let m0: Vec<u64> = (0..places).map(|_| rng.next(2)).collect();
        let Some(net) = build(&arcs, &m0) else { continue };
        let result = generate(&net, &BddConfig::default());
        match bfs_safe(&arcs, &m0) {
            Some(expected) => {
                let mut space = result.unwrap_or_else(|e| panic!("case {case}: {e}"));
                assert_eq!(space.states(usize::MAX).unwrap(), expected, "case {case}");
                assert_eq!(space.state_count(), Some(expected.len() as u128));
                assert!(expected.iter().all(|m| space.contains(m)));
                assert!(space.any_state().is_some_and(|m| expected.contains(&m)));
                let bounds = space.place_bounds();
                for p in 0..places {
                    assert_eq!(bounds[p], expected.iter().map(|m| m[p]).max().unwrap());
                }
                let dd = mdd::generate(&net, &mdd::DdConfig::default()).unwrap();
                assert_eq!(dd.states(usize::MAX).unwrap(), expected, "case {case} vs MDD");
                safe += 1;
            }
            None => {
                assert!(result.is_err(), "case {case}: unsafe net accepted");
                unsafe_refused += 1;
            }
        }
    }
    assert!(safe > 500 && unsafe_refused > 100, "weak coverage: {safe} safe, {unsafe_refused} refused");
}

#[test]
fn rejects_weights_and_multi_token_markings() {
    let mut b = PetriNetBuilder::default();
    let p = b.add_place("p".to_string(), None);
    let t = b.add_transition("t".to_string(), None);
    b.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 2);
    let net = b.build(vec![TokenValue::Finite(2)]).unwrap();
    assert!(generate(&net, &BddConfig::default()).is_err());
}

#[test]
fn symbolic_representation_is_exponentially_smaller_than_the_state_set() {
    // n independent two-place cycles: 2^n markings, a BDD of O(n) vertices.
    let n = 40;
    let mut b = PetriNetBuilder::default();
    let mut m0 = Vec::new();
    for i in 0..n {
        let a = b.add_place(format!("a{i}"), None);
        let c = b.add_place(format!("c{i}"), None);
        let go = b.add_transition(format!("go{i}"), None);
        let back = b.add_transition(format!("back{i}"), None);
        b.add_arc(ArcEndpoint::Place(a), ArcEndpoint::Transition(go), 1);
        b.add_arc(ArcEndpoint::Transition(go), ArcEndpoint::Place(c), 1);
        b.add_arc(ArcEndpoint::Place(c), ArcEndpoint::Transition(back), 1);
        b.add_arc(ArcEndpoint::Transition(back), ArcEndpoint::Place(a), 1);
        m0.extend([1, 0]);
    }
    let net = b.build(m0.into_iter().map(TokenValue::Finite).collect()).unwrap();
    let space = generate(&net, &BddConfig::default()).unwrap();
    assert_eq!(space.state_count(), Some(1u128 << n));
    assert!(space.node_count() <= 4 * n, "BDD has {} vertices", space.node_count());
}
