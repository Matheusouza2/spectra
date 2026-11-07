//! Variant identifiers for net derivations.
//!
//! The original net is never destroyed. Every derived analysis artifact is
//! labelled with the combination of reductions that produced it.

use crate::capability::ReductionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VariantId {
    #[default]
    Original,
    Symmetry,
    Stubborn,
    SymmetryStubborn,
}

impl VariantId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Symmetry => "symmetry",
            Self::Stubborn => "stubborn",
            Self::SymmetryStubborn => "symmetry-stubborn",
        }
    }

    /// Derive the variant label from the requested reductions.
    pub fn for_reductions(kinds: &[ReductionKind]) -> Self {
        let symmetry = kinds.contains(&ReductionKind::Symmetry);
        let stubborn = kinds.contains(&ReductionKind::Stubborn);
        match (symmetry, stubborn) {
            (true, true) => Self::SymmetryStubborn,
            (true, false) => Self::Symmetry,
            (false, true) => Self::Stubborn,
            (false, false) => Self::Original,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_labels_match_requested_reductions() {
        assert_eq!(VariantId::for_reductions(&[]), VariantId::Original);
        assert_eq!(
            VariantId::for_reductions(&[ReductionKind::Symmetry]),
            VariantId::Symmetry
        );
        assert_eq!(
            VariantId::for_reductions(&[ReductionKind::Stubborn]),
            VariantId::Stubborn
        );
        assert_eq!(
            VariantId::for_reductions(&[ReductionKind::Symmetry, ReductionKind::Stubborn]),
            VariantId::SymmetryStubborn
        );
    }
}
