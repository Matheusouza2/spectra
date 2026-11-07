import React from 'react';
import { useI18n } from '../i18n/I18nContext';
import CTLTrace from './CTLTrace';
import { prettyCtl } from '../services/ctl-format';

const CTLResult = ({ result, loading }) => {
  const { t } = useI18n();

  if (!result) {
    return (
      <section className="ctl-result ctl-result-empty" aria-live="polite">
        <p>{t('ctl.noResult')}</p>
      </section>
    );
  }

  const ev = result.witness || result.counterexample;
  const total = Number(result.total_states) || 0;
  const pct = total > 0 ? (Number(result.count) / total) * 100 : 0;
  const verdict = result.satisfied ? 'sat' : 'unsat';

  return (
    <section className={`ctl-result ${verdict} ${loading ? 'stale' : ''}`} aria-live="polite" aria-label={t('ctl.result')}>
      <div className="ctl-verdict">
        <span className="ctl-verdict-mark" aria-hidden="true">{result.satisfied ? '✓' : '✗'}</span>
        <span className="ctl-verdict-text">{result.satisfied ? t('ctl.satisfied') : t('ctl.notSatisfied')}</span>
        <span className="ctl-verdict-time">{Number(result.time).toFixed(2)} ms</span>
      </div>

      <code className="ctl-result-formula">{prettyCtl(result.formula)}</code>

      <div className="ctl-coverage">
        <div className="ctl-coverage-head">
          <span>{t('ctl.satisfiedStates')}</span>
          <strong>{result.count} <span className="of">/ {total}</span></strong>
        </div>
        <div className="ctl-coverage-track" role="img" aria-label={`${result.count} / ${total}`}>
          <div className="ctl-coverage-fill" style={{ width: `${pct}%` }} />
        </div>
      </div>

      {ev && (
        <div className="ctl-evidence">
          <h3>{result.witness ? t('ctl.witness') : t('ctl.counterexample')}</h3>
          {ev.description && <p>{ev.description}</p>}
          {ev.steps?.length > 0 && (
            <CTLTrace
              steps={ev.steps}
              nodes={ev.nodes}
              placeNames={ev.place_names}
              kind={result.witness ? 'witness' : 'counterexample'}
              infinite={ev.is_infinite}
            />
          )}
          {!(ev.steps?.length > 0) && (
          <ol className="ctl-trace" aria-label={t('ctl.witness')}>
              {ev.nodes.map((node, i) => (
                <li key={i}>
                  <span className="ctl-trace-node">M{node}</span>
                  {i < ev.transitions.length && <span className="ctl-trace-fire" title={t('ctl.firings')}>t{ev.transitions[i]}</span>}
                </li>
              ))}
            </ol>
          )}
        </div>
      )}
    </section>
  );
};

export default CTLResult;
