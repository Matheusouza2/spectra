import React, { useEffect, useState } from 'react';
import './AdvancedMethods.css';
import { runAdvancedAnalysis } from '../services/tauri-api';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';

const METHODS = [
  { id: 'symbolic_mdd', params: ['limit'] },
  { id: 'symbolic_bdd', params: ['limit'] },
  { id: 'unfolding_prefix', params: ['limit', 'order'] },
  { id: 'structural_rules', params: ['goal', 'support'] },
  { id: 'polyhedral_abstraction', params: [] },
];

/** Renders a summary value: scalars inline, short lists joined, the rest as formatted JSON. */
function Value({ value }) {
  if (value === null || value === undefined) return <span>—</span>;
  if (typeof value === 'boolean') return <span>{value ? '✓' : '✗'}</span>;
  if (typeof value !== 'object') return <span>{String(value)}</span>;
  if (Array.isArray(value) && value.every((v) => typeof v !== 'object') && value.length <= 24) {
    return <span>{value.length === 0 ? '∅' : value.join(', ')}</span>;
  }
  return <pre className="am-json">{JSON.stringify(value, null, 1)}</pre>;
}

/** Methods with their own result shape: symbolic state spaces, unfolding and reductions (coverability sets live in the Graphs tab). */
export default function AdvancedMethods({ filePath, placeNames = [], extended = false }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [method, setMethod] = useState(METHODS[0].id);
  const [limit, setLimit] = useState('');
  const [order, setOrder] = useState('erv');
  const [goal, setGoal] = useState('deadlock');
  const [support, setSupport] = useState([]);
  const [result, setResult] = useState(null);
  const [running, setRunning] = useState(false);

  useEffect(() => { setResult(null); setSupport([]); }, [filePath]);

  if (!filePath) return <p className="am-note">{t('advanced.selectFile')}</p>;
  const spec = METHODS.find((m) => m.id === method);

  const run = async () => {
    if (running || extended) return;
    setRunning(true);
    setResult(null);
    const params = {};
    if (spec.params.includes('limit') && Number(limit) > 0) params.limit = Number(limit);
    if (spec.params.includes('order')) params.order = order;
    if (spec.params.includes('goal')) {
      params.goal = goal;
      if (goal === 'safety') params.support = support;
    }
    try {
      setResult(await runAdvancedAnalysis(filePath, method, params));
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(String(err)) }), 'error');
    } finally {
      setRunning(false);
    }
  };

  const summary = result?.summary ?? {};
  return (
    <div className="am">
      <p className="am-note">{t('advanced.hint')}</p>
      {extended ? <p className="am-note am-blocked" role="alert">{t('advanced.extendedBlocked')}</p> : (
      <div className="am-form">
        <label className="am-field">
          <span>{t('advanced.method')}</span>
          <select value={method} onChange={(e) => { setMethod(e.target.value); setResult(null); }}>
            {METHODS.map((m) => <option key={m.id} value={m.id}>{t(`advanced.methods.${m.id}.name`)}</option>)}
          </select>
        </label>
        {spec.params.includes('limit') && (
          <label className="am-field">
            <span>{t('advanced.limit')}</span>
            <input type="number" min="1" placeholder={t('advanced.limitDefault')} value={limit} onChange={(e) => setLimit(e.target.value)} />
          </label>
        )}
        {spec.params.includes('order') && (
          <label className="am-field">
            <span>{t('advanced.order')}</span>
            <select value={order} onChange={(e) => setOrder(e.target.value)}>
              <option value="erv">ERV</option>
              <option value="mcmillan">McMillan</option>
            </select>
          </label>
        )}
        {spec.params.includes('goal') && (
          <label className="am-field">
            <span>{t('advanced.goal')}</span>
            <select value={goal} onChange={(e) => setGoal(e.target.value)}>
              <option value="deadlock">{t('advanced.goalDeadlock')}</option>
              <option value="safety">{t('advanced.goalSafety')}</option>
            </select>
          </label>
        )}
        {spec.params.includes('support') && goal === 'safety' && (
          <label className="am-field">
            <span>{t('advanced.support')}</span>
            <select multiple size={Math.min(5, Math.max(2, placeNames.length))} value={support}
              onChange={(e) => setSupport(Array.from(e.target.selectedOptions, (o) => o.value))}>
              {placeNames.map((name) => <option key={name} value={name}>{name}</option>)}
            </select>
          </label>
        )}
        <button type="button" className="am-run" onClick={run} disabled={running || extended || (goal === 'safety' && spec.params.includes('support') && support.length === 0)}>
          {running ? t('advanced.running') : t('advanced.run')}
        </button>
      </div>
      )}
      <p className="am-note">{t(`advanced.methods.${method}.when`)}</p>
      {result && (
        <section className="am-result" aria-live="polite">
          <h3>{result.name}</h3>
          <dl>
            {Object.entries(summary).map(([key, value]) => (
              <div key={key} className="am-row">
                <dt>{t(`advanced.fields.${key}`) === `advanced.fields.${key}` ? key : t(`advanced.fields.${key}`)}</dt>
                <dd><Value value={value} /></dd>
              </div>
            ))}
          </dl>
          <p className="am-meta">
            {t('advanced.elapsed', { ms: result.elapsed_ms })}
            {result.reference_ids?.length > 0 && ` · ${result.reference_ids.join(', ')}`}
          </p>
          {result.does_not_preserve?.length > 0 && (
            <p className="am-meta">{t('advanced.doesNotPreserve')}: {result.does_not_preserve.join('; ')}</p>
          )}
        </section>
      )}
    </div>
  );
}
