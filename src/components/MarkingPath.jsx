import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './MarkingPath.css';
import CTLTrace from './CTLTrace';
import VectorField from './VectorField';
import ReductionOptions, { useReductionFlag } from './ReductionOptions';
import { checkMarkingCoverability, checkMarkingReachability, getNetInfo, testMarkingAlgebraic } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { markingToNumbers, tokenValueToString } from '../services/token-value';
import { parseVector } from '../services/vector-input';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

/** Weighted terms such as 2·IR + IR_BUFFER, from a coefficient vector. */
function Terms({ weights, names }) {
  const terms = weights
    .map((w, i) => ({ w: Number(w), name: names[i] || `#${i}` }))
    .filter((term) => term.w !== 0);
  return (
    <span className="mp-terms">
      {terms.map((term, k) => (
        <span key={term.name} className="mp-term">
          {(k > 0 || term.w < 0) && <span className="mp-op">{term.w < 0 ? '−' : '+'}</span>}
          <span className="mp-chip">{Math.abs(term.w) !== 1 && <b>{Math.abs(term.w)}</b>}{term.name}</span>
        </span>
      ))}
    </span>
  );
}

function AlgebraicResult({ result, t }) {
  const unreachable = result.verdict === 'unreachable';
  const counts = (result.parikh || [])
    .map((count, i) => ({ count: Number(count), name: result.transition_names[i] || `t${i}` }))
    .filter((entry) => entry.count > 0);
  return (
    <section className={`mp-result ${unreachable ? 'missing' : 'neutral'}`} aria-live="polite">
      <div className="mp-verdict">
        <span aria-hidden="true">{unreachable ? '✗' : '?'}</span>
        <strong>{unreachable ? t('verify.algebraic.unreachable') : t('verify.algebraic.possible')}</strong>
      </div>
      {unreachable && result.method === 'p_invariant' && (
        <>
          <p className="mp-meta">{t('verify.algebraic.byInvariant')}</p>
          <p className="mp-equation"><Terms weights={result.invariant} names={result.place_names} /></p>
          <p className="mp-meta">{t('verify.algebraic.invariantValues', { initial: result.invariant_initial, target: result.invariant_target })}</p>
        </>
      )}
      {unreachable && result.method === 'state_equation' && (
        <>
          <p className="mp-meta">{t('verify.algebraic.byStateEquation')}</p>
          <p className="mp-meta">{t('verify.algebraic.farkas')}</p>
          <p className="mp-equation"><Terms weights={result.farkas} names={result.place_names} /></p>
        </>
      )}
      {!unreachable && (
        <>
          <p className="mp-meta">{t('verify.algebraic.possibleDesc')}</p>
          {result.parikh && (
            <p className="mp-meta">
              <span>{t('verify.algebraic.parikh')}</span>{' '}
              {counts.length === 0
                ? <em>{t('verify.algebraic.noFirings')}</em>
                : counts.map((entry) => <span key={entry.name} className="mp-chip mp-firing">{entry.name}<b>×{entry.count}</b></span>)}
            </p>
          )}
        </>
      )}
    </section>
  );
}

export default function MarkingPath({ filePath, blocked = false, extended = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [placeNames, setPlaceNames] = useState([]);
  const [initial, setInitial] = useState([]);
  const [text, setText] = useState('');
  const [result, setResult] = useState(null);
  const [algebraic, setAlgebraic] = useState(null);
  const [loading, setLoading] = useState(false);
  const [algebraicLoading, setAlgebraicLoading] = useState(false);
  const [coverage, setCoverage] = useState(null);
  const [coverageLoading, setCoverageLoading] = useState(false);
  const [infoLoading, setInfoLoading] = useState(false);
  const inputRef = useRef(null);

  useEffect(() => {
    setResult(null);
    setAlgebraic(null);
    setCoverage(null);
    if (!filePath) return undefined;
    let current = true;
    setInfoLoading(true);
    getNetInfo(filePath)
      .then((info) => {
        if (!current) return;
        const values = markingToNumbers(info.initial_marking || []);
        setPlaceNames(info.place_names || []);
        setInitial(values);
        setText(values.join(', '));
      })
      .catch(() => { if (current) { setPlaceNames([]); setInitial([]); setText(''); } })
      .finally(() => { if (current) setInfoLoading(false); });
    return () => { current = false; };
  }, [filePath]);

  const [stubborn, setStubborn] = useReductionFlag('marking', 'stubborn');
  // The stubborn search is bounded by the node limit, so it may also run on nets the gate rejects.
  const graphBlocked = blocked && !(stubborn && !extended);
  const parsed = useMemo(() => parseVector(text, placeNames.length), [text, placeNames.length]);
  const edit = (value) => { setText(value); setResult(null); setAlgebraic(null); setCoverage(null); };

  const runGraph = useCallback(async () => {
    if (!filePath || loading || graphBlocked || !parsed.valid) return;
    setLoading(true);
    setResult(null);
    try {
      setResult(await checkMarkingReachability(filePath, parsed.values, { ...loadGraphGenerationConfig(), stubborn: stubborn && !extended }));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, loading, graphBlocked, parsed, stubborn, extended, addToast, t, tm]);

  const runAlgebraic = useCallback(async () => {
    if (!filePath || algebraicLoading || !parsed.valid) return;
    setAlgebraicLoading(true);
    setAlgebraic(null);
    try {
      setAlgebraic(await testMarkingAlgebraic(filePath, parsed.values));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setAlgebraicLoading(false);
    }
  }, [filePath, algebraicLoading, parsed, addToast, t, tm]);

  const runCoverage = useCallback(async () => {
    if (!filePath || coverageLoading || extended || !parsed.valid) return;
    setCoverageLoading(true);
    setCoverage(null);
    try {
      setCoverage(await checkMarkingCoverability(filePath, parsed.values, loadGraphGenerationConfig()));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setCoverageLoading(false);
    }
  }, [filePath, coverageLoading, extended, parsed, addToast, t, tm]);

  if (!filePath) return <p className="mp-empty">{t('verify.selectFile')}</p>;
  if (infoLoading) return <p className="mp-empty" role="status">{t('verify.marking.loading')}</p>;
  if (placeNames.length === 0) return <p className="mp-empty">{t('verify.marking.noPlaces')}</p>;

  const nameOf = (i) => placeNames[i] || `p${i}`;
  const lastStep = Array.isArray(result?.path) && result.path.length > 0 ? result.path[result.path.length - 1] : null;
  const closest = lastStep ? lastStep.marking.map((token, i) => `${nameOf(i)}: ${tokenValueToString(token)}`).join(', ') : null;

  return (
    <div className="mp">
      <VectorField
        label={t('verify.marking.label')}
        help={t('verify.marking.help')}
        text={text}
        onChange={edit}
        onEnter={graphBlocked ? runAlgebraic : runGraph}
        names={placeNames}
        placeholder={t('verify.marking.placeholder', { example: placeNames.map((_, i) => (i === 0 ? 1 : 0)).join(', ') })}
        inputRef={inputRef}
      />

      <ReductionOptions options={[
        { id: 'stubborn', checked: stubborn, onChange: setStubborn, disabled: extended, disabledReason: t('reduction.plainOnly') },
      ]} />

      <div className="mp-actions">
        <button type="button" className="mp-run" onClick={runGraph} disabled={graphBlocked || loading || !parsed.valid}>
          {loading ? t('verify.marking.checking') : t('verify.marking.check')}
        </button>
        <button type="button" className="mp-secondary strong" onClick={runAlgebraic} disabled={algebraicLoading || !parsed.valid}>
          {algebraicLoading ? t('verify.algebraic.testing') : t('verify.algebraic.test')}
        </button>
        <button type="button" className="mp-secondary strong" onClick={runCoverage} disabled={extended || coverageLoading || !parsed.valid} title={extended ? t('verify.coverage.unsupported') : undefined}>
          {coverageLoading ? t('verify.coverage.testing') : t('verify.coverage.test')}
        </button>
        <button type="button" className="mp-secondary" onClick={() => { edit(initial.join(', ')); inputRef.current?.focus(); }}>
          {t('verify.marking.useInitial')}
        </button>
      </div>
      <p className="mp-note">{graphBlocked ? t('verify.algebraic.onlyAlgebraic') : t('verify.algebraic.hint')}</p>

      {extended && <p className="mp-note">{t('verify.coverage.unsupported')}</p>}
      {algebraic && <AlgebraicResult result={algebraic} t={t} />}

      {coverage && (
        <section className={`mp-result ${coverage.covered ? 'found' : 'missing'}`} aria-live="polite">
          <div className="mp-verdict">
            <span aria-hidden="true">{coverage.covered ? '✓' : '✗'}</span>
            <strong>{coverage.covered ? t('verify.coverage.covered') : t('verify.coverage.notCovered')}</strong>
          </div>
          <p className="mp-meta">{coverage.covered ? t('verify.coverage.coveredDesc') : t('verify.coverage.notCoveredDesc')}</p>
          {coverage.bounded === false && <p className="mp-meta">{t('verify.coverage.unbounded')}</p>}
          {coverage.covered && coverage.path.length > 1 && (
            <>
              <p className="mp-meta">
                {coverage.path_has_omega ? t('verify.coverage.omegaPath') : t('verify.coverage.exactPath')}
              </p>
              <CTLTrace steps={coverage.path} placeNames={placeNames} kind="witness" />
            </>
          )}
          {coverage.covered && coverage.path.length === 1 && <p className="mp-meta">{t('verify.coverage.initialCovers')}</p>}
        </section>
      )}

      {result && (
        <section className={`mp-result ${result.reachable ? 'found' : 'missing'}`} aria-live="polite">
          <div className="mp-verdict">
            <span aria-hidden="true">{result.reachable ? '✓' : '✗'}</span>
            <strong>{result.reachable ? t('verify.marking.reachable') : t('verify.marking.notReachable')}</strong>
          </div>
          {result.reachable ? (
            result.path?.length > 1 ? (
              <>
                <p className="mp-meta">{result.path.length - 1 === 1 ? t('verify.marking.firingsOne') : t('verify.marking.firings', { count: result.path.length - 1 })}</p>
                <CTLTrace steps={result.path} placeNames={placeNames} kind="witness" />
              </>
            ) : (
              <p className="mp-meta">{t('verify.marking.alreadyInitial')}</p>
            )
          ) : (
            <>
              <p className="mp-meta">{t('verify.marking.notReachableDesc')}</p>
              {closest && <p className="mp-meta"><span>{t('verify.marking.lastChecked')}</span> <code>{closest}</code></p>}
            </>
          )}
        </section>
      )}
    </div>
  );
}
