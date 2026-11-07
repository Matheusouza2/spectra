use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use racon_core::execution::AnalysisContext;
use racon_core::graph::{NodeId, StateSpace};
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::PetriNet;
use racon_ctl::{CTLEngine, CTLEngineImpl, CTLFormula, CTLResult, ComparisonOp};

use crate::error::PropertyError;

// ---------------------------------------------------------------------------
// Data Structures
// ---------------------------------------------------------------------------

/// Definition of a "Bad State" — a marking that violates safety.
#[derive(Debug, Clone)]
pub struct BadStateDefinition {
    pub id: String,
    pub description: String,
    pub predicate: StatePredicate,
}

/// A logical predicate over markings.
#[derive(Debug, Clone)]
pub enum StatePredicate {
    /// Linear constraint: sum( coeff * m[place] ) OP constant
    LinearConstraint {
        terms: Vec<(racon_core::net::PlaceId, BigInt)>,
        op: ComparisonOp,
        constant: BigInt,
    },
    And(Box<StatePredicate>, Box<StatePredicate>),
    Or(Box<StatePredicate>, Box<StatePredicate>),
    Not(Box<StatePredicate>),
}

/// Result of a marking test (algebraic reachability check).
#[derive(Debug, Clone)]
pub enum MarkingTestResult {
    Unreachable {
        reason: UnreachabilityReason,
        certificate: Option<String>,
    },
    PossiblyReachable {
        parikh_vector: Option<Vec<u64>>,
        state_equation_searched_within_bound: bool,
    },
}

#[derive(Debug, Clone)]
pub enum UnreachabilityReason {
    StateEquationInfeasible,
    PInvariantViolation { invariant_index: usize },
}

/// Firing sequence (sequence of transition IDs).
#[derive(Debug, Clone)]
pub struct FiringSequence {
    pub transitions: Vec<racon_core::net::TransitionId>,
    pub length: usize,
}

/// Report of bad state evaluation.
#[derive(Debug, Clone)]
pub struct BadStateReport {
    pub is_safe: bool,
    pub violations: Vec<BadStateViolation>,
}

#[derive(Debug, Clone)]
pub struct BadStateViolation {
    pub definition_id: String,
    pub reached: bool,
    pub witness_path: Option<FiringSequence>,
    pub target_node: Option<NodeId>,
}

/// Result of the INA-style algebraic pre-check for one bad-state formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadStateAlgebraicResult {
    pub definition_id: String,
    pub proven_unreachable: bool,
    pub mathematically_possible: bool,
    pub searched_exhaustively: bool,
    pub certificate: Option<String>,
}

/// Conclusion from the expert system.
#[derive(Debug, Clone)]
pub struct ExpertConclusion {
    pub property: String,
    pub value: String,
    pub evidence: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confidence {
    Definitive,
    Heuristic,
}

/// Report from the expert system.
#[derive(Debug, Clone, Default)]
pub struct ExpertSystemReport {
    pub conclusions: Vec<ExpertConclusion>,
}

// ---------------------------------------------------------------------------
// Property Engine Trait
// ---------------------------------------------------------------------------

pub trait PropertyEngine {
    /// Perform a marking test using algebraic methods.
    fn marking_test(
        &self,
        net: &PetriNet,
        target: &Marking,
        invariants: &[racon_invariants::types::PInvariant],
        context: &AnalysisContext,
    ) -> Result<MarkingTestResult, PropertyError>;

    /// Evaluate bad states over a state space using CTL.
    fn evaluate_bad_states(
        &self,
        net: &PetriNet,
        bad_state_defs: &[BadStateDefinition],
        state_space: &dyn StateSpace,
        context: &AnalysisContext,
    ) -> Result<BadStateReport, PropertyError>;

    /// Run the expert system to infer high-level properties.
    fn infer_properties(
        &self,
        invariants: &racon_invariants::types::InvariantAnalysisResult,
        structural: &racon_structural::topology::StructuralProperties,
        state_space: Option<&dyn StateSpace>,
    ) -> ExpertSystemReport;
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

pub struct PropertyEngineImpl {
    pub ctl_engine: CTLEngineImpl,
}

impl Default for PropertyEngineImpl {
    fn default() -> Self {
        Self {
            ctl_engine: CTLEngineImpl::default(),
        }
    }
}

impl PropertyEngine for PropertyEngineImpl {
    fn marking_test(
        &self,
        net: &PetriNet,
        target: &Marking,
        invariants: &[racon_invariants::types::PInvariant],
        context: &AnalysisContext,
    ) -> Result<MarkingTestResult, PropertyError> {
        // 1. Check P-Invariants first (very fast)
        let m0 = net.initial_marking();
        for (idx, inv) in invariants.iter().enumerate() {
            context.check_cancellation()?;
            let (Some(val_m0), Some(val_target)) = (
                dot_product(&inv.coefficients, m0),
                dot_product(&inv.coefficients, target),
            ) else {
                continue;
            };

            if val_m0 != val_target {
                return Ok(MarkingTestResult::Unreachable {
                    reason: UnreachabilityReason::PInvariantViolation {
                        invariant_index: idx,
                    },
                    certificate: Some(format!(
                        "P-Invariant {} evaluates to {} at m0, but {} at target.",
                        idx, val_m0, val_target
                    )),
                });
            }
        }

        // 2. Try the state equation C*x = target - m0 with a bounded exact
        // integer search. A found vector is valid evidence for the equation,
        // but passing without a vector remains only a necessary-condition
        // result because the equation is not sufficient for reachability.
        let (parikh_vector, searched_within_bound) = solve_state_equation(net, target, context)?;

        Ok(MarkingTestResult::PossiblyReachable {
            parikh_vector,
            state_equation_searched_within_bound: searched_within_bound,
        })
    }

    fn evaluate_bad_states(
        &self,
        _net: &PetriNet,
        bad_state_defs: &[BadStateDefinition],
        state_space: &dyn StateSpace,
        context: &AnalysisContext,
    ) -> Result<BadStateReport, PropertyError> {
        if !state_space.is_exact() {
            return Err(PropertyError::NonExactStateSpace);
        }
        let mut violations = Vec::new();
        let mut is_safe = true;

        for def in bad_state_defs {
            context.check_cancellation()?;

            let ctl_formula = self.predicate_to_ctl(&def.predicate)?;
            let ef_formula = CTLFormula::EF(Box::new(ctl_formula));

            let ctl_result: CTLResult =
                self.ctl_engine
                    .evaluate(&ef_formula, state_space, context)?;

            if ctl_result.satisfied {
                is_safe = false;

                let (witness_path, target_node) = if let Some(ref w) = ctl_result.witness {
                    let ws = FiringSequence {
                        transitions: w.transitions.clone(),
                        length: w.nodes.len().saturating_sub(1),
                    };
                    (Some(ws), w.nodes.last().copied())
                } else {
                    (None, None)
                };

                violations.push(BadStateViolation {
                    definition_id: def.id.clone(),
                    reached: true,
                    witness_path,
                    target_node,
                });
            } else {
                violations.push(BadStateViolation {
                    definition_id: def.id.clone(),
                    reached: false,
                    witness_path: None,
                    target_node: None,
                });
            }
        }

        Ok(BadStateReport {
            is_safe,
            violations,
        })
    }

    fn infer_properties(
        &self,
        invariants: &racon_invariants::types::InvariantAnalysisResult,
        structural: &racon_structural::topology::StructuralProperties,
        state_space: Option<&dyn StateSpace>,
    ) -> ExpertSystemReport {
        let mut report = ExpertSystemReport::default();

        // Rule: CPI => Structurally Bounded
        if invariants.is_cpi {
            report.conclusions.push(ExpertConclusion {
                property: "Boundedness".to_string(),
                value: "True (Structural)".to_string(),
                evidence: "Net is Covered by P-Invariants (CPI).".to_string(),
                confidence: Confidence::Definitive,
            });
        }

        // Rule: Conservative => Bounded
        if invariants.is_conservative {
            report.conclusions.push(ExpertConclusion {
                property: "Boundedness".to_string(),
                value: "True (Conservative)".to_string(),
                evidence: "Net is conservative (strict P-invariant > 0).".to_string(),
                confidence: Confidence::Definitive,
            });
        }

        if let Some(certificate) = &invariants.structural_bounded_by_inequality {
            report.conclusions.push(ExpertConclusion {
                property: "Boundedness".to_string(),
                value: "True (Murata Ay<=0)".to_string(),
                evidence: format!("Positive certificate y={certificate:?} satisfies C^T y <= 0."),
                confidence: Confidence::Definitive,
            });
        }

        // Rule: Structurally bounded + Free Choice => Live if well-formed
        let is_structurally_bounded = invariants.is_cpi
            || invariants.is_conservative
            || invariants.structural_bounded_by_inequality.is_some();
        if is_structurally_bounded && structural.is_free_choice {
            report.conclusions.push(ExpertConclusion {
                property: "Liveness".to_string(),
                value: "Potential (Structural)".to_string(),
                evidence: "Bounded Free-Choice net — may be live.".to_string(),
                confidence: Confidence::Heuristic,
            });
        }

        // Rule: State Space Reversibility via SCC
        if let Some(ss) = state_space.filter(|ss| ss.is_exact()) {
            use racon_scc::dag::SCCDAG;
            use racon_scc::tarjan::SCCEngine;

            let scc_engine = SCCEngine;
            if let Ok(scc_result) = scc_engine.compute_sccs(ss, &context_noop()) {
                let dag = SCCDAG::analyze(&scc_result, ss);
                report.conclusions.push(ExpertConclusion {
                    property: "Reversibility".to_string(),
                    value: dag.is_reversible.to_string(),
                    evidence: "Verified via SCC analysis of the state space.".to_string(),
                    confidence: Confidence::Definitive,
                });

                if dag.has_deadlocks {
                    report.conclusions.push(ExpertConclusion {
                        property: "Deadlock Freedom".to_string(),
                        value: "False".to_string(),
                        evidence: "Deadlock detected via terminal SCC analysis.".to_string(),
                        confidence: Confidence::Definitive,
                    });
                }
            }
        }

        report
    }
}

impl PropertyEngineImpl {
    /// Cross a bad-state predicate with the P-invariant equations before any
    /// state-space exploration. A finite bound for every place is obtained
    /// from non-negative P-invariants; exhaustive enumeration then decides
    /// whether the invariant polytope intersects the bad-state formula.
    pub fn analyze_bad_states_algebraic(
        &self,
        net: &PetriNet,
        definitions: &[BadStateDefinition],
        invariants: &[racon_invariants::types::PInvariant],
        context: &AnalysisContext,
    ) -> Result<Vec<BadStateAlgebraicResult>, PropertyError> {
        let bounds = invariant_bounds(net, invariants);
        let exhaustive = bounds
            .as_ref()
            .map(|bounds| {
                bounds
                    .iter()
                    .try_fold(1usize, |total, bound| {
                        total.checked_mul((*bound as usize).saturating_add(1))
                    })
                    .is_some_and(|count| count <= 2_000_000)
            })
            .unwrap_or(false);

        definitions.iter().map(|definition| {
            context.check_cancellation()?;
            if !exhaustive {
                return Ok(BadStateAlgebraicResult {
                    definition_id: definition.id.clone(),
                    proven_unreachable: false,
                    mathematically_possible: false,
                    searched_exhaustively: false,
                    certificate: Some("P-invariants do not provide a finite tractable bound for every place.".into()),
                });
            }
            let bounds = bounds.as_ref().unwrap();
            let mut tokens = vec![0u64; net.place_count()];
            let found = search_bad_marking(0, bounds, &mut tokens, definition, invariants, net, context)?;
            Ok(BadStateAlgebraicResult {
                definition_id: definition.id.clone(),
                proven_unreachable: !found,
                mathematically_possible: found,
                searched_exhaustively: true,
                certificate: (!found).then(|| "A fórmula do Bad State é inconsistente com as equações dos P-invariantes.".into()),
            })
        }).collect()
    }

    fn predicate_to_ctl(&self, pred: &StatePredicate) -> Result<CTLFormula, PropertyError> {
        match pred {
            StatePredicate::LinearConstraint {
                terms,
                op,
                constant,
            } => {
                let ctl_terms: Vec<racon_ctl::PredicateTerm> = terms
                    .iter()
                    .map(|(place_id, coeff)| {
                        Ok(racon_ctl::PredicateTerm {
                            place_id: *place_id,
                            coefficient: coeff.to_i64().ok_or_else(|| {
                                PropertyError::InvalidPredicate(
                                    "CTL coefficient exceeds i64 boundary".into(),
                                )
                            })?,
                        })
                    })
                    .collect::<Result<_, PropertyError>>()?;

                Ok(CTLFormula::AtomicProposition(racon_ctl::AtomicPredicate {
                    terms: ctl_terms,
                    operator: *op,
                    constant: constant.to_i64().ok_or_else(|| {
                        PropertyError::InvalidPredicate("CTL constant exceeds i64 boundary".into())
                    })?,
                }))
            }
            StatePredicate::And(l, r) => {
                let l = self.predicate_to_ctl(l)?;
                let r = self.predicate_to_ctl(r)?;
                Ok(CTLFormula::And(Box::new(l), Box::new(r)))
            }
            StatePredicate::Or(l, r) => {
                let l = self.predicate_to_ctl(l)?;
                let r = self.predicate_to_ctl(r)?;
                Ok(CTLFormula::Or(Box::new(l), Box::new(r)))
            }
            StatePredicate::Not(inner) => {
                let inner = self.predicate_to_ctl(inner)?;
                Ok(CTLFormula::Not(Box::new(inner)))
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn dot_product(coeffs: &[BigInt], marking: &Marking) -> Option<BigInt> {
    if marking.has_omega() {
        return None;
    }
    coeffs
        .iter()
        .zip(marking.tokens.iter())
        .map(|(c, t)| match t {
            TokenValue::Finite(v) => c * BigInt::from(*v),
            TokenValue::Exact(v) => c * BigInt::from(v.clone()),
            TokenValue::Omega => unreachable!("omega was checked above"),
        })
        .sum::<BigInt>()
        .into()
}

fn invariant_bounds(
    net: &PetriNet,
    invariants: &[racon_invariants::types::PInvariant],
) -> Option<Vec<u64>> {
    let mut bounds: Vec<Option<u64>> = vec![None; net.place_count()];
    for invariant in invariants {
        if invariant.coefficients.len() != net.place_count()
            || invariant.coefficients.iter().any(|c| c < &BigInt::zero())
        {
            continue;
        }
        let Some(constant) = invariant
            .constant_value
            .clone()
            .or_else(|| dot_product(&invariant.coefficients, net.initial_marking()))
        else {
            continue;
        };
        if constant < BigInt::zero() {
            continue;
        }
        for (place, coefficient) in invariant
            .coefficients
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > BigInt::zero())
        {
            let Some(candidate) = (constant.clone() / coefficient).to_u64() else {
                return None;
            };
            bounds[place] = Some(bounds[place].map_or(candidate, |current| current.min(candidate)));
        }
    }
    bounds.into_iter().collect()
}

fn search_bad_marking(
    place: usize,
    bounds: &[u64],
    tokens: &mut [u64],
    definition: &BadStateDefinition,
    invariants: &[racon_invariants::types::PInvariant],
    net: &PetriNet,
    context: &AnalysisContext,
) -> Result<bool, PropertyError> {
    context.check_cancellation()?;
    if place == bounds.len() {
        let marking = Marking::from_finite(tokens);
        let invariant_consistent = invariants.iter().all(|invariant| {
            let Some(expected) = invariant
                .constant_value
                .clone()
                .or_else(|| dot_product(&invariant.coefficients, net.initial_marking()))
            else {
                return false;
            };
            dot_product(&invariant.coefficients, &marking) == Some(expected)
        });
        if !invariant_consistent || !predicate_holds(&definition.predicate, &marking) {
            return Ok(false);
        }
        // The state equation is necessary for reachability. A bounded search
        // can reject this candidate as well; when it cannot be searched, the
        // invariant intersection remains only a mathematical possibility.
        let _state_equation = solve_state_equation(net, &marking, context)?;
        // The state equation is necessary but not sufficient, and the
        // bounded Parikh search is intentionally incomplete. Therefore a
        // candidate that satisfies the invariant intersection remains a
        // possibility unless exact reachability later refutes it.
        return Ok(true);
    }
    for value in 0..=bounds[place] {
        tokens[place] = value;
        if search_bad_marking(
            place + 1,
            bounds,
            tokens,
            definition,
            invariants,
            net,
            context,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn predicate_holds(predicate: &StatePredicate, marking: &Marking) -> bool {
    match predicate {
        StatePredicate::LinearConstraint {
            terms,
            op,
            constant,
        } => {
            if marking.has_omega() {
                return false;
            }
            let value: BigInt = terms
                .iter()
                .map(|(place, coefficient)| {
                    let tokens = match marking.get(place.0 as usize) {
                        Some(TokenValue::Finite(value)) => BigInt::from(*value),
                        Some(TokenValue::Exact(value)) => BigInt::from(value.clone()),
                        Some(TokenValue::Omega) => unreachable!("omega was checked above"),
                        None => BigInt::zero(),
                    };
                    coefficient * tokens
                })
                .sum();
            match op {
                ComparisonOp::Eq => value == *constant,
                ComparisonOp::Neq => value != *constant,
                ComparisonOp::Lt => value < *constant,
                ComparisonOp::Le => value <= *constant,
                ComparisonOp::Gt => value > *constant,
                ComparisonOp::Ge => value >= *constant,
            }
        }
        StatePredicate::And(left, right) => {
            predicate_holds(left, marking) && predicate_holds(right, marking)
        }
        StatePredicate::Or(left, right) => {
            predicate_holds(left, marking) || predicate_holds(right, marking)
        }
        StatePredicate::Not(inner) => !predicate_holds(inner, marking),
    }
}

fn context_noop() -> AnalysisContext {
    AnalysisContext::new(racon_core::execution::AnalysisConfig::default())
}

const PARIKH_SEARCH_LIMIT: u64 = 64;

fn solve_state_equation(
    net: &PetriNet,
    target: &Marking,
    context: &AnalysisContext,
) -> Result<(Option<Vec<u64>>, bool), PropertyError> {
    let places = net.place_count();
    let transitions = net.transition_count();
    if target.len() != places || target.has_omega() || transitions > 8 {
        return Ok((None, false));
    }

    let mut residual: Vec<BigInt> = Vec::with_capacity(places);
    for (initial, wanted) in net
        .initial_marking()
        .tokens
        .iter()
        .zip(target.tokens.iter())
    {
        let (TokenValue::Finite(initial), TokenValue::Finite(wanted)) = (initial, wanted) else {
            return Ok((None, false));
        };
        residual.push(BigInt::from(*wanted) - BigInt::from(*initial));
    }

    let columns: Vec<Vec<BigInt>> = (0..transitions)
        .map(|transition| {
            net.incidence_matrix()
                .column(transition)
                .into_iter()
                .collect()
        })
        .collect();
    let mut solution = vec![0u64; transitions];
    if search_state_equation(0, &columns, &mut residual, &mut solution, context)? {
        Ok((Some(solution), true))
    } else {
        Ok((None, true))
    }
}

fn search_state_equation(
    transition: usize,
    columns: &[Vec<BigInt>],
    residual: &mut [BigInt],
    solution: &mut [u64],
    context: &AnalysisContext,
) -> Result<bool, PropertyError> {
    context.check_cancellation()?;
    if transition == columns.len() {
        return Ok(residual.iter().all(|value| value.is_zero()));
    }

    let column = &columns[transition];
    for count in 0..=PARIKH_SEARCH_LIMIT {
        solution[transition] = count;
        for (value, effect) in residual.iter_mut().zip(column.iter()) {
            *value -= effect * BigInt::from(count);
        }
        if search_state_equation(transition + 1, columns, residual, solution, context)? {
            return Ok(true);
        }
        for (value, effect) in residual.iter_mut().zip(column.iter()) {
            *value += effect * BigInt::from(count);
        }
    }
    solution[transition] = 0;
    Ok(false)
}
