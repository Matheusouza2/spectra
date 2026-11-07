import React from 'react';
import './AnalysisOverview.css';
import { markingToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

function StatusPill({ label, value, pending, settled }) {
  const { t, tm } = useI18n();
  const known = typeof value === 'boolean';
  return (
    <div className={`analysis-status ${known ? (value ? 'is-good' : 'is-bad') : pending ? 'is-pending' : settled ? 'is-inconclusive' : 'is-unknown'}`} aria-busy={pending && !known}>
      <span className="analysis-status-dot" />
      <span>{label}</span>
      <strong title={!known && settled ? t('common.inconclusiveHint') : undefined}>{known ? (value ? t('common.yes') : t('common.no')) : pending ? '…' : settled ? `? ${t('common.inconclusive')}` : '—'}</strong>
    </div>
  );
}

// B / L / S come from the full property computation (requiredProperties) when
// available; the initial structural analysis alone leaves them undecided.
function resolveStatus(data, required) {
  const pick = (a, b) => (typeof a === 'boolean' ? a : typeof b === 'boolean' ? b : null);
  const bounded = pick(required?.bounded, data?.bounded);
  const live = pick(required?.live, data?.live);
  let safe = pick(data?.safe, null);
  if (safe === null) {
    if (bounded === false || required?.live_and_safe === true) safe = required?.live_and_safe === true;
    else if (live === true && typeof required?.live_and_safe === 'boolean') safe = required.live_and_safe;
  }
  return { bounded, live, safe };
}

export default function AnalysisOverview({ data, requiredProperties, pending = false }) {
  const { t, tm } = useI18n();
  const settled = Boolean(requiredProperties) && !pending;
  const status3 = resolveStatus(data, requiredProperties);
  const marking = Array.isArray(data?.initial_marking)
    ? `[${markingToString(data.initial_marking)}]`
    : '—';
  const statusKey = {
    exact_reachability_complete: 'overview.status.exact',
    structural_analysis_complete: 'overview.status.structural',
    state_space_limit_reached: 'overview.status.limit',
    state_space_cancelled: 'overview.status.cancelled',
    state_space_unavailable: 'overview.status.unavailable',
  }[data?.analysis_status];
  const status = statusKey ? t(statusKey) : data?.analysis_status || t('overview.status.available');

  return (
    <section className="analysis-overview" aria-label={t('overview.ariaLabel')}>
      <div className="analysis-overview-heading">
        <div>
          <span className="analysis-eyebrow">{t('overview.eyebrow')}</span>
          <h1>{t('overview.title')}</h1>
          <p className={data?.analysis_message ? 'analysis-overview-message' : undefined}>
            {tm(data?.analysis_message) || status.replaceAll('_', ' ')}
          </p>
        </div>
        <div className="analysis-m0-chip">
          <span>M₀</span>
          <code>{marking}</code>
        </div>
      </div>

      <div className="analysis-kpis">
        <div className="analysis-kpi">
          <span>{t('overview.states')}</span>
          <strong>{data?.analysis_info?.num_states ?? '—'}</strong>
        </div>
        <div className="analysis-kpi">
          <span>{t('overview.places')}</span>
          <strong>{data?.net_places ?? '—'}</strong>
        </div>
        <div className="analysis-kpi">
          <span>{t('overview.transitions')}</span>
          <strong>{data?.net_transitions ?? '—'}</strong>
        </div>
        <div className="analysis-kpi analysis-kpi-status">
          <StatusPill label={t('overview.bounded')} value={status3.bounded} pending={pending} settled={settled} />
          <StatusPill label={t('overview.live')} value={status3.live} pending={pending} settled={settled} />
          <StatusPill label={t('overview.safe')} value={status3.safe} pending={pending} settled={settled} />
        </div>
      </div>
    </section>
  );
}
