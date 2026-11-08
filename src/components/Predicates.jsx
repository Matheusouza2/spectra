import React, { useCallback, useEffect, useMemo, useState } from 'react';
import './Predicates.css';
import { buildPredicate, getNetInfo } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { parseStateList, predicateToDsl, reachabilityQuery } from '../services/predicate-dsl';
import { dslFor } from '../locales/dsl';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * INA "define an enabledness predicate" (E) and "convert a set of states to a predicate" (C).
 * A predicate is a disjunction of conjunctions of place bounds.
 */
export default function Predicates({ filePath, blocked = false, onUseInCtl }) {
  const { t, tm, language } = useI18n();
  const { addToast } = useToast();
  const dsl = useMemo(() => dslFor(language), [language]);
  const [transitionNames, setTransitionNames] = useState([]);
  const [chosen, setChosen] = useState(() => new Set());
  const [filter, setFilter] = useState('');
  const [stateText, setStateText] = useState('');
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setResult(null);
    setChosen(new Set());
    setStateText('');
    if (!filePath) return undefined;
    let current = true;
    getNetInfo(filePath).then((info) => { if (current) setTransitionNames(info.transition_names || []); }).catch(() => { if (current) setTransitionNames([]); });
    return () => { current = false; };
  }, [filePath]);

  const states = useMemo(() => parseStateList(stateText), [stateText]);
  const visible = transitionNames.filter((name) => name.toLowerCase().includes(filter.trim().toLowerCase()));

  const build = useCallback(async (kind) => {
    if (!filePath || loading) return;
    setLoading(true);
    setResult(null);
    try {
      const picked = transitionNames.filter((name) => chosen.has(name));
      const response = await buildPredicate(filePath, kind, kind === 'enabled' ? picked : [], kind === 'states' ? (states || []) : [], loadGraphGenerationConfig());
      // The compact form is what the CTL editor accepts as an atom for enabledness.
      const atom = kind === 'enabled' ? `enabled(${picked.join(', ')})` : null;
      setResult({ kind, ...response, dsl: atom ?? predicateToDsl(response.conjunctions, dsl) });
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, loading, transitionNames, chosen, states, dsl, addToast, t, tm]);

  if (!filePath) return <p className="pr-empty">{t('verify.selectFile')}</p>;

  const toggle = (name) => setChosen((prev) => { const next = new Set(prev); if (next.has(name)) next.delete(name); else next.add(name); return next; });
  const copy = async (text) => addToast(await copyText(text) ? t('verify.predicates.copied') : t('verify.predicates.copyFailed'), 'info');
  const statesReady = states !== null && states.length > 0;
  const useQuery = result ? reachabilityQuery(result.kind === 'enabled' ? result.dsl : result.dsl, dsl) : '';

  return (
    <div className="pr vf-split">
    <div className="vf-in">
      <section className="pr-card" aria-labelledby="pr-e">
        <h3 id="pr-e">{t('verify.predicates.enabled.title')}</h3>
        <p className="pr-desc">{t('verify.predicates.enabled.desc')}</p>
        <input
          className="pr-filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t('verify.predicates.enabled.filter')}
          aria-label={t('verify.predicates.enabled.filter')}
          spellCheck={false}
        />
        <ul className="pr-transitions" aria-label={t('verify.predicates.enabled.title')}>
          {visible.map((name) => (
            <li key={name}>
              <label className={chosen.has(name) ? 'on' : ''}>
                <input type="checkbox" checked={chosen.has(name)} onChange={() => toggle(name)} />
                <span>{name}</span>
              </label>
            </li>
          ))}
        </ul>
        <button type="button" className="pr-run" onClick={() => build('enabled')} disabled={loading || chosen.size === 0}>
          {t('verify.predicates.build')}
        </button>
      </section>

      <section className="pr-card" aria-labelledby="pr-c">
        <h3 id="pr-c">{t('verify.predicates.states.title')}</h3>
        <p className="pr-desc">{t('verify.predicates.states.desc')}</p>
        <label className="pr-label" htmlFor="pr-states">{t('verify.predicates.states.label')}</label>
        <input
          id="pr-states"
          className={`pr-input ${states === null ? 'invalid' : ''}`}
          value={stateText}
          onChange={(e) => setStateText(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && statesReady && !blocked) build('states'); }}
          placeholder="0, 3, 5-8"
          spellCheck={false}
          aria-invalid={states === null}
        />
        <p className={`pr-status ${states === null ? 'error' : ''}`}>
          {states === null ? t('verify.predicates.states.invalid') : statesReady ? t('verify.predicates.states.count', { count: states.length }) : t('verify.predicates.states.hint')}
        </p>
        <button type="button" className="pr-run" onClick={() => build('states')} disabled={blocked || loading || !statesReady}>
          {t('verify.predicates.build')}
        </button>
        {blocked && <p className="pr-status">{t('verify.predicates.states.needsGraph')}</p>}
      </section>

    </div>
    <div className="vf-out">
      {result && (
        <section className="pr-result" aria-live="polite">
          <h3>{t('verify.predicates.result')}</h3>
          <p className="pr-desc">{result.kind === 'enabled' ? t('verify.predicates.result.enabled') : t('verify.predicates.result.states', { count: result.conjunctions.length })}</p>

          <h4>{t('verify.predicates.dsl')}</h4>
          <pre className="pr-code">{result.dsl}</pre>
          <div className="pr-actions">
            <button type="button" className="pr-run" onClick={() => onUseInCtl?.(useQuery)}>{t('verify.predicates.useInCtl')}</button>
            <button type="button" className="pr-secondary" onClick={() => copy(result.dsl)}>{t('verify.predicates.copyDsl')}</button>
          </div>
        </section>
      )}
    </div>
    </div>
  );
}
