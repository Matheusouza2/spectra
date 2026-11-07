import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { getNetInfo, checkBadStates } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import ReductionOptions, { useReductionFlag } from './ReductionOptions';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';
import './BadStatesEditor.css';

const OPERATORS = [
  { value: '>=', label: '≥' },
  { value: '>', label: '>' },
  { value: '=', label: '=' },
  { value: '<=', label: '≤' },
  { value: '<', label: '<' },
  { value: '!=', label: '≠' },
];

const STATUS = {
  unreachable_by_p_invariants: { tone: 'good', key: 'badstates.status.safeInvariants' },
  unreachable_exactly: { tone: 'good', key: 'badstates.status.safeExact' },
  reachable: { tone: 'bad', key: 'badstates.status.violated' },
  possibly_reachable: { tone: 'unknown', key: 'badstates.status.inconclusive' },
};

const storageKey = (filePath) => `spectra.bad-states.v1:${filePath}`;
const newTerm = () => ({ coefficient: 1, place: 0 });
let counter = 0;
const newDefinition = () => ({ uid: `bs-${Date.now()}-${counter++}`, description: '', terms: [newTerm()], operator: '>=', constant: 1 });

function loadDefinitions(filePath) {
  try {
    const raw = JSON.parse(localStorage.getItem(storageKey(filePath)) || 'null');
    if (Array.isArray(raw) && raw.length) return raw;
  } catch { /* storage unavailable */ }
  return [];
}

const isInt = (v) => v !== '' && Number.isInteger(Number(v));

function formula(definition, names) {
  const left = definition.terms.map((term, i) => {
    const c = Number(term.coefficient);
    const name = names[term.place] ?? `p${term.place}`;
    const sign = c < 0 ? '−' : i === 0 ? '' : '+';
    const mag = Math.abs(c) === 1 ? '' : `${Math.abs(c)}·`;
    return `${i === 0 ? sign : ` ${sign} `}${mag}${name}`;
  }).join('');
  const op = OPERATORS.find((o) => o.value === definition.operator)?.label ?? definition.operator;
  return `${left} ${op} ${definition.constant}`;
}

export default function BadStatesEditor({ filePath, onResult, extended = false, hasImmediate = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [placeNames, setPlaceNames] = useState([]);
  const [definitions, setDefinitions] = useState([]);
  const [results, setResults] = useState(null);
  const [running, setRunning] = useState(false);
  const [stubborn, setStubborn] = useReductionFlag('badstates', 'stubborn');
  const [structuralRules, setStructuralRules] = useReductionFlag('badstates', 'structural');

  useEffect(() => {
    setResults(null);
    setDefinitions(filePath ? loadDefinitions(filePath) : []);
    onResult?.(null);
    if (!filePath) { setPlaceNames([]); return undefined; }
    let current = true;
    getNetInfo(filePath)
      .then((info) => { if (current) setPlaceNames(info.place_names || []); })
      .catch(() => { if (current) setPlaceNames([]); });
    return () => { current = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath]);

  useEffect(() => {
    if (!filePath) return;
    try { localStorage.setItem(storageKey(filePath), JSON.stringify(definitions)); } catch { /* ignore */ }
  }, [filePath, definitions]);

  const update = useCallback((uid, patch) => {
    setDefinitions((list) => list.map((d) => (d.uid === uid ? { ...d, ...patch } : d)));
    setResults(null);
    onResult?.(null);
  }, [onResult]);

  const updateTerm = (uid, index, patch) => {
    setDefinitions((list) => list.map((d) => (d.uid === uid
      ? { ...d, terms: d.terms.map((term, i) => (i === index ? { ...term, ...patch } : term)) }
      : d)));
    setResults(null);
    onResult?.(null);
  };

  const valid = useMemo(() => definitions.length > 0 && definitions.every((d) =>
    d.terms.length > 0 && isInt(d.constant) && d.terms.every((term) => isInt(term.coefficient) && Number(term.coefficient) !== 0 && term.place < placeNames.length)),
  [definitions, placeNames]);

  const run = async () => {
    if (!valid || running) return;
    setRunning(true);
    try {
      const payload = definitions.map((d, i) => ({
        id: `BS${i + 1}`,
        description: d.description.trim() || formula(d, placeNames),
        terms: d.terms.map((term) => ({ place: Number(term.place), coefficient: Number(term.coefficient) })),
        operator: d.operator,
        constant: Number(d.constant),
      }));
      const response = await checkBadStates(filePath, payload, {
        ...loadGraphGenerationConfig(),
        stubborn: stubborn && !extended,
        structuralRules: structuralRules && !hasImmediate,
      });
      setResults(response);
      onResult?.({ safe: response.safe });
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(String(err)) }), 'error');
    } finally {
      setRunning(false);
    }
  };

  if (!filePath) return null;

  const overall = results
    ? results.safe === true ? 'good' : results.safe === false ? 'bad' : 'unknown'
    : null;

  return (
    <section className="bs-editor panel" aria-label={t('badstates.title')}>
      <div className="panel-header">
        <div>
          <span className="panel-kicker">{t('badstates.kicker')}</span>
          <span className="panel-title">{t('badstates.title')}</span>
        </div>
        {overall && (
          <span className={`bs-overall bs-${overall}`} role="status">
            {overall === 'good' ? t('badstates.overall.safe') : overall === 'bad' ? t('badstates.overall.violated') : t('badstates.overall.inconclusive')}
          </span>
        )}
      </div>

      <div className="bs-body">
        <p className="bs-hint">{t('badstates.hint')}</p>

        <ReductionOptions options={[
          { id: 'stubborn', checked: stubborn, onChange: setStubborn, disabled: extended, disabledReason: t('reduction.plainOnly') },
          { id: 'structural', checked: structuralRules, onChange: setStructuralRules, disabled: hasImmediate, disabledReason: t('reduction.noImmediate') },
        ]} />

        {definitions.length === 0 && <p className="bs-empty">{t('badstates.empty')}</p>}

        <ol className="bs-list">
          {definitions.map((definition, index) => {
            const entry = results?.results?.find((r) => r.id === `BS${index + 1}`);
            const status = entry ? STATUS[entry.status] : null;
            return (
              <li key={definition.uid} className="bs-item">
                <div className="bs-item-head">
                  <span className="bs-id">BS{index + 1}</span>
                  <input
                    className="bs-desc" type="text" value={definition.description}
                    placeholder={t('badstates.descriptionPlaceholder')} aria-label={t('badstates.description')}
                    onChange={(e) => update(definition.uid, { description: e.target.value })}
                  />
                  <button type="button" className="bs-icon-btn" aria-label={t('badstates.remove')} title={t('badstates.remove')}
                    onClick={() => { setDefinitions((list) => list.filter((d) => d.uid !== definition.uid)); setResults(null); onResult?.(null); }}>×</button>
                </div>

                <div className="bs-condition">
                  {definition.terms.map((term, i) => (
                    <span className="bs-term" key={i}>
                      {i > 0 && <span className="bs-plus">+</span>}
                      <input type="number" step="1" className="bs-coef" value={term.coefficient}
                        aria-label={t('badstates.coefficient')}
                        onChange={(e) => updateTerm(definition.uid, i, { coefficient: e.target.value })} />
                      <span className="bs-times">·</span>
                      <select value={term.place} aria-label={t('badstates.place')}
                        onChange={(e) => updateTerm(definition.uid, i, { place: Number(e.target.value) })}>
                        {placeNames.map((name, p) => <option key={p} value={p}>{name}</option>)}
                      </select>
                      {definition.terms.length > 1 && (
                        <button type="button" className="bs-icon-btn bs-term-remove" aria-label={t('badstates.removeTerm')}
                          onClick={() => update(definition.uid, { terms: definition.terms.filter((_, k) => k !== i) })}>×</button>
                      )}
                    </span>
                  ))}
                  <button type="button" className="bs-link-btn"
                    onClick={() => update(definition.uid, { terms: [...definition.terms, newTerm()] })}>+ {t('badstates.addTerm')}</button>
                  <select className="bs-op" value={definition.operator} aria-label={t('badstates.operator')}
                    onChange={(e) => update(definition.uid, { operator: e.target.value })}>
                    {OPERATORS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
                  </select>
                  <input type="number" step="1" className="bs-const" value={definition.constant}
                    aria-label={t('badstates.constant')} onChange={(e) => update(definition.uid, { constant: e.target.value })} />
                </div>

                {isInt(definition.constant) && (
                  <code className="bs-formula">{t('badstates.bad')}: {formula(definition, placeNames)}</code>
                )}

                {status && (
                  <div className={`bs-result bs-${status.tone}`}>
                    <strong title={status.tone === 'unknown' ? t('common.inconclusiveHint') : undefined}>
                      {status.tone === 'unknown' ? '? ' : ''}{t(status.key)}
                    </strong>
                    {entry.certificate && <span>{tm(entry.certificate)}</span>}
                  </div>
                )}
              </li>
            );
          })}
        </ol>

        <div className="bs-actions">
          <button type="button" className="bs-secondary" onClick={() => { setDefinitions((list) => [...list, newDefinition()]); setResults(null); onResult?.(null); }}>
            + {t('badstates.add')}
          </button>
          <button type="button" className="bs-primary" disabled={!valid || running} onClick={run}>
            {running ? t('badstates.running') : t('badstates.run')}
          </button>
        </div>
        {definitions.length > 0 && !valid && <p className="bs-invalid">{t('badstates.invalid')}</p>}
      </div>
    </section>
  );
}
