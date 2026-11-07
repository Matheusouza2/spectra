use std::collections::VecDeque;
use std::time::Instant;

use fixedbitset::FixedBitSet;
use rayon::prelude::*;

use racon_core::execution::AnalysisContext;
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::TokenValue;

use crate::error::CTLError;
use crate::formula::{
    AtomicPredicate, CTLFormula, CTLResult, ComparisonOp, PathCounterexample, PathWitness,
};

pub trait CTLEngine {
    /// Evaluate a CTL formula over the given state space.
    fn evaluate(
        &self,
        formula: &CTLFormula,
        state_space: &dyn StateSpace,
        context: &AnalysisContext,
    ) -> Result<CTLResult, CTLError>;
}

#[derive(Debug, Clone)]
pub struct CTLConfig {
    pub max_iterations: usize,
    pub extract_witness: bool,
}

impl Default for CTLConfig {
    fn default() -> Self {
        Self {
            max_iterations: 1_000_000,
            extract_witness: true,
        }
    }
}

pub struct CTLEngineImpl {
    pub config: CTLConfig,
}

impl Default for CTLEngineImpl {
    fn default() -> Self {
        Self {
            config: CTLConfig::default(),
        }
    }
}

impl CTLEngine for CTLEngineImpl {
    fn evaluate(
        &self,
        formula: &CTLFormula,
        state_space: &dyn StateSpace,
        context: &AnalysisContext,
    ) -> Result<CTLResult, CTLError> {
        if !state_space.is_exact() {
            return Err(CTLError::NonExactStateSpace);
        }
        let start = Instant::now();
        let n = state_space.node_count();
        let result_set = self.eval(formula, state_space, n, context)?;

        let initial_node = state_space.initial_node();
        let satisfied = result_set.contains(initial_node.0 as usize);

        let satisfying_states: Vec<NodeId> = result_set.ones().map(|i| NodeId(i as u32)).collect();

        let (witness, counterexample) = if self.config.extract_witness {
            self.extract_witness_counterexample(
                formula,
                &result_set,
                satisfied,
                state_space,
                context,
            )?
        } else {
            (None, None)
        };

        Ok(CTLResult {
            formula: formula.clone(),
            satisfied,
            satisfying_states,
            total_states: n,
            evaluation_time_ms: start.elapsed().as_millis() as u64,
            witness,
            counterexample,
        })
    }
}

// ── Core Evaluation ─────────────────────────────────────────────────────────

impl CTLEngineImpl {
    /// CTL structures use a total transition relation. Reachability graphs
    /// may contain a deadlock with no explicit successor, so expose a logical
    /// self-loop without mutating the original graph.
    fn ctl_successors(&self, ss: &dyn StateSpace, node: NodeId) -> Vec<NodeId> {
        let successors = ss.successors(node);
        if successors.is_empty() {
            vec![node]
        } else {
            successors.to_vec()
        }
    }

    fn transitions_for_nodes(
        &self,
        ss: &dyn StateSpace,
        nodes: &[NodeId],
    ) -> Vec<racon_core::net::TransitionId> {
        nodes
            .windows(2)
            .filter_map(|pair| {
                ss.outgoing_edges(pair[0])
                    .into_iter()
                    .find(|(target, _)| *target == pair[1])
                    .map(|(_, transition)| transition)
            })
            .collect()
    }

    fn eval(
        &self,
        formula: &CTLFormula,
        ss: &dyn StateSpace,
        n: usize,
        ctx: &AnalysisContext,
    ) -> Result<FixedBitSet, CTLError> {
        match formula {
            CTLFormula::True => Ok(all_ones_set(n)),

            CTLFormula::False => Ok(FixedBitSet::with_capacity(n)),

            CTLFormula::AtomicProposition(pred) => Ok(self.eval_atomic(pred, ss, n)),

            CTLFormula::Not(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                let mut result = all_ones_set(n);
                result.difference_with(&inner_set);
                Ok(result)
            }

            CTLFormula::And(l, r) => {
                let l_set = self.eval(l, ss, n, ctx)?;
                let r_set = self.eval(r, ss, n, ctx)?;
                Ok(l_set.intersection(&r_set).collect())
            }

            CTLFormula::Or(l, r) => {
                let l_set = self.eval(l, ss, n, ctx)?;
                let r_set = self.eval(r, ss, n, ctx)?;
                Ok(l_set.union(&r_set).collect())
            }

            CTLFormula::Implies(l, r) => {
                let l_set = self.eval(l, ss, n, ctx)?;
                let r_set = self.eval(r, ss, n, ctx)?;
                let mut result = all_ones_set(n);
                result.difference_with(&l_set);
                result.union_with(&r_set);
                Ok(result)
            }

            CTLFormula::EX(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                Ok(self.compute_ex(&inner_set, ss, n))
            }

            CTLFormula::AX(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                Ok(self.compute_ax(&inner_set, ss, n))
            }

            CTLFormula::EF(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                self.compute_ef(&inner_set, ss, n, ctx)
            }

            CTLFormula::AF(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                // AF φ = NOT EG (NOT φ)
                let not_phi = {
                    let mut tmp = all_ones_set(n);
                    tmp.difference_with(&inner_set);
                    tmp
                };
                let eg_not_phi = self.compute_eg(&not_phi, ss, n, ctx)?;
                let mut result = all_ones_set(n);
                result.difference_with(&eg_not_phi);
                Ok(result)
            }

            CTLFormula::EG(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                self.compute_eg(&inner_set, ss, n, ctx)
            }

            CTLFormula::AG(inner) => {
                let inner_set = self.eval(inner, ss, n, ctx)?;
                // AG φ = NOT EF (NOT φ)
                let not_phi = {
                    let mut tmp = all_ones_set(n);
                    tmp.difference_with(&inner_set);
                    tmp
                };
                let ef_not_phi = self.compute_ef(&not_phi, ss, n, ctx)?;
                let mut result = all_ones_set(n);
                result.difference_with(&ef_not_phi);
                Ok(result)
            }

            CTLFormula::EU(l, r) => {
                let l_set = self.eval(l, ss, n, ctx)?;
                let r_set = self.eval(r, ss, n, ctx)?;
                self.compute_eu(&l_set, &r_set, ss, n, ctx)
            }

            CTLFormula::AU(l, r) => {
                let l_set = self.eval(l, ss, n, ctx)?;
                let r_set = self.eval(r, ss, n, ctx)?;
                // A[φ U ψ] = NOT (EG (NOT ψ) ∨ E[(NOT ψ) U (NOT φ ∧ NOT ψ)])
                let not_r = {
                    let mut tmp = all_ones_set(n);
                    tmp.difference_with(&r_set);
                    tmp
                };
                let not_l = {
                    let mut tmp = all_ones_set(n);
                    tmp.difference_with(&l_set);
                    tmp
                };
                // E[(NOT ψ) U (NOT φ ∧ NOT ψ)]
                let not_l_and_not_r = not_l.intersection(&not_r).collect();
                let eu_notr_notlr = self.compute_eu(&not_r, &not_l_and_not_r, ss, n, ctx)?;

                // EG (NOT ψ)
                let eg_notr = self.compute_eg(&not_r, ss, n, ctx)?;

                let mut bad = eg_notr;
                bad.union_with(&eu_notr_notlr);
                let mut result = all_ones_set(n);
                result.difference_with(&bad);
                Ok(result)
            }
        }
    }

    fn eval_atomic(
        &self,
        pred: &AtomicPredicate,
        state_space: &dyn StateSpace,
        n: usize,
    ) -> FixedBitSet {
        let mut satisfied = FixedBitSet::with_capacity(n);
        let nodes: Vec<_> = (0..n).collect();

        let results: Vec<bool> = nodes
            .par_iter()
            .map(|&i| {
                let node_id = NodeId(i as u32);
                let marking = state_space.marking(node_id);

                let sum: i64 = pred
                    .terms
                    .iter()
                    .map(|term| {
                        let tokens = match &marking.tokens[term.place_id.0 as usize] {
                            TokenValue::Finite(c) => *c as i64,
                            TokenValue::Exact(c) => i64::try_from(c).unwrap_or(i64::MAX),
                            TokenValue::Omega => i64::MAX,
                        };
                        term.coefficient * tokens
                    })
                    .sum();

                match pred.operator {
                    ComparisonOp::Eq => sum == pred.constant,
                    ComparisonOp::Neq => sum != pred.constant,
                    ComparisonOp::Lt => sum < pred.constant,
                    ComparisonOp::Le => sum <= pred.constant,
                    ComparisonOp::Gt => sum > pred.constant,
                    ComparisonOp::Ge => sum >= pred.constant,
                }
            })
            .collect();

        for (i, &holds) in results.iter().enumerate() {
            if holds {
                satisfied.insert(i);
            }
        }
        satisfied
    }
}

// ── Pre-image Computation (EX / AX) ─────────────────────────────────────────

impl CTLEngineImpl {
    /// EX φ: States that have at least one successor in φ.
    fn compute_ex(&self, phi: &FixedBitSet, ss: &dyn StateSpace, n: usize) -> FixedBitSet {
        let mut result = FixedBitSet::with_capacity(n);
        let indices: Vec<_> = (0..n).collect();

        let ex_bits: Vec<bool> = indices
            .par_iter()
            .map(|&i| {
                let node_id = NodeId(i as u32);
                self.ctl_successors(ss, node_id)
                    .iter()
                    .any(|&succ| phi.contains(succ.0 as usize))
            })
            .collect();

        for (i, &holds) in ex_bits.iter().enumerate() {
            if holds {
                result.insert(i);
            }
        }
        result
    }

    /// AX φ: States where ALL successors are in φ.
    /// States with no successors vacuously satisfy AX φ.
    fn compute_ax(&self, phi: &FixedBitSet, ss: &dyn StateSpace, n: usize) -> FixedBitSet {
        let mut result = FixedBitSet::with_capacity(n);
        let indices: Vec<_> = (0..n).collect();

        let ax_bits: Vec<bool> = indices
            .par_iter()
            .map(|&i| {
                let node_id = NodeId(i as u32);
                self.ctl_successors(ss, node_id)
                    .iter()
                    .all(|&succ| phi.contains(succ.0 as usize))
            })
            .collect();

        for (i, &holds) in ax_bits.iter().enumerate() {
            if holds {
                result.insert(i);
            }
        }
        result
    }
}

// ── Least Fixed Point (EF, EU) ──────────────────────────────────────────────

impl CTLEngineImpl {
    /// EF φ = μZ . (φ ∨ EX Z)
    fn compute_ef(
        &self,
        phi: &FixedBitSet,
        ss: &dyn StateSpace,
        n: usize,
        ctx: &AnalysisContext,
    ) -> Result<FixedBitSet, CTLError> {
        let mut z = phi.clone();
        for _ in 0..self.config.max_iterations {
            ctx.check_cancellation()?;
            let ex_z = self.compute_ex(&z, ss, n);
            let z_new = z.union(&ex_z).collect();
            if z_new == z {
                return Ok(z);
            }
            z = z_new;
        }
        Err(CTLError::FixedPointConvergenceFailed {
            limit: self.config.max_iterations,
        })
    }

    /// E[φ1 U φ2] = μZ . (φ2 ∨ (φ1 ∧ EX Z))
    fn compute_eu(
        &self,
        phi1: &FixedBitSet,
        phi2: &FixedBitSet,
        ss: &dyn StateSpace,
        n: usize,
        ctx: &AnalysisContext,
    ) -> Result<FixedBitSet, CTLError> {
        let mut z = phi2.clone();
        for _ in 0..self.config.max_iterations {
            ctx.check_cancellation()?;
            let ex_z = self.compute_ex(&z, ss, n);
            let phi1_and_ex_z: FixedBitSet = phi1.intersection(&ex_z).collect();
            let z_new = z.union(&phi1_and_ex_z).collect();
            if z_new == z {
                return Ok(z);
            }
            z = z_new;
        }
        Err(CTLError::FixedPointConvergenceFailed {
            limit: self.config.max_iterations,
        })
    }
}

// ── Greatest Fixed Point (EG) ────────────────────────────────────────────────

impl CTLEngineImpl {
    /// EG φ = νZ . (φ ∧ EX Z)
    /// Iterative greatest fixed-point over (φ ∧ EX Z) starting from top (φ).
    fn compute_eg(
        &self,
        phi: &FixedBitSet,
        ss: &dyn StateSpace,
        n: usize,
        ctx: &AnalysisContext,
    ) -> Result<FixedBitSet, CTLError> {
        let mut z = phi.clone();
        for _ in 0..self.config.max_iterations {
            ctx.check_cancellation()?;
            let ex_z = self.compute_ex(&z, ss, n);
            let phi_and_ex_z: FixedBitSet = phi.intersection(&ex_z).collect();
            if phi_and_ex_z == z {
                return Ok(z);
            }
            z = phi_and_ex_z;
        }
        Err(CTLError::FixedPointConvergenceFailed {
            limit: self.config.max_iterations,
        })
    }
}

// ── Witness Extraction ───────────────────────────────────────────────────────

impl CTLEngineImpl {
    fn extract_witness_counterexample(
        &self,
        formula: &CTLFormula,
        result_set: &FixedBitSet,
        satisfied: bool,
        ss: &dyn StateSpace,
        ctx: &AnalysisContext,
    ) -> Result<(Option<PathWitness>, Option<PathCounterexample>), CTLError> {
        let init = ss.initial_node();

        if satisfied {
            // Extract witnesses for existential path formulas and EX.
            match formula {
                cf @ CTLFormula::EF(_) | cf @ CTLFormula::EU(_, _) => {
                    let target_set = if matches!(cf, CTLFormula::EF(_)) {
                        let inner = match cf {
                            CTLFormula::EF(i) => i,
                            _ => unreachable!(),
                        };
                        self.eval(inner, ss, ss.node_count(), ctx)?
                    } else {
                        // EU: target is φ2
                        let inner = match cf {
                            CTLFormula::EU(_, r) => r,
                            _ => unreachable!(),
                        };
                        self.eval(inner, ss, ss.node_count(), ctx)?
                    };

                    if let Some(witness) = self.extract_ef_witness(init, &target_set, ss, ctx) {
                        return Ok((Some(witness), None));
                    }
                }
                CTLFormula::EG(_) => {
                    if let Some(witness) = self.extract_eg_witness(init, result_set, ss, ctx) {
                        return Ok((Some(witness), None));
                    }
                }
                CTLFormula::EX(inner) => {
                    let target_set = self.eval(inner, ss, ss.node_count(), ctx)?;
                    if let Some(&target) = self
                        .ctl_successors(ss, init)
                        .iter()
                        .find(|node| target_set.contains(node.0 as usize))
                    {
                        let nodes = if target == init {
                            vec![init]
                        } else {
                            vec![init, target]
                        };
                        return Ok((
                            Some(PathWitness {
                                description: "Immediate successor satisfying EX".into(),
                                transitions: self.transitions_for_nodes(ss, &nodes),
                                nodes,
                                is_infinite: false,
                                cycle_start: None,
                            }),
                            None,
                        ));
                    }
                }
                _ => {}
            }
        } else {
            // Extract counterexamples for universal path formulas.
            match formula {
                CTLFormula::AG(_) => {
                    // AG φ failed → EF ¬φ succeeded, find path to ¬φ
                    let inner = match formula {
                        CTLFormula::AG(i) => i,
                        _ => unreachable!(),
                    };
                    let not_phi_set = {
                        let inner_set = self.eval(inner, ss, ss.node_count(), ctx)?;
                        let mut tmp = all_ones_set(ss.node_count());
                        tmp.difference_with(&inner_set);
                        tmp
                    };

                    if let Some(witness) = self.extract_ef_witness(init, &not_phi_set, ss, ctx) {
                        let ce = PathCounterexample {
                            description: "Path to a state violating AG".to_string(),
                            nodes: witness.nodes.clone(),
                            transitions: witness.transitions,
                            violation_point: witness.nodes.last().copied().unwrap_or(init),
                            is_infinite: false,
                        };
                        return Ok((None, Some(ce)));
                    }
                }
                CTLFormula::AF(inner) => {
                    let inner_set = self.eval(inner, ss, ss.node_count(), ctx)?;
                    let mut not_inner = all_ones_set(ss.node_count());
                    not_inner.difference_with(&inner_set);
                    if let Some(witness) = self.extract_eg_witness(init, &not_inner, ss, ctx) {
                        return Ok((
                            None,
                            Some(PathCounterexample {
                                description: "Infinite path that never reaches AF target".into(),
                                violation_point: witness.nodes.last().copied().unwrap_or(init),
                                nodes: witness.nodes,
                                transitions: witness.transitions,
                                is_infinite: true,
                            }),
                        ));
                    }
                }
                CTLFormula::AX(inner) => {
                    let inner_set = self.eval(inner, ss, ss.node_count(), ctx)?;
                    if let Some(&target) = self
                        .ctl_successors(ss, init)
                        .iter()
                        .find(|node| !inner_set.contains(node.0 as usize))
                    {
                        let nodes = if target == init {
                            vec![init]
                        } else {
                            vec![init, target]
                        };
                        return Ok((
                            None,
                            Some(PathCounterexample {
                                description: "Immediate successor violating AX".into(),
                                violation_point: target,
                                transitions: self.transitions_for_nodes(ss, &nodes),
                                nodes,
                                is_infinite: false,
                            }),
                        ));
                    }
                }
                CTLFormula::AU(left, right) => {
                    let left_set = self.eval(left, ss, ss.node_count(), ctx)?;
                    let right_set = self.eval(right, ss, ss.node_count(), ctx)?;
                    let mut not_right = all_ones_set(ss.node_count());
                    not_right.difference_with(&right_set);
                    if let Some(witness) = self.extract_eg_witness(init, &not_right, ss, ctx) {
                        return Ok((
                            None,
                            Some(PathCounterexample {
                                description: "Infinite path that never reaches AU right side"
                                    .into(),
                                violation_point: witness.nodes.last().copied().unwrap_or(init),
                                nodes: witness.nodes,
                                transitions: witness.transitions,
                                is_infinite: true,
                            }),
                        ));
                    }
                    let mut not_left_and_not_right = all_ones_set(ss.node_count());
                    not_left_and_not_right.difference_with(&left_set);
                    not_left_and_not_right.intersect_with(&not_right);
                    if let Some(witness) =
                        self.extract_eu_witness(init, &not_right, &not_left_and_not_right, ss)
                    {
                        return Ok((
                            None,
                            Some(PathCounterexample {
                                description: "Finite path violating AU until condition".into(),
                                violation_point: witness.nodes.last().copied().unwrap_or(init),
                                nodes: witness.nodes,
                                transitions: witness.transitions,
                                is_infinite: false,
                            }),
                        ));
                    }
                }
                _ => {}
            }
        }

        Ok((None, None))
    }

    /// BFS from start to any state in target_set.
    fn extract_ef_witness(
        &self,
        start: NodeId,
        target_set: &FixedBitSet,
        ss: &dyn StateSpace,
        _ctx: &AnalysisContext,
    ) -> Option<PathWitness> {
        let n = ss.node_count();
        let mut visited = vec![false; n];
        let mut parent = vec![None; n];
        let mut queue = VecDeque::new();

        let start_idx = start.0 as usize;
        visited[start_idx] = true;
        queue.push_back(start_idx);

        while let Some(u) = queue.pop_front() {
            if target_set.contains(u) {
                // Reconstruct path
                let mut nodes_rev = vec![NodeId(u as u32)];
                let mut cur = u;
                while let Some(p) = parent[cur] {
                    nodes_rev.push(NodeId(p as u32));
                    cur = p;
                }
                nodes_rev.reverse();

                let transitions = self.transitions_for_nodes(ss, &nodes_rev);
                return Some(PathWitness {
                    description: "Path to target state".to_string(),
                    nodes: nodes_rev,
                    transitions,
                    is_infinite: false,
                    cycle_start: None,
                });
            }

            let node = NodeId(u as u32);
            for succ in self.ctl_successors(ss, node) {
                let si = succ.0 as usize;
                if !visited[si] {
                    visited[si] = true;
                    parent[si] = Some(u);
                    queue.push_back(si);
                }
            }
        }
        None
    }

    fn extract_eu_witness(
        &self,
        start: NodeId,
        left_set: &FixedBitSet,
        target_set: &FixedBitSet,
        ss: &dyn StateSpace,
    ) -> Option<PathWitness> {
        let n = ss.node_count();
        let mut visited = vec![false; n];
        let mut parent = vec![None; n];
        let mut queue = VecDeque::from([start.0 as usize]);
        visited[start.0 as usize] = true;

        while let Some(current) = queue.pop_front() {
            if target_set.contains(current) {
                let mut nodes = vec![NodeId(current as u32)];
                let mut cursor = current;
                while let Some(previous) = parent[cursor] {
                    nodes.push(NodeId(previous as u32));
                    cursor = previous;
                }
                nodes.reverse();
                return Some(PathWitness {
                    description: "Path satisfying the until counterexample prefix".into(),
                    transitions: self.transitions_for_nodes(ss, &nodes),
                    nodes,
                    is_infinite: false,
                    cycle_start: None,
                });
            }
            if !left_set.contains(current) {
                continue;
            }
            for successor in self.ctl_successors(ss, NodeId(current as u32)) {
                let next = successor.0 as usize;
                if !visited[next] {
                    visited[next] = true;
                    parent[next] = Some(current);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// For EG: find a path that enters a self-sustaining cycle in φ.
    fn extract_eg_witness(
        &self,
        start: NodeId,
        phi_set: &FixedBitSet,
        ss: &dyn StateSpace,
        _ctx: &AnalysisContext,
    ) -> Option<PathWitness> {
        let n = ss.node_count();
        // BFS to find a cycle within φ
        let mut visited = vec![false; n];
        let mut parent = vec![None; n];
        let mut entry_time = vec![0u32; n];
        let mut queue = VecDeque::new();
        let start_idx = start.0 as usize;
        visited[start_idx] = true;
        entry_time[start_idx] = 0;
        queue.push_back(start_idx);

        while let Some(u) = queue.pop_front() {
            let node = NodeId(u as u32);
            let time = entry_time[u] + 1;

            for succ in self.ctl_successors(ss, node) {
                let si = succ.0 as usize;
                if !phi_set.contains(si) {
                    continue;
                }
                if !visited[si] {
                    visited[si] = true;
                    parent[si] = Some(u);
                    entry_time[si] = time;
                    queue.push_back(si);
                } else {
                    // Found a cycle (already visited and in φ)
                    // Reconstruct path
                    let mut nodes_rev = vec![succ];
                    let mut cur = u;
                    while let Some(p) = parent[cur] {
                        nodes_rev.push(NodeId(p as u32));
                        cur = p;
                    }
                    nodes_rev.reverse();
                    let cycle_start = nodes_rev.len();

                    // Continue path to find the cycle entry
                    nodes_rev.push(succ);
                    nodes_rev.push(NodeId(u as u32));

                    let transitions = self.transitions_for_nodes(ss, &nodes_rev);
                    return Some(PathWitness {
                        description: "Path with cycle in φ".to_string(),
                        nodes: nodes_rev,
                        transitions,
                        is_infinite: true,
                        cycle_start: Some(cycle_start),
                    });
                }
            }
        }
        None
    }
}

/// Create a FixedBitSet with all bits set to 1.
fn all_ones_set(n: usize) -> FixedBitSet {
    let mut set = FixedBitSet::with_capacity(n);
    set.insert_range(..);
    set
}
