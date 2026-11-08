import React, { useCallback, useEffect, useMemo, useState } from 'react';
import './Distances.css';
import CTLTrace from './CTLTrace';
import VectorField from './VectorField';
import { computeDistances, getNetInfo } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { markingToNumbers } from '../services/token-value';
import { parseVector } from '../services/vector-input';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

/** INA "compute distances": firings needed to go from a source marking to every state, or to one target. */
export default function Distances({ filePath, blocked = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [placeNames, setPlaceNames] = useState([]);
  const [initial, setInitial] = useState([]);
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setResult(null);
    setTo('');
    if (!filePath) return undefined;
    let current = true;
    getNetInfo(filePath)
      .then((info) => {
        if (!current) return;
        const values = markingToNumbers(info.initial_marking || []);
        setPlaceNames(info.place_names || []);
        setInitial(values);
        setFrom(values.join(', '));
      })
      .catch(() => { if (current) { setPlaceNames([]); setFrom(''); } });
    return () => { current = false; };
  }, [filePath]);

  const parsedFrom = useMemo(() => parseVector(from, placeNames.length), [from, placeNames.length]);
  const parsedTo = useMemo(() => parseVector(to, placeNames.length), [to, placeNames.length]);
  const toGiven = to.trim() !== '';
  const ready = parsedFrom.valid && (!toGiven || parsedTo.valid);

  const run = useCallback(async () => {
    if (!filePath || loading || blocked || !ready) return;
    setLoading(true);
    setResult(null);
    try {
      // The source defaults to M0 on the backend; sending it explicitly keeps the screen honest.
      setResult(await computeDistances(filePath, parsedFrom.values, toGiven ? parsedTo.values : null, loadGraphGenerationConfig()));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, loading, blocked, ready, parsedFrom, parsedTo, toGiven, addToast, t, tm]);

  if (!filePath) return <p className="ds-empty">{t('verify.selectFile')}</p>;
  if (placeNames.length === 0) return <p className="ds-empty">{t('verify.marking.noPlaces')}</p>;

  const reset = (setter, value) => { setter(value); setResult(null); };
  const peak = result ? Math.max(...result.histogram) : 1;

  return (
    <div className="ds vf-split">
    <div className="vf-in">
      <VectorField
        label={t('verify.distances.from')}
        help={t('verify.distances.fromHelp')}
        text={from}
        onChange={(v) => reset(setFrom, v)}
        onEnter={run}
        names={placeNames}
      />
      <VectorField
        label={t('verify.distances.to')}
        text={to}
        onChange={(v) => reset(setTo, v)}
        onEnter={run}
        names={placeNames}
        emptyHint={t('verify.distances.toHint')}
        placeholder={placeNames.map(() => 0).join(', ')}
      />

      <div className="ds-actions">
        <button type="button" className="ds-run" onClick={run} disabled={blocked || loading || !ready}>
          {loading ? t('verify.distances.computing') : t('verify.distances.compute')}
        </button>
        <button type="button" className="ds-secondary" onClick={() => { reset(setFrom, initial.join(', ')); reset(setTo, ''); }}>
          {t('verify.marking.useInitial')}
        </button>
      </div>

    </div>
    <div className="vf-out">
      {result && (
        <section className="ds-result" aria-live="polite">
          <dl className="ds-stats">
            <div><dt>{t('verify.distances.states')}</dt><dd>{result.state_count}</dd></div>
            <div><dt>{t('verify.distances.eccentricity')}</dt><dd>{result.eccentricity}</dd></div>
            <div>
              <dt>{t('verify.distances.diameter')}</dt>
              <dd>{result.diameter ?? '—'}</dd>
            </div>
            <div>
              <dt>{t('verify.distances.connected')}</dt>
              <dd>{result.strongly_connected == null ? '—' : result.strongly_connected ? t('common.yes') : t('common.no')}</dd>
            </div>
          </dl>
          <p className="ds-note">{t('verify.distances.explain')}</p>
          {result.unreachable > 0 && <p className="ds-note">{t('verify.distances.unreachable', { count: result.unreachable })}</p>}
          {result.diameter == null && <p className="ds-note">{t('verify.distances.diameterSkipped')}</p>}

          <h3 className="ds-title">{t('verify.distances.histogram')}</h3>
          <ol className="ds-hist">
            {result.histogram.map((count, d) => (
              <li key={d}>
                <span className="ds-d">{d}</span>
                <span className="ds-bar" aria-hidden="true"><span style={{ width: `${(count / peak) * 100}%` }} /></span>
                <span className="ds-n">{count}</span>
              </li>
            ))}
          </ol>

          {result.target && (
            <div className={`ds-target ${result.target.distance == null ? 'missing' : 'found'}`}>
              <strong>
                {result.target.distance == null
                  ? t('verify.distances.noPath')
                  : t('verify.distances.pathLength', { count: result.target.distance })}
              </strong>
              {result.target.distance != null && result.target.path.length > 1 && (
                <CTLTrace steps={result.target.path} placeNames={placeNames} kind="witness" />
              )}
            </div>
          )}
        </section>
      )}
    </div>
    </div>
  );
}
