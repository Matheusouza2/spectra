import React, { useEffect, useMemo, useRef, useState } from 'react';
import './VectorTest.css';
import VectorField from './VectorField';
import { getNetInfo, testVector } from '../services/tauri-api';
import { parseVector } from '../services/vector-input';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

const KINDS = ['place', 'transition'];

const Yes = ({ value, t }) => (
  <span className={`vt-flag ${value ? 'yes' : 'no'}`}>
    <span aria-hidden="true">{value ? '✓' : '✗'}</span> {value ? t('common.yes') : t('common.no')}
  </span>
);

/** INA "test place- or transition-vectors for invariant properties". */
export default function VectorTest({ filePath }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [kind, setKind] = useState('place');
  const [names, setNames] = useState({ place: [], transition: [] });
  const [text, setText] = useState('');
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef(null);

  useEffect(() => {
    setResult(null);
    setText('');
    if (!filePath) return undefined;
    let current = true;
    getNetInfo(filePath)
      .then((info) => { if (current) setNames({ place: info.place_names || [], transition: info.transition_names || [] }); })
      .catch(() => { if (current) setNames({ place: [], transition: [] }); });
    return () => { current = false; };
  }, [filePath]);

  const slots = names[kind];
  const parsed = useMemo(() => parseVector(text, slots.length, { allowNegative: true }), [text, slots.length]);
  if (!filePath || (names.place.length === 0 && names.transition.length === 0)) return null;

  const switchKind = (next) => { setKind(next); setText(''); setResult(null); };
  const run = async () => {
    if (!parsed.valid || loading) return;
    setLoading(true);
    setResult(null);
    try {
      setResult(await testVector(filePath, kind, parsed.values));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  };

  const residual = result
    ? result.residual
      .map((value, i) => ({ value: Number(value), name: result.residual_names[i] || `#${i}` }))
      .filter((entry) => entry.value !== 0)
    : [];
  const prefix = result?.kind === 'transition' ? 'T' : 'P';

  return (
    <section className="vt" aria-labelledby="vt-title">
      <header className="vt-head">
        <h3 id="vt-title">{t('vtest.title')}</h3>
        <p>{t('vtest.subtitle')}</p>
        <div className="vt-switch" role="group" aria-label={t('vtest.kind')}>
          {KINDS.map((k) => (
            <button key={k} type="button" aria-pressed={kind === k} className={kind === k ? 'on' : ''} onClick={() => switchKind(k)}>
              {t(`vtest.kinds.${k}`)}
            </button>
          ))}
        </div>
      </header>

      <VectorField
        label={t(`vtest.label.${kind}`)}
        help={t(`vtest.help.${kind}`)}
        text={text}
        onChange={(value) => { setText(value); setResult(null); }}
        onEnter={run}
        names={slots}
        allowNegative
        placeholder={slots.map((_, i) => (i === 0 ? 1 : 0)).join(', ')}
        inputRef={inputRef}
      />

      <div className="vt-actions">
        <button type="button" className="vt-run" onClick={run} disabled={!parsed.valid || loading}>
          {loading ? t('vtest.testing') : t('vtest.test')}
        </button>
      </div>

      {result && (
        <div className="vt-result" aria-live="polite">
          <dl className="vt-flags">
            <div><dt>{t('vtest.semipositive')}</dt><dd><Yes value={result.semipositive} t={t} /></dd></div>
            <div><dt>{t('vtest.positive')}</dt><dd><Yes value={result.positive} t={t} /></dd></div>
            <div><dt>{t(`vtest.invariant.${prefix}`)}</dt><dd><Yes value={result.invariant} t={t} /></dd></div>
            <div><dt>{t(`vtest.sub.${prefix}`)}</dt><dd><Yes value={result.sub} t={t} /></dd></div>
            <div><dt>{t(`vtest.sur.${prefix}`)}</dt><dd><Yes value={result.sur} t={t} /></dd></div>
          </dl>
          {result.initial_value != null && <p className="vt-note">{t('vtest.initialValue', { value: result.initial_value })}</p>}
          <div className="vt-residual">
            <span className="vt-residual-title">{t(`vtest.residual.${prefix}`)}</span>
            {residual.length === 0 ? (
              <span className="vt-note">{t('vtest.zeroResidual')}</span>
            ) : (
              <ul>
                {residual.map((entry) => (
                  <li key={entry.name} className={entry.value < 0 ? 'neg' : 'pos'}>
                    <span>{entry.name}</span><b>{entry.value > 0 ? `+${entry.value}` : entry.value}</b>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
