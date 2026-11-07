use racon_core::net::{PetriNet, PlaceId};
use racon_ctl::{AtomicPredicate, CTLFormula, ComparisonOp, PredicateTerm};

use crate::error::DslParseError;
use crate::parser::{DslCompOp, DslExpr, DslQuantifier, DslTemporalOp};

/// Translate a parsed DSL expression into a formal CTLFormula.
pub struct Translator;

impl Translator {
    pub fn translate(dsl: &DslExpr, net: &PetriNet) -> Result<CTLFormula, DslParseError> {
        Self::translate_expr(dsl, net)
    }

    fn translate_expr(dsl: &DslExpr, net: &PetriNet) -> Result<CTLFormula, DslParseError> {
        match dsl {
            DslExpr::True => Ok(CTLFormula::True),
            DslExpr::False => Ok(CTLFormula::False),

            DslExpr::AtomicPlace {
                place_name,
                op,
                value,
            } => Self::translate_atomic(place_name, *op, *value, net),

            DslExpr::PlaceInterval {
                place_name,
                min,
                max,
            } => Self::translate_interval(place_name, *min, *max, net),

            DslExpr::Enabled(names) => Self::translate_enabled(names, net),

            DslExpr::Not(inner) => {
                let inner = Self::translate_expr(inner, net)?;
                Ok(CTLFormula::Not(Box::new(inner)))
            }

            DslExpr::And(l, r) => {
                let l = Self::translate_expr(l, net)?;
                let r = Self::translate_expr(r, net)?;
                Ok(CTLFormula::And(Box::new(l), Box::new(r)))
            }

            DslExpr::Or(l, r) => {
                let l = Self::translate_expr(l, net)?;
                let r = Self::translate_expr(r, net)?;
                Ok(CTLFormula::Or(Box::new(l), Box::new(r)))
            }

            DslExpr::Implies(l, r) => {
                let l = Self::translate_expr(l, net)?;
                let r = Self::translate_expr(r, net)?;
                Ok(CTLFormula::Implies(Box::new(l), Box::new(r)))
            }

            DslExpr::TemporalUnary {
                quantifier,
                temporal_op,
                expr,
            } => {
                let inner = Self::translate_expr(expr, net)?;
                match (quantifier, temporal_op) {
                    (DslQuantifier::Exists, DslTemporalOp::Next) => {
                        Ok(CTLFormula::EX(Box::new(inner)))
                    }
                    (DslQuantifier::ForAll, DslTemporalOp::Next) => {
                        Ok(CTLFormula::AX(Box::new(inner)))
                    }
                    (DslQuantifier::Exists, DslTemporalOp::Eventually) => {
                        Ok(CTLFormula::EF(Box::new(inner)))
                    }
                    (DslQuantifier::ForAll, DslTemporalOp::Eventually) => {
                        Ok(CTLFormula::AF(Box::new(inner)))
                    }
                    (DslQuantifier::Exists, DslTemporalOp::Globally) => {
                        Ok(CTLFormula::EG(Box::new(inner)))
                    }
                    (DslQuantifier::ForAll, DslTemporalOp::Globally) => {
                        Ok(CTLFormula::AG(Box::new(inner)))
                    }
                }
            }

            DslExpr::Until {
                quantifier,
                left,
                right,
            } => {
                let l = Self::translate_expr(left, net)?;
                let r = Self::translate_expr(right, net)?;
                match quantifier {
                    DslQuantifier::Exists => Ok(CTLFormula::EU(Box::new(l), Box::new(r))),
                    DslQuantifier::ForAll => Ok(CTLFormula::AU(Box::new(l), Box::new(r))),
                }
            }

            DslExpr::Before {
                quantifier,
                left,
                right,
            } => {
                // E[f1 B f2] = E[ (not f2) U (f1 and not f2) ]
                // A[f1 B f2] = A[ (not f2) U (f1 and not f2) ]
                let l = Self::translate_expr(left, net)?;
                let r = Self::translate_expr(right, net)?;
                let not_r = CTLFormula::Not(Box::new(r.clone()));
                let l_and_not_r = CTLFormula::And(Box::new(l.clone()), Box::new(not_r.clone()));
                match quantifier {
                    DslQuantifier::Exists => {
                        Ok(CTLFormula::EU(Box::new(not_r), Box::new(l_and_not_r)))
                    }
                    DslQuantifier::ForAll => {
                        Ok(CTLFormula::AU(Box::new(not_r), Box::new(l_and_not_r)))
                    }
                }
            }
        }
    }

    /// `enabled(t)` as the conjunction INA uses for an enabledness predicate:
    /// every input place holds at least the arc weight, and every inhibitor place is
    /// below its threshold. This is enabledness by tokens and inhibitors only: it does not
    /// apply the precedence of immediate transitions.
    fn translate_enabled(names: &[String], net: &PetriNet) -> Result<CTLFormula, DslParseError> {
        let mut alternatives: Vec<CTLFormula> = Vec::new();
        for name in names {
            let transition = net
                .transitions()
                .iter()
                .find(|t| t.name == *name || t.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| DslParseError::UnknownTransition(name.clone()))?;
            let atom = |place_id: PlaceId, operator: ComparisonOp, constant: i64| {
                CTLFormula::AtomicProposition(AtomicPredicate {
                    terms: vec![PredicateTerm { place_id, coefficient: 1 }],
                    operator,
                    constant,
                })
            };
            let mut conditions: Vec<CTLFormula> = net
                .preset(transition.id)
                .iter()
                .map(|&(place, weight)| atom(place, ComparisonOp::Ge, i64::from(weight)))
                .collect();
            conditions.extend(
                net.inhibitors(transition.id)
                    .iter()
                    .map(|&(place, threshold)| atom(place, ComparisonOp::Lt, threshold as i64)),
            );
            let conjunction = conditions
                .into_iter()
                .reduce(|left, right| CTLFormula::And(Box::new(left), Box::new(right)))
                .unwrap_or(CTLFormula::True);
            alternatives.push(conjunction);
        }
        alternatives
            .into_iter()
            .reduce(|left, right| CTLFormula::Or(Box::new(left), Box::new(right)))
            .ok_or_else(|| DslParseError::SemanticError("enabled() needs at least one transition".into()))
    }

    fn resolve_place(name: &str, net: &PetriNet) -> Result<PlaceId, DslParseError> {
        net.places()
            .iter()
            .find(|p| p.name == name || p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
            .ok_or_else(|| DslParseError::UnknownPlace(name.to_string()))
    }

    fn translate_atomic(
        place_name: &str,
        op: Option<DslCompOp>,
        value: Option<u64>,
        net: &PetriNet,
    ) -> Result<CTLFormula, DslParseError> {
        let place_id = Self::resolve_place(place_name, net)?;

        let (coeff, constant, comp_op) = match (op, value) {
            (Some(op), Some(val)) => (1, val as i64, Self::map_op(&op)),
            (None, None) => (1, 0, ComparisonOp::Gt), // "P1" implies P1 > 0
            _ => {
                return Err(DslParseError::InvalidAtomicSyntax(format!(
                    "Invalid syntax for place '{}'",
                    place_name
                )))
            }
        };

        Ok(CTLFormula::AtomicProposition(AtomicPredicate {
            terms: vec![PredicateTerm {
                place_id,
                coefficient: coeff,
            }],
            operator: comp_op,
            constant,
        }))
    }

    fn translate_interval(
        place_name: &str,
        min: u64,
        max: u64,
        net: &PetriNet,
    ) -> Result<CTLFormula, DslParseError> {
        let place_id = Self::resolve_place(place_name, net)?;

        // P between A and B = (P >= A) ∧ (P <= B)
        let ge = CTLFormula::AtomicProposition(AtomicPredicate {
            terms: vec![PredicateTerm {
                place_id,
                coefficient: 1,
            }],
            operator: ComparisonOp::Ge,
            constant: min as i64,
        });

        let le = CTLFormula::AtomicProposition(AtomicPredicate {
            terms: vec![PredicateTerm {
                place_id,
                coefficient: 1,
            }],
            operator: ComparisonOp::Le,
            constant: max as i64,
        });

        Ok(CTLFormula::And(Box::new(ge), Box::new(le)))
    }

    fn map_op(op: &DslCompOp) -> ComparisonOp {
        match op {
            DslCompOp::Eq => ComparisonOp::Eq,
            DslCompOp::Neq => ComparisonOp::Neq,
            DslCompOp::Lt => ComparisonOp::Lt,
            DslCompOp::Le => ComparisonOp::Le,
            DslCompOp::Gt => ComparisonOp::Gt,
            DslCompOp::Ge => ComparisonOp::Ge,
        }
    }
}
