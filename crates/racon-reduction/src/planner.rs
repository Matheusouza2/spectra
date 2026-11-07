//! Reduction planner.
//!
//! The planner decides whether a set of requested reductions may be combined,
//! in which order they must run, and which compatibility constraints apply.
//! Arbitrary sequences that would change the meaning of a result are rejected
//! instead of being applied silently.

use crate::capability::ReductionKind;
use crate::error::ReductionError;
use crate::variant::VariantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedReduction {
    pub kind: ReductionKind,
    pub order: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReductionPlan {
    pub steps: Vec<PlannedReduction>,
    pub variant: VariantId,
}

impl ReductionPlan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn kinds(&self) -> Vec<ReductionKind> {
        self.steps.iter().map(|step| step.kind).collect()
    }
}

/// Build an ordered, validated plan for the requested reductions.
///
/// Rules:
/// * duplicates are rejected;
/// * `FINKEL_MINIMAL_COVERABILITY` operates on coverability and therefore
///   cannot be combined with the exact state-space reductions;
/// * `SYMMETRY` runs before `STUBBORN` so the stubborn exploration already
///   works on canonical markings.
pub fn plan(requested: &[ReductionKind]) -> Result<ReductionPlan, ReductionError> {
    let mut seen: Vec<ReductionKind> = Vec::new();
    for kind in requested {
        if seen.contains(kind) {
            return Err(ReductionError::Incompatible {
                reason: format!("reduction '{}' requested more than once", kind.as_str()),
            });
        }
        seen.push(*kind);
    }

    let has_finkel = seen.contains(&ReductionKind::FinkelMinimalCoverability);
    let has_exact = seen.contains(&ReductionKind::Symmetry) || seen.contains(&ReductionKind::Stubborn);
    if has_finkel && has_exact {
        return Err(ReductionError::Incompatible {
            reason:
                "FINKEL_MINIMAL_COVERABILITY cannot be combined with SYMMETRY or STUBBORN".to_owned(),
        });
    }

    let mut ordered = seen.clone();
    ordered.sort_by_key(|kind| match kind {
        ReductionKind::Symmetry => 0,
        ReductionKind::Stubborn => 1,
        ReductionKind::FinkelMinimalCoverability => 2,
    });

    let steps = ordered
        .into_iter()
        .enumerate()
        .map(|(order, kind)| PlannedReduction { kind, order })
        .collect();

    Ok(ReductionPlan {
        steps,
        variant: VariantId::for_reductions(&seen),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_plan_is_original_variant() {
        let plan = plan(&[]).unwrap();
        assert!(plan.is_empty());
        assert_eq!(plan.variant, VariantId::Original);
    }

    #[test]
    fn symmetry_runs_before_stubborn() {
        let plan = plan(&[ReductionKind::Stubborn, ReductionKind::Symmetry]).unwrap();
        assert_eq!(
            plan.kinds(),
            vec![ReductionKind::Symmetry, ReductionKind::Stubborn]
        );
        assert_eq!(plan.variant, VariantId::SymmetryStubborn);
    }

    #[test]
    fn duplicate_reductions_are_rejected() {
        let error = plan(&[ReductionKind::Symmetry, ReductionKind::Symmetry]).unwrap_err();
        assert_eq!(error.code(), "REDUCTION_NOT_APPLICABLE");
    }

    #[test]
    fn finkel_cannot_be_combined_with_exact_reductions() {
        let error = plan(&[
            ReductionKind::FinkelMinimalCoverability,
            ReductionKind::Symmetry,
        ])
        .unwrap_err();
        assert!(matches!(error, ReductionError::Incompatible { .. }));
    }
}
