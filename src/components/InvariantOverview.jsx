import React from 'react';
import './InvariantOverview.css';
import { markingToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

const GLYPH = { positive: '✓', negative: '✗' };

const Metric = ({ label, value, tone = '' }) => (
  <div className="invariant-metric">
    <dt>{label}</dt>
    <dd className={tone}>{GLYPH[tone] && <span className="glyph" aria-hidden="true">{GLYPH[tone]}</span>}{value}</dd>
  </div>
);

export default function InvariantOverview({ analysisData, invariant }) {
  const { t } = useI18n();
  const yesNo = (v) => (typeof v === 'boolean' ? t(v ? 'common.yes' : 'common.no') : '—');
  const inv = invariant || analysisData?.invariant || {};
  const pCount = inv.minimal_p_semiflows?.length ?? '—';
  const tCount = inv.minimal_t_semiflows?.length ?? '—';
  const marking = Array.isArray(analysisData?.initial_marking)
    ? `[${markingToString(analysisData.initial_marking)}]`
    : '—';
  const tone = (v) => (v === true ? 'positive' : v === false ? 'negative' : '');

  return (
    <section className="invariant-overview" aria-label={t('invOverview.ariaLabel')}>
      <div className="invariant-overview-title">
        <h1>{t('invOverview.title')}</h1>
        <p>{t('invOverview.subtitle')}</p>
      </div>
      <dl className="invariant-metrics">
        <Metric label={t('invOverview.pInvariants')} value={pCount} />
        <Metric label={t('invOverview.tInvariants')} value={tCount} />
        <Metric label={t('invOverview.conservative')} value={yesNo(inv.conservative)} tone={tone(inv.conservative)} />
        <Metric label={t('invOverview.structurallyBounded')} value={yesNo(inv.structurally_bounded)} tone={tone(inv.structurally_bounded)} />
        <Metric label="M₀" value={marking} tone="mono" />
      </dl>
    </section>
  );
}
