import React from 'react';
import './AnalysisOverview.css';
import { markingToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

// One verdict per cell: the colour bar and the word carry the result, the dot is redundant on purpose
// so the state never depends on colour alone.
function Verdict({ label, value, pending, settled }) {
  const { t } = useI18n();
  const known = typeof value === 'boolean';
  const tone = known ? (value ? 'good' : 'bad') : pending ? 'pending' : settled ? 'inconclusive' : 'unknown';
  const word = known ? (value ? t('common.yes') : t('common.no')) : pending ? '…' : settled ? t('common.inconclusive') : '—';
  return (
    <div className={`ao-verdict is-${tone}`} aria-busy={pending && !known} title={!known && settled ? t('common.inconclusiveHint') : undefined}>
      <span className="ao-verdict-label">{label}</span>
      <strong className="ao-verdict-word">{word}</strong>
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
  const partial = Boolean(data?.analysis_message) || /limit|cancel|unavailable/.test(data?.analysis_status || '');

  const counts = [
    ['places', data?.net_places],
    ['transitions', data?.net_transitions],
  ];

  return (
    <section className="ao" aria-label={t('overview.ariaLabel')}>
      <div className="ao-lead">
        <h1 className="ao-title">{t('overview.title')}</h1>
        <p className={`ao-status${partial ? ' is-partial' : ''}`}>
          {tm(data?.analysis_message) || status.replaceAll('_', ' ')}
        </p>
      </div>

      <dl className="ao-figures">
        {counts.map(([key, value]) => (
          <div key={key} className="ao-figure">
            <dt>{t(`overview.${key}`)}</dt>
            <dd>{value ?? '—'}</dd>
          </div>
        ))}
        <div className="ao-figure ao-marking">
          <dt>M₀</dt>
          <dd><code>{marking}</code></dd>
        </div>
      </dl>

      <div className="ao-verdicts" role="group" aria-label={t('overview.verdicts')}>
        <Verdict label={t('overview.bounded')} value={status3.bounded} pending={pending} settled={settled} />
        <Verdict label={t('overview.live')} value={status3.live} pending={pending} settled={settled} />
        <Verdict label={t('overview.safe')} value={status3.safe} pending={pending} settled={settled} />
      </div>
    </section>
  );
}
