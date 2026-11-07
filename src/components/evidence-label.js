const INVARIANT_LABELS = {
  P_INVARIANTS: 'P-Invariant',
  T_INVARIANTS: 'T-Invariant',
};

export function evidenceTitle(evidence) {
  const label = INVARIANT_LABELS[evidence.property];
  if (label && Number.isInteger(evidence.sequence)) {
    return `${label} #${evidence.sequence + 1}`;
  }

  return evidence.definition?.name || evidence.property;
}
