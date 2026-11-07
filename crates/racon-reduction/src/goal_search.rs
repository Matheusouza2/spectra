//! Goal-directed stubborn-set reachability (Schmidt 1999, Thms 3-4; Bønneland
//! et al. 2019, Thm 1).
//!
//! Unlike the deadlock-oriented set in [`crate::stubborn`], the set computed
//! here is *property oriented*: at a marking `M` that does not satisfy the goal
//! it contains, for every disjunct of the goal, the transitions that can move
//! one unsatisfied linear constraint in the required direction ("interesting"
//! transitions), closed under
//!
//! * **D1** — for an enabled member, every transition sharing an input place;
//! * **D2** — for a disabled member, a *scapegoat* input place `p` with
//!   `M(p) < W(p,t)` and every transition that increases `p`.
//!
//! Any firing sequence from `M` to a goal marking fires an interesting
//! transition, hence a member of the set; the first member can be moved to the
//! front without changing the reached marking. By induction on the length of
//! the shortest path, exploring only the enabled members at each marking
//! preserves reachability of the goal, so **no ignoring proviso is needed**.
//! A marking whose set has no enabled member is a dead end for the goal.
//!
//! The search is breadth-first, so the witness is a **shortest** firing
//! sequence; Schmidt's Theorem 5 guarantees the reduced graph contains a
//! shortest path of the full graph, hence the length is exact.
//!
//! Only plain P/T nets are supported: inhibitor arcs and immediate transitions
//! invalidate the commutation argument.

use std::collections::{HashMap, VecDeque};

use racon_core::execution::AnalysisContext;
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{PetriNet, PlaceId, TransitionId};

use crate::error::ReductionError;

/// Comparison of a linear form against a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOp {
    Ge,
    Le,
    Eq,
}

/// `Σ coeff·M(place) OP constant`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearConstraint {
    pub terms: Vec<(PlaceId, i64)>,
    pub op: GoalOp,
    pub constant: i64,
}

impl LinearConstraint {
    fn value(&self, marking: &Marking) -> Option<i128> {
        let mut sum: i128 = 0;
        for &(place, coeff) in &self.terms {
            let tokens = i128::try_from(token_u128(marking.get(place.0 as usize)?)?).ok()?;
            sum += tokens * i128::from(coeff);
        }
        Some(sum)
    }

    fn holds(&self, marking: &Marking) -> bool {
        match self.value(marking) {
            Some(value) => match self.op {
                GoalOp::Ge => value >= i128::from(self.constant),
                GoalOp::Le => value <= i128::from(self.constant),
                GoalOp::Eq => value == i128::from(self.constant),
            },
            None => false,
        }
    }

    /// Direction in which the form must move at `marking` (+1 up, -1 down),
    /// or `None` when the constraint already holds.
    fn needed_direction(&self, marking: &Marking) -> Option<i8> {
        let value = self.value(marking)?;
        let constant = i128::from(self.constant);
        match self.op {
            GoalOp::Ge if value < constant => Some(1),
            GoalOp::Le if value > constant => Some(-1),
            GoalOp::Eq if value < constant => Some(1),
            GoalOp::Eq if value > constant => Some(-1),
            _ => None,
        }
    }
}

/// Reachability goal in disjunctive normal form: reached when every constraint
/// of at least one conjunction holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachGoal {
    pub disjuncts: Vec<Vec<LinearConstraint>>,
}

impl ReachGoal {
    /// Goal "reach exactly this marking".
    pub fn marking(target: &Marking) -> Option<Self> {
        let mut constraints = Vec::with_capacity(target.len());
        for index in 0..target.len() {
            let value = token_u128(target.get(index)?)?;
            constraints.push(LinearConstraint {
                terms: vec![(PlaceId(index as u32), 1)],
                op: GoalOp::Eq,
                constant: i64::try_from(value).ok()?,
            });
        }
        Some(Self {
            disjuncts: vec![constraints],
        })
    }

    pub fn single(constraint: LinearConstraint) -> Self {
        Self {
            disjuncts: vec![vec![constraint]],
        }
    }

    pub fn is_satisfied(&self, marking: &Marking) -> bool {
        self.disjuncts
            .iter()
            .any(|conjunction| conjunction.iter().all(|c| c.holds(marking)))
    }
}

/// Outcome of a goal search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalSearchOutcome {
    /// Goal reached; `path` is a firing sequence of the *original* net.
    Reachable {
        path: Vec<TransitionId>,
        marking: Marking,
    },
    /// The reduced graph was exhausted without meeting the goal: unreachable.
    Unreachable,
}

#[derive(Debug, Clone, Copy)]
pub struct GoalSearchConfig {
    pub max_nodes: usize,
    /// When false the search explores every enabled transition (baseline).
    pub use_stubborn: bool,
}

impl Default for GoalSearchConfig {
    fn default() -> Self {
        Self {
            max_nodes: 5_000_000,
            use_stubborn: true,
        }
    }
}

/// Statistics of a finished search, useful to quantify the reduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalSearchReport {
    pub outcome: GoalSearchOutcome,
    pub visited: usize,
}

pub(crate) struct NetIndex {
    /// Transitions whose net effect on the place is positive.
    producers: Vec<Vec<usize>>,
    /// Transitions that have the place as an input.
    consumers: Vec<Vec<usize>>,
    /// Net effect `post - pre` per transition, sparse.
    effects: Vec<Vec<(usize, i64)>>,
}

impl NetIndex {
    pub(crate) fn new(net: &PetriNet) -> Self {
        let places = net.place_count();
        let mut producers = vec![Vec::new(); places];
        let mut consumers = vec![Vec::new(); places];
        let mut effects = Vec::with_capacity(net.transition_count());
        for transition in net.transitions() {
            let mut delta: HashMap<usize, i64> = HashMap::new();
            for &(place, weight) in net.preset(transition.id) {
                consumers[place.0 as usize].push(transition.id.0 as usize);
                *delta.entry(place.0 as usize).or_default() -= i64::from(weight);
            }
            for &(place, weight) in net.postset(transition.id) {
                *delta.entry(place.0 as usize).or_default() += i64::from(weight);
            }
            let mut sparse: Vec<(usize, i64)> =
                delta.into_iter().filter(|&(_, d)| d != 0).collect();
            sparse.sort_unstable();
            for &(place, d) in &sparse {
                if d > 0 {
                    producers[place].push(transition.id.0 as usize);
                }
            }
            effects.push(sparse);
        }
        Self {
            producers,
            consumers,
            effects,
        }
    }

    fn effect_on(&self, transition: usize, constraint: &LinearConstraint) -> i128 {
        let mut total: i128 = 0;
        for &(place, coeff) in &constraint.terms {
            if let Ok(position) = self.effects[transition]
                .binary_search_by_key(&(place.0 as usize), |&(p, _)| p)
            {
                total += i128::from(self.effects[transition][position].1) * i128::from(coeff);
            }
        }
        total
    }
}

/// Finite token count, `None` for ω or values beyond `u128`.
fn token_u128(value: &TokenValue) -> Option<u128> {
    u128::try_from(value.as_biguint()?).ok()
}

fn token_count(marking: &Marking, place: usize) -> u128 {
    marking
        .get(place)
        .and_then(token_u128)
        .unwrap_or(u128::MAX)
}

/// Close `included` (with the pending `stack`) under D1/D2 at `marking`.
///
/// For an enabled member every transition sharing an input place is added; for
/// a disabled one, a scapegoat input place with the fewest producers and all
/// its producers. The result is stubborn in the weak sense of Schmidt Def. 5
/// (and in Valmari's sense), which is what every caller needs.
pub(crate) fn close_stubborn(
    net: &PetriNet,
    index: &NetIndex,
    marking: &Marking,
    included: &mut Vec<bool>,
    stack: &mut Vec<usize>,
) {
    let add = |transition: usize, included: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !included[transition] {
            included[transition] = true;
            stack.push(transition);
        }
    };
    while let Some(transition) = stack.pop() {
        let id = TransitionId(transition as u32);
        if net.is_enabled(id, marking) {
            // D1: transitions sharing an input place.
            for &(place, _) in net.preset(id) {
                for &other in &index.consumers[place.0 as usize] {
                    add(other, included, stack);
                }
            }
        } else {
            // D2: scapegoat place with the fewest producers.
            let scapegoat = net
                .preset(id)
                .iter()
                .filter(|&&(place, weight)| {
                    token_count(marking, place.0 as usize) < u128::from(weight)
                })
                .min_by_key(|&&(place, _)| index.producers[place.0 as usize].len());
            if let Some(&(place, _)) = scapegoat {
                for &other in &index.producers[place.0 as usize] {
                    add(other, included, stack);
                }
            }
        }
    }
}

/// Stubborn set for `goal` at `marking`; `None` means the goal already holds.
fn goal_stubborn_set(
    net: &PetriNet,
    index: &NetIndex,
    marking: &Marking,
    goal: &ReachGoal,
) -> Option<Vec<bool>> {
    if goal.is_satisfied(marking) {
        return None;
    }
    let count = net.transition_count();
    let mut included = vec![false; count];
    let mut stack: Vec<usize> = Vec::new();
    let add = |transition: usize, included: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !included[transition] {
            included[transition] = true;
            stack.push(transition);
        }
    };

    // Interesting transitions: for each (false) disjunct, the unsatisfied
    // constraint with the fewest interesting transitions.
    for conjunction in &goal.disjuncts {
        let mut best: Option<Vec<usize>> = None;
        for constraint in conjunction {
            let Some(direction) = constraint.needed_direction(marking) else {
                continue;
            };
            let interesting: Vec<usize> = (0..count)
                .filter(|&t| {
                    let effect = index.effect_on(t, constraint);
                    if direction > 0 {
                        effect > 0
                    } else {
                        effect < 0
                    }
                })
                .collect();
            if best.as_ref().map_or(true, |b| interesting.len() < b.len()) {
                best = Some(interesting);
            }
        }
        // A disjunct with no unsatisfied constraint would mean the goal holds.
        for transition in best.unwrap_or_default() {
            add(transition, &mut included, &mut stack);
        }
    }

    close_stubborn(net, &index, marking, &mut included, &mut stack);
    Some(included)
}

/// Search for a marking satisfying `goal`, expanding only goal-stubborn
/// subsets of the enabled transitions when `config.use_stubborn` is set.
///
/// Returns `Unreachable` only when the (reduced) state space was exhausted;
/// exceeding `max_nodes` yields a `LimitExceeded` error, never a verdict.
pub fn search_goal(
    net: &PetriNet,
    goal: &ReachGoal,
    config: GoalSearchConfig,
    context: &AnalysisContext,
) -> Result<GoalSearchReport, ReductionError> {
    if net.has_immediate_transitions() || (config.use_stubborn && net.is_extended()) {
        return Err(ReductionError::NotApplicable {
            name: "STUBBORN_REACHABILITY",
            reason: "goal-directed stubborn sets are proved for plain P/T nets; nets with inhibitor arcs or immediate transitions are not supported",
        });
    }
    let index = NetIndex::new(net);

    let initial = net.initial_marking().clone();
    // Node 0 is the initial marking. parents[i] = (parent index, fired transition).
    let mut markings: Vec<Marking> = vec![initial.clone()];
    let mut parents: Vec<Option<(usize, TransitionId)>> = vec![None];
    let mut seen: HashMap<Marking, usize> = HashMap::from([(initial, 0)]);
    let mut queue: VecDeque<usize> = VecDeque::from([0]);

    let rebuild = |parents: &[Option<(usize, TransitionId)>], mut node: usize| {
        let mut path = Vec::new();
        while let Some((parent, transition)) = parents[node] {
            path.push(transition);
            node = parent;
        }
        path.reverse();
        path
    };

    while let Some(node) = queue.pop_front() {
        context
            .check_cancellation()
            .map_err(|_| ReductionError::Cancelled)?;
        let marking = markings[node].clone();

        if goal.is_satisfied(&marking) {
            return Ok(GoalSearchReport {
                outcome: GoalSearchOutcome::Reachable {
                    path: rebuild(&parents, node),
                    marking,
                },
                visited: markings.len(),
            });
        }

        let stubborn = if config.use_stubborn {
            goal_stubborn_set(net, &index, &marking, goal)
        } else {
            None
        };
        for transition in net.transitions() {
            let id = transition.id;
            if let Some(set) = &stubborn {
                if !set[id.0 as usize] {
                    continue;
                }
            }
            if !net.is_enabled(id, &marking) {
                continue;
            }
            let successor = net.fire(id, &marking);
            if seen.contains_key(&successor) {
                continue;
            }
            if markings.len() >= config.max_nodes {
                return Err(ReductionError::LimitExceeded {
                    limit: config.max_nodes,
                });
            }
            let created = markings.len();
            markings.push(successor.clone());
            parents.push(Some((node, id)));
            seen.insert(successor, created);
            queue.push_back(created);
        }
    }

    Ok(GoalSearchReport {
        outcome: GoalSearchOutcome::Unreachable,
        visited: markings.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::execution::AnalysisConfig;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

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

    /// Random small weighted net; token growth is capped by the search budget.
    fn random_net(rng: &mut Lcg) -> PetriNet {
        let places = 2 + rng.next(3) as usize;
        let transitions = 2 + rng.next(4) as usize;
        let mut builder = PetriNetBuilder::default();
        let ps: Vec<_> = (0..places)
            .map(|i| builder.add_place(format!("p{i}"), None))
            .collect();
        let ts: Vec<_> = (0..transitions)
            .map(|i| builder.add_transition(format!("t{i}"), None))
            .collect();
        for &t in &ts {
            let mut has_input = false;
            for &p in &ps {
                if rng.next(3) == 0 {
                    builder.add_arc(
                        ArcEndpoint::Place(p),
                        ArcEndpoint::Transition(t),
                        1 + rng.next(2) as u32,
                    );
                    has_input = true;
                }
                if rng.next(3) == 0 {
                    builder.add_arc(
                        ArcEndpoint::Transition(t),
                        ArcEndpoint::Place(p),
                        1 + rng.next(2) as u32,
                    );
                }
            }
            if !has_input {
                builder.add_arc(ArcEndpoint::Place(ps[0]), ArcEndpoint::Transition(t), 1);
            }
        }
        let tokens = (0..places)
            .map(|_| TokenValue::Finite(rng.next(3)))
            .collect();
        builder.build(tokens).unwrap()
    }

    fn random_goal(rng: &mut Lcg, places: usize) -> ReachGoal {
        let mut disjuncts = Vec::new();
        for _ in 0..1 + rng.next(2) {
            let mut conjunction = Vec::new();
            for _ in 0..1 + rng.next(2) {
                let mut terms = Vec::new();
                for p in 0..places {
                    if rng.next(2) == 0 {
                        terms.push((PlaceId(p as u32), 1 + rng.next(2) as i64));
                    }
                }
                let terms = if terms.is_empty() {
                    vec![(PlaceId(0), 1)]
                } else {
                    terms
                };
                let op = match rng.next(3) {
                    0 => GoalOp::Ge,
                    1 => GoalOp::Le,
                    _ => GoalOp::Eq,
                };
                conjunction.push(LinearConstraint {
                    terms,
                    op,
                    constant: rng.next(5) as i64,
                });
            }
            disjuncts.push(conjunction);
        }
        ReachGoal { disjuncts }
    }

    fn replay(net: &PetriNet, path: &[TransitionId]) -> Marking {
        let mut marking = net.initial_marking().clone();
        for &t in path {
            assert!(net.is_enabled(t, &marking), "witness fires a disabled transition");
            marking = net.fire(t, &marking);
        }
        marking
    }

    #[test]
    fn stubborn_search_agrees_with_full_search_on_random_nets() {
        let context = AnalysisContext::new(AnalysisConfig::default());
        let mut rng = Lcg(0x5EED);
        let (mut compared, mut reduced_smaller) = (0, 0);
        for _ in 0..600 {
            let net = random_net(&mut rng);
            let goal = random_goal(&mut rng, net.place_count());
            let budget = GoalSearchConfig {
                max_nodes: 3000,
                use_stubborn: false,
            };
            let Ok(full) = search_goal(&net, &goal, budget, &context) else {
                continue; // unbounded sample: no ground truth
            };
            let reduced = search_goal(
                &net,
                &goal,
                GoalSearchConfig {
                    use_stubborn: true,
                    ..budget
                },
                &context,
            )
            .expect("reduced search cannot exceed the full state space");
            compared += 1;
            match (&full.outcome, &reduced.outcome) {
                (GoalSearchOutcome::Unreachable, GoalSearchOutcome::Unreachable) => {}
                (
                    GoalSearchOutcome::Reachable { .. },
                    GoalSearchOutcome::Reachable { path, marking },
                ) => {
                    assert_eq!(&replay(&net, path), marking);
                    // Schmidt Thm 5: the shortest path survives the reduction.
                    let GoalSearchOutcome::Reachable { path: full_path, .. } = &full.outcome
                    else {
                        unreachable!()
                    };
                    assert_eq!(path.len(), full_path.len(), "shortest path length changed for {goal:?}\n{net:?}");
                    assert!(goal.is_satisfied(marking));
                }
                other => panic!("verdict mismatch: {other:?} for goal {goal:?}"),
            }
            if reduced.visited < full.visited {
                reduced_smaller += 1;
            }
        }
        assert!(compared > 200, "too few bounded samples: {compared}");
        assert!(reduced_smaller > 0, "stubborn never reduced anything");
    }

    #[test]
    fn independent_components_are_not_interleaved() {
        // Two independent counters; the goal only involves the first.
        let mut builder = PetriNetBuilder::default();
        let a = builder.add_place("a".into(), None);
        let a2 = builder.add_place("a2".into(), None);
        let b = builder.add_place("b".into(), None);
        let b2 = builder.add_place("b2".into(), None);
        let ta = builder.add_transition("ta".into(), None);
        let tb = builder.add_transition("tb".into(), None);
        builder.add_arc(ArcEndpoint::Place(a), ArcEndpoint::Transition(ta), 1);
        builder.add_arc(ArcEndpoint::Transition(ta), ArcEndpoint::Place(a2), 1);
        builder.add_arc(ArcEndpoint::Place(b), ArcEndpoint::Transition(tb), 1);
        builder.add_arc(ArcEndpoint::Transition(tb), ArcEndpoint::Place(b2), 1);
        let net = builder
            .build(vec![
                TokenValue::Finite(5),
                TokenValue::Finite(0),
                TokenValue::Finite(5),
                TokenValue::Finite(0),
            ])
            .unwrap();
        let goal = ReachGoal::single(LinearConstraint {
            terms: vec![(a2, 1)],
            op: GoalOp::Ge,
            constant: 5,
        });
        let context = AnalysisContext::new(AnalysisConfig::default());
        let run = |use_stubborn| {
            search_goal(
                &net,
                &goal,
                GoalSearchConfig {
                    use_stubborn,
                    ..GoalSearchConfig::default()
                },
                &context,
            )
            .unwrap()
        };
        let full = run(false);
        let reduced = run(true);
        assert!(matches!(reduced.outcome, GoalSearchOutcome::Reachable { .. }));
        assert_eq!(reduced.visited, 6);
        assert!(full.visited > reduced.visited);
    }

    #[test]
    fn extended_nets_are_rejected() {
        let mut builder = PetriNetBuilder::default();
        let p = builder.add_place("p".into(), None);
        let t = builder.add_transition("t".into(), None);
        builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        builder.add_inhibitor_arc(p, t, 1);
        let net = builder.build(vec![TokenValue::Finite(0)]).unwrap();
        let goal = ReachGoal::marking(net.initial_marking()).unwrap();
        let error = search_goal(
            &net,
            &goal,
            GoalSearchConfig::default(),
            &AnalysisContext::new(AnalysisConfig::default()),
        )
        .unwrap_err();
        assert_eq!(error.code(), "REDUCTION_NOT_APPLICABLE");
    }
}
