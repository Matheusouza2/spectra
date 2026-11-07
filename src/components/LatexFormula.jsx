import React from 'react';
import { latexToPresentationText } from '../services/latex';

export default function LatexFormula({ value, className = '' }) {
  const text = latexToPresentationText(value);
  return (
    <span
      className={`latex-formula ${className}`.trim()}
      role="math"
      aria-label={value || ''}
    >
      {text}
    </span>
  );
}
