import React, { useEffect, useState } from 'react';
import './PlaceBoundedness.css';
import { checkPlaceBoundedness, getNetInfo } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

/** Boundedness of one place, decided without the full reachability graph (stubborn-set Karp-Miller tree). */
export default function PlaceBoundedness({ filePath, extended = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [placeNames, setPlaceNames] = useState([]);
  const [place, setPlace] = useState(0);
  const [result, setResult] = useState(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    setResult(null);
    setPlace(0);
    if (!filePath) { setPlaceNames([]); return undefined; }
    let current = true;
    getNetInfo(filePath)
      .then((info) => { if (current) setPlaceNames(info.place_names || []); })
      .catch(() => { if (current) setPlaceNames([]); });
    return () => { current = false; };
  }, [filePath]);

  const run = async () => {
    if (running || extended) return;
    setRunning(true);
    setResult(null);
    try {
      setResult(await checkPlaceBoundedness(filePath, Number(place), loadGraphGenerationConfig()));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(String(err)) }), 'error');
    } finally {
      setRunning(false);
    }
  };

  if (!filePath) return <p className="pb-empty">{t('verify.selectFile')}</p>;
  if (placeNames.length === 0) return <p className="pb-empty">{t('verify.marking.noPlaces')}</p>;

  const tone = result ? (result.bounded === true ? 'found' : result.bounded === false ? 'missing' : 'unknown') : null;
  return (
    <div className="pb">
      <p className="pb-note">{t('verify.placeBound.hint')}</p>
      <div className="pb-actions">
        <label className="pb-field">
          <span>{t('verify.placeBound.place')}</span>
          <select value={place} onChange={(e) => { setPlace(Number(e.target.value)); setResult(null); }}>
            {placeNames.map((name, i) => <option key={i} value={i}>{name}</option>)}
          </select>
        </label>
        <button type="button" className="pb-run" onClick={run} disabled={running || extended}>
          {running ? t('verify.placeBound.checking') : t('verify.placeBound.check')}
        </button>
      </div>
      {extended && <p className="pb-note">{t('reduction.plainOnly')}</p>}
      {result && (
        <section className={`pb-result ${tone}`} aria-live="polite">
          <strong>
            {result.bounded === true && t('verify.placeBound.bounded', { place: result.place })}
            {result.bounded === false && t('verify.placeBound.unbounded', { place: result.place })}
            {result.bounded === null && t('verify.placeBound.unknown', { place: result.place })}
          </strong>
          <p>{result.bounded === null ? t('verify.placeBound.unknownDesc', { count: result.visited }) : t('verify.placeBound.nodes', { count: result.visited })}</p>
        </section>
      )}
    </div>
  );
}
