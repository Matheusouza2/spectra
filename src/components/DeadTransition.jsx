import React, { useEffect, useState } from 'react';
import './PlaceBoundedness.css';
import { checkTransitionDead, getNetInfo } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

/** Is a transition dead? Decided with stubborn sets, which also yields a shortest enabling sequence. */
export default function DeadTransition({ filePath, extended = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [names, setNames] = useState([]);
  const [transition, setTransition] = useState(0);
  const [result, setResult] = useState(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    setResult(null);
    setTransition(0);
    if (!filePath) { setNames([]); return undefined; }
    let current = true;
    getNetInfo(filePath)
      .then((info) => { if (current) setNames(info.transition_names || []); })
      .catch(() => { if (current) setNames([]); });
    return () => { current = false; };
  }, [filePath]);

  const run = async () => {
    if (running || extended) return;
    setRunning(true);
    setResult(null);
    try {
      setResult(await checkTransitionDead(filePath, Number(transition), loadGraphGenerationConfig()));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(String(err)) }), 'error');
    } finally {
      setRunning(false);
    }
  };

  if (!filePath) return <p className="pb-empty">{t('verify.selectFile')}</p>;
  if (names.length === 0) return <p className="pb-empty">{t('verify.deadTransition.none')}</p>;

  const tone = result ? (result.dead === true ? 'missing' : result.dead === false ? 'found' : 'unknown') : null;
  return (
    <div className="pb">
      <p className="pb-note">{t('verify.deadTransition.hint')}</p>
      <div className="pb-actions">
        <label className="pb-field">
          <span>{t('verify.deadTransition.transition')}</span>
          <select value={transition} onChange={(e) => { setTransition(Number(e.target.value)); setResult(null); }}>
            {names.map((name, i) => <option key={i} value={i}>{name}</option>)}
          </select>
        </label>
        <button type="button" className="pb-run" onClick={run} disabled={running || extended}>
          {running ? t('verify.deadTransition.checking') : t('verify.deadTransition.check')}
        </button>
      </div>
      {extended && <p className="pb-note">{t('reduction.plainOnly')}</p>}
      {result && (
        <section className={`pb-result ${tone}`} aria-live="polite">
          <strong>
            {result.dead === true && t('verify.deadTransition.dead', { transition: result.transition })}
            {result.dead === false && t('verify.deadTransition.live', { transition: result.transition })}
            {result.dead === null && t('verify.deadTransition.unknown', { transition: result.transition })}
          </strong>
          {result.dead === false && (
            <p>{result.path.length === 0
              ? t('verify.deadTransition.enabledInitially')
              : t('verify.deadTransition.witness', { count: result.path.length, path: result.path.join(' → ') })}</p>
          )}
          {result.dead === true && <p>{t('verify.deadTransition.deadDesc', { count: result.visited })}</p>}
          {result.dead === null && <p>{t('verify.deadTransition.unknownDesc', { count: result.visited })}</p>}
        </section>
      )}
    </div>
  );
}
