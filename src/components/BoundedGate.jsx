import React from 'react';
import './BoundedGate.css';
import { useI18n } from '../i18n/I18nContext';

/** Queries run on the exact reachability graph, so they need a provably bounded net. */
export function queryGate(requiredProperties, loading, hasFile = true) {
  if (!hasFile) return { allowed: false, reason: 'noFile' };
  if (loading || !requiredProperties) return { allowed: false, reason: 'loading' };
  if (requiredProperties.bounded === true) return { allowed: true, reason: null };
  if (requiredProperties.bounded === false) return { allowed: false, reason: 'unbounded' };
  return { allowed: false, reason: 'unknown' };
}

export default function BoundedGate({ gate }) {
  const { t } = useI18n();
  if (!gate || gate.allowed || gate.reason === 'noFile') return null;
  return (
    <div className={`bgate ${gate.reason}`} role="status">
      <strong>{t(`gate.${gate.reason}.title`)}</strong>
      <span>{t(`gate.${gate.reason}.text`)}</span>
    </div>
  );
}
