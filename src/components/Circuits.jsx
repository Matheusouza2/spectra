import React, { useCallback, useEffect, useMemo, useState } from 'react';
import './Circuits.css';
import CTLTrace from './CTLTrace';
import VectorField from './VectorField';
import { computeCircuits, getNetInfo } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { parseVector } from '../services/vector-input';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

const KINDS = ['shortest', 'all', 'longest'];

function Circuit({ circuit, placeNames, index, t }) {
  const sequence = circuit.steps.slice(1).map((step) => step.transition_name || `t${step.transition}`);
  return (
    <li className="cx-item">
      <details>
        <summary>
          <span className="cx-index">{index + 1}</span>
          <span className="cx-seq">{sequence.join(' → ')}</span>
          <span className="cx-len">{t('verify.circuits.length', { count: circuit.length })}</span>
        </summary>
        <div className="cx-body">
          <p className="cx-meta">{t('verify.circuits.closed')}</p>
          <CTLTrace
            steps={circuit.steps}
            nodes={[...circuit.states, circuit.states[0]].map((state) => Number(state.slice(1)))}
            placeNames={placeNames}
            kind="circuit"
          />
          <h4>{t('verify.circuits.tinv')}</h4>
          <p className="cx-chips">
            {circuit.parikh.map((entry) => (
              <span key={entry.name} className="cx-chip">{entry.name}{entry.count > 1 && <b>×{entry.count}</b>}</span>
            ))}
          </p>
          <p className="cx-flags">
            <span className={circuit.fair ? 'yes' : 'no'}><span aria-hidden="true">{circuit.fair ? '✓' : '✗'}</span> {t('verify.circuits.fair')}</span>
            <span className={circuit.state_fair ? 'yes' : 'no'}><span aria-hidden="true">{circuit.state_fair ? '✓' : '✗'}</span> {t('verify.circuits.stateFair')}</span>
          </p>
          {circuit.not_fired.length > 0 && (
            <details className="cx-notfired">
              <summary>{t('verify.circuits.notFired', { count: circuit.not_fired.length })}</summary>
              <p>{circuit.not_fired.join(', ')}</p>
            </details>
          )}
        </div>
      </details>
    </li>
  );
}

/** INA "compute circuits": elementary circuits of the reachability graph, with bounded search. */
export default function Circuits({ filePath, blocked = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [placeNames, setPlaceNames] = useState([]);
  const [kind, setKind] = useState('shortest');
  const [through, setThrough] = useState('');
  const [maxLength, setMaxLength] = useState(8);
  const [firstOnly, setFirstOnly] = useState(false);
  const [limit, setLimit] = useState(50);
  const [seconds, setSeconds] = useState(5);
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setResult(null);
    setThrough('');
    if (!filePath) return undefined;
    let current = true;
    getNetInfo(filePath).then((info) => { if (current) setPlaceNames(info.place_names || []); }).catch(() => { if (current) setPlaceNames([]); });
    return () => { current = false; };
  }, [filePath]);

  const parsedThrough = useMemo(() => parseVector(through, placeNames.length), [through, placeNames.length]);
  const throughGiven = through.trim() !== '';
  const ready = !throughGiven || parsedThrough.valid;

  const run = useCallback(async () => {
    if (!filePath || loading || blocked || !ready) return;
    setLoading(true);
    setResult(null);
    try {
      setResult(await computeCircuits(filePath, {
        mode: kind,
        through: throughGiven ? parsedThrough.values : null,
        max_length: kind === 'all' ? Math.max(2, Number(maxLength) || 8) : null,
        first_only: firstOnly && kind !== 'longest',
        limit: Math.min(1000, Math.max(1, Number(limit) || 50)),
        time_limit_ms: Math.round((Number(seconds) || 5) * 1000),
      }, loadGraphGenerationConfig()));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, loading, blocked, ready, kind, throughGiven, parsedThrough, maxLength, firstOnly, limit, seconds, addToast, t, tm]);

  if (!filePath) return <p className="cx-empty">{t('verify.selectFile')}</p>;
  if (placeNames.length === 0) return <p className="cx-empty">{t('verify.marking.noPlaces')}</p>;

  const peak = result ? Math.max(1, ...result.length_counts) : 1;
  const summary = (() => {
    if (!result) return null;
    if (result.circuits.length === 0 && result.total_found === 0) {
      return result.exhaustive ? t('verify.circuits.none') : t('verify.circuits.noneYet');
    }
    if (kind === 'shortest') return t('verify.circuits.shortestSummary', { count: result.total_found, length: result.shortest });
    if (kind === 'longest') return t('verify.circuits.longestSummary', { length: result.longest });
    return t('verify.circuits.allSummary', { count: result.total_found });
  })();

  return (
    <div className="cx vf-split">
    <div className="vf-in">
      <fieldset className="cx-kinds">
        <legend>{t('verify.circuits.which')}</legend>
        {KINDS.map((k) => (
          <label key={k} className={`cx-kind ${kind === k ? 'on' : ''}`}>
            <input type="radio" name="cx-kind" value={k} checked={kind === k} onChange={() => { setKind(k); setResult(null); }} />
            <span className="cx-kind-title">{t(`verify.circuits.kind.${k}.title`)}</span>
            <span className="cx-kind-desc">{t(`verify.circuits.kind.${k}.desc`)}</span>
          </label>
        ))}
      </fieldset>

      <VectorField
        label={t('verify.circuits.through')}
        text={through}
        onChange={(v) => { setThrough(v); setResult(null); }}
        onEnter={run}
        names={placeNames}
        emptyHint={t('verify.circuits.throughHint')}
        placeholder={placeNames.map(() => 0).join(', ')}
      />

      <div className="cx-options">
        {kind === 'all' && (
          <label>{t('verify.circuits.maxLength')}
            <input type="number" min="2" value={maxLength} onChange={(e) => setMaxLength(e.target.value)} />
          </label>
        )}
        <label>{t('verify.circuits.limit')}
          <input type="number" min="1" max="1000" value={limit} onChange={(e) => setLimit(e.target.value)} />
        </label>
        <label>{t('verify.circuits.seconds')}
          <input type="number" min="1" max="60" value={seconds} onChange={(e) => setSeconds(e.target.value)} />
        </label>
        {kind !== 'longest' && (
          <label className="cx-check">
            <input type="checkbox" checked={firstOnly} onChange={(e) => setFirstOnly(e.target.checked)} />
            {t('verify.circuits.firstOnly')}
          </label>
        )}
      </div>

      <div className="cx-actions">
        <button type="button" className="cx-run" onClick={run} disabled={blocked || loading || !ready}>
          {loading ? t('verify.circuits.searching') : t('verify.circuits.search')}
        </button>
      </div>
      <p className="cx-note">{t('verify.circuits.budgetNote')}</p>

    </div>
    <div className="vf-out">
      {result && (
        <section className="cx-result" aria-live="polite">
          <p className="cx-summary">{summary}</p>
          {!result.exhaustive && result.total_found > 0 && (
            <p className="cx-warn">{kind === 'longest' ? t('verify.circuits.partialLongest') : firstOnly ? t('verify.circuits.firstOnlyNote') : t('verify.circuits.partial')}</p>
          )}
          {!result.exhaustive && result.total_found === 0 && <p className="cx-warn">{t('verify.circuits.partial')}</p>}
          {kind === 'all' && result.length_counts.some((c) => c > 0) && (
            <ol className="cx-hist" aria-label={t('verify.circuits.byLength')}>
              {result.length_counts.map((count, length) => count > 0 && (
                <li key={length}>
                  <span className="cx-d">{length}</span>
                  <span className="cx-bar" aria-hidden="true"><span style={{ width: `${(count / peak) * 100}%` }} /></span>
                  <span className="cx-n">{count}</span>
                </li>
              ))}
            </ol>
          )}
          {result.circuits.length > 0 && (
            <>
              <h3 className="cx-title">{t('verify.circuits.listed', { count: result.circuits.length })}</h3>
              <ol className="cx-list">
                {result.circuits.map((circuit, i) => <Circuit key={i} circuit={circuit} index={i} placeNames={placeNames} t={t} />)}
              </ol>
            </>
          )}
        </section>
      )}
    </div>
    </div>
  );
}
