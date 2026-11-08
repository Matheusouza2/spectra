import React, { useEffect, useMemo, useState } from 'react';
import { getAlgorithmCatalog, validateAnalysisConfig } from '../services/tauri-api';
import { useI18n } from '../i18n/I18nContext';
import './AlgorithmPanel.css';

const STATUS_KEY = {
  stable: 'algo.status.stable',
  experimental: 'algo.status.experimental',
  legacy_experimental: 'algo.status.legacy',
  proposed: 'algo.status.proposed',
  under_study: 'algo.status.underStudy',
};

const OBJECTIVES = [
  { id: 'reachability', labelKey: 'algo.obj.reachability', hintKey: 'algo.obj.reachabilityHint' },
  { id: 'coverability', labelKey: 'algo.obj.coverability', hintKey: 'algo.obj.coverabilityHint' },
];

const MODES = [
  { id: 'manual', labelKey: 'algo.mode.manual', hintKey: 'algo.mode.manualHint' },
  { id: 'automatic', labelKey: 'algo.mode.automatic', hintKey: 'algo.mode.automaticHint' },
  { id: 'none', labelKey: 'algo.mode.none', hintKey: 'algo.mode.noneHint' },
];

let catalogPromise = null;
const loadCatalog = () => {
  if (!catalogPromise) {
    catalogPromise = getAlgorithmCatalog().catch((error) => {
      catalogPromise = null;
      throw error;
    });
  }
  return catalogPromise;
};

const List = ({ title, items }) => {
  const { tm } = useI18n();
  return items && items.length > 0 ? (
    <div className="alg-list"><strong>{title}</strong><ul>{items.map((item) => <li key={item}>{tm(item)}</li>)}</ul></div>
  ) : null;
};

const References = ({ ids, references }) => {
  const { t, tm } = useI18n();
  if (!ids || ids.length === 0) return <p className="alg-muted">{t('algo.classic')}</p>;
  return (
    <ul className="alg-refs">
      {ids.map((id) => {
        const ref = references.find((r) => r.id === id);
        if (!ref) return <li key={id}>{id}</li>;
        return (
          <li key={id}>
            <span className="alg-ref-id">[{ref.id}]</span> {ref.citation} ({ref.year}).{' '}
            {ref.doi && <span>DOI: {ref.doi}. </span>}
            {ref.url && <a href={ref.url} target="_blank" rel="noreferrer">{ref.url}</a>}
            <em> — {tm(ref.access)}</em>
          </li>
        );
      })}
    </ul>
  );
};

const AlgorithmCard = ({ entry, references, selected, disabledReason, onSelect, type, name, expandAlways = false, flat = false }) => {
  const { t, tm } = useI18n();
  const unavailable = !isRunnable(entry) || Boolean(disabledReason);
  return (
    <div className={`alg-card ${selected ? 'is-selected' : ''} ${unavailable ? 'is-unavailable' : ''} ${flat ? 'is-flat' : ''}`}>
      <label className="alg-card-head">
        <input type={type} name={name} checked={selected} disabled={unavailable} onChange={() => onSelect(entry.id)} />
        <span className="alg-card-title">{tm(entry.name)}</span>
        <span className={`alg-badge alg-badge-${entry.status}`}>{STATUS_KEY[entry.status] ? t(STATUS_KEY[entry.status]) : entry.status}</span>
      </label>
      {(!isRunnable(entry) || disabledReason) && (
        <p className="alg-card-blocked">{t('algo.unavailable')}: {disabledReason || tm(entry.when_not_to_use)}</p>
      )}
      {(selected || expandAlways) && <p className="alg-card-desc">{tm(entry.description)}</p>}
      {(selected || expandAlways) && <details className="alg-details">
        <summary>{t('algo.howItWorks')}</summary>
        <dl>
          <dt>{t('algo.whenToUse')}</dt><dd>{tm(entry.when_to_use)}</dd>
          <dt>{t('algo.whenNotToUse')}</dt><dd>{tm(entry.when_not_to_use)}</dd>
          <dt>{t('algo.produces')}</dt><dd>{tm(entry.output)}</dd>
          <dt>{t('algo.implVersion')}</dt><dd>{entry.implementation_version || t('algo.notImplemented')}</dd>
        </dl>
        <List title={t('algo.canConclude')} items={entry.preserves} />
        <List title={t('algo.doNotConclude')} items={entry.does_not_preserve} />
        {entry.notes && <p className="alg-note">{tm(entry.notes)}</p>}
        <strong>{t('algo.references')}</strong>
        <References ids={entry.reference_ids} references={references} />
      </details>}
    </div>
  );
};

/**
 * Algorithm / reduction selection (spec 2026 §5). Validates every change with
 * the Rust capability matrix and never enables an incompatible option without
 * showing the reason.
 */
// Coverability-set methods that the catalog keeps out of the exploration jobs: GraphTab runs them
// through the advanced-analysis command and shows the resulting set, so the panel treats them as runnable.
export const SET_METHOD_IDS = new Set(['coverability_grvb', 'coverability_mp']);
const isRunnable = (a) => a.selectable || SET_METHOD_IDS.has(a.id);

const AlgorithmPanel = ({ extended, disabled, onRun, children, bounded = null, resetKey }) => {
  const { t, tm } = useI18n();
  const [catalog, setCatalog] = useState(null);
  const [catalogError, setCatalogError] = useState(null);
  // Reachability needs a finite state space, so it is only recommended for bounded nets.
  // Unbounded or still-unknown boundedness -> coverability (Karp-Miller decides boundedness and always terminates).
  const boundedKnown = typeof bounded === 'boolean' ? bounded : null;
  const recommended = boundedKnown === true ? 'reachability' : 'coverability';
  const [objective, setObjective] = useState(recommended);
  const [mode, setMode] = useState('manual');
  const [exploration, setExploration] = useState('reachability_bfs');
  const [chosenReductions, setChosenReductions] = useState([]);
  const toggleReduction = (id) => setChosenReductions((current) => (current.includes(id) ? current.filter((x) => x !== id) : [...current, id]));
  const [report, setReport] = useState(null);

  useEffect(() => {
    let active = true;
    loadCatalog().then((data) => active && setCatalog(data)).catch((e) => active && setCatalogError(String(e)));
    return () => { active = false; };
  }, []);

  useEffect(() => { setObjective(recommended); }, [recommended, resetKey]);

  const netClass = extended ? 'EXTENDED_PT' : 'PT_WEIGHTED';
  // Methods with no published Petri-net paper stay out of the UI. When that leaves an objective
  // with nothing to pick (reachability: BFS/DFS), the first of them still runs as the default.
  const hasPaper = (a) => a.reference_ids && a.reference_ids.length > 0;
  const allExplorations = useMemo(
    () => (catalog ? catalog.algorithms.filter((a) => a.role === 'exploration' && isRunnable(a) && a.objectives.includes(objective)) : []),
    [catalog, objective],
  );
  const explorations = useMemo(() => allExplorations.filter(hasPaper), [allExplorations]);
  const explorationPool = explorations.length ? explorations : allExplorations;
  const usesDefaultExploration = explorations.length === 0 && allExplorations.length > 0;
  const reductions = useMemo(
    () => (catalog ? catalog.algorithms.filter((a) => a.role === 'reduction' && a.objectives.includes(objective) && a.selectable && hasPaper(a)) : []),
    [catalog, objective],
  );

  // Keep the exploration choice coherent with the objective.
  useEffect(() => {
    if (!explorationPool.length) return;
    if (!explorationPool.some((a) => a.id === exploration && isRunnable(a))) {
      const first = explorationPool.find(isRunnable);
      if (first) setExploration(first.id);
    }
  }, [explorationPool, exploration]);

  const request = useMemo(() => ({
    objective,
    mode,
    exploration: mode === 'automatic' ? null : exploration,
    reductions: mode === 'manual' ? chosenReductions.filter((id) => reductions.some((r) => r.id === id)) : [],
    net_class: netClass,
  }), [objective, mode, exploration, chosenReductions, reductions, netClass]);

  useEffect(() => {
    if (!catalog) return undefined;
    const setMethod = mode === 'manual' ? catalog.algorithms.find((a) => a.id === exploration && SET_METHOD_IDS.has(a.id)) : null;
    if (setMethod) {
      setReport({
        status: 'valid', experimental: true, pipeline: [setMethod.name], applied: [], ignored: [], warnings: [],
        preserves: setMethod.preserves, does_not_preserve: setMethod.does_not_preserve, automatic_decisions: [],
      });
      return undefined;
    }
    let active = true;
    validateAnalysisConfig(request)
      .then((result) => active && setReport(result))
      .catch((e) => active && setReport({ status: 'unsupported_combination', issues: [{ code: 'error', message: String(e) }] }));
    return () => { active = false; };
  }, [catalog, request]);

  if (catalogError) return <p className="gv-generation-error" role="alert">{t('algo.catalogUnavailable', { error: tm(catalogError) })}</p>;
  if (!catalog) return <p className="alg-muted">{t('algo.catalogLoading')}</p>;

  const valid = report?.status === 'valid';
  const effectiveId = mode === 'automatic' ? report?.automatic_decisions?.[0]?.id : exploration;
  const chosen = explorationPool.find((a) => a.id === exploration);

  const run = () => onRun({
    exploration: effectiveId,
    stubborn: request.reductions.includes('partial_order_stubborn'),
    symmetry: request.reductions.includes('symmetry_reduction'),
    experimental: Boolean(report?.experimental),
    doesNotPreserve: report?.does_not_preserve || [],
    preserves: report?.preserves || [],
    mode,
  });

  return (
    <section className="alg-panel" aria-label={t('algo.ariaLabel')}>
      <div className="alg-step">
      <h3>{t('algo.goal')}</h3>
      <div className="alg-row" role="radiogroup" aria-label={t('algo.objective')}>
        {OBJECTIVES.map((o) => {
          const blocked = o.id === 'reachability' && boundedKnown === false;
          return (
            <button key={o.id} type="button" role="radio" aria-checked={objective === o.id} disabled={blocked}
              className={`alg-seg ${objective === o.id ? 'is-on' : ''}`} onClick={() => setObjective(o.id)}
              title={blocked ? t('algo.reachabilityBlocked') : t(o.hintKey)}>
              {t(o.labelKey)}
              {o.id === recommended && <span className="alg-reco">{t('algo.recommended')}</span>}
            </button>
          );
        })}
      </div>
      <p className="alg-reco-note">{t(boundedKnown === true ? 'algo.reco.bounded' : boundedKnown === false ? 'algo.reco.unbounded' : 'algo.reco.unknown')}</p>
      </div>

      <div className="alg-step">
      <h3>{t('algo.method')}</h3>
      <div className="alg-row" role="radiogroup" aria-label={t('algo.mode.label')}>
        {MODES.map((m) => (
          <button key={m.id} type="button" role="radio" aria-checked={mode === m.id}
            className={`alg-seg ${mode === m.id ? 'is-on' : ''}`} onClick={() => setMode(m.id)}>
            {t(m.labelKey)}
          </button>
        ))}
      </div>
      <p className="alg-muted">{t(MODES.find((m) => m.id === mode).hintKey)}</p>

      {extended && (
        <p className="alg-warn" role="note">
          {t('algo.extendedWarning')}
        </p>
      )}

      {mode === 'automatic' ? (
        <p className="alg-auto">
          {t('algo.autoWillUse')} <strong>{report?.automatic_decisions?.[0]?.id || '—'}</strong>
          {report?.automatic_decisions?.[0] && <> — {tm(report.automatic_decisions[0].reason)}</>}
        </p>
      ) : usesDefaultExploration ? (
        <p className="alg-muted">{t('algo.defaultExploration', { name: tm(chosen?.name || '') })}</p>
      ) : (
        <>
          <label className="alg-field">
            {t('algo.explorationLegend')}
            <select value={exploration} onChange={(e) => setExploration(e.target.value)}>
              {explorations.map((entry) => {
                const blocked = !isRunnable(entry) || !entry.supported_net_classes.includes(netClass);
                return <option key={entry.id} value={entry.id} disabled={blocked}>{tm(entry.name)}{blocked ? ` (${t('algo.unavailable').toLowerCase()})` : ''}</option>;
              })}
            </select>
          </label>
          {chosen && (
            <AlgorithmCard entry={chosen} references={catalog.references} type="radio" name="alg-exploration" selected flat
              disabledReason={!chosen.supported_net_classes.includes(netClass) ? t('algo.monotonicityBlocked') : null}
              onSelect={() => {}} />
          )}
        </>
      )}
      </div>

      {mode === 'manual' && (
        <div className="alg-step">
        <h3>{t('algo.reductionsLegend')}</h3>
        {reductions.length === 0 && <p className="alg-muted">{t('algo.noReductions')}</p>}
        {reductions.map((entry) => (
          <AlgorithmCard key={entry.id} entry={entry} references={catalog.references} type="checkbox" name="alg-reduction"
            selected={chosenReductions.includes(entry.id)}
            disabledReason={!entry.supported_net_classes.includes(netClass) ? t('algo.reductionBlocked') : null}
            onSelect={() => toggleReduction(entry.id)} />
        ))}
        </div>
      )}

      <div className={`alg-report ${valid ? 'is-valid' : 'is-invalid'}`} role="status" aria-live="polite">
        {!report && <span>{t('algo.validating')}</span>}
        {report && valid && (
          <>
            <strong>{t('algo.valid')}</strong>
            <details className="alg-details">
              <summary>{t('algo.pipelineDetails')}</summary>
            <ol className="alg-pipeline">{report.pipeline.map((step) => <li key={step}>{tm(step)}</li>)}</ol>
            <List title={t('algo.selectedApplied')} items={report.applied.map((a) => `${a.id}: ${tm(a.reason)}`)} />
            <List title={t('algo.ignored')} items={report.ignored.map((a) => `${a.id}: ${tm(a.reason)}`)} />
            <List title={t('algo.guarantees')} items={report.preserves} />
            <List title={t('algo.doNotConcludeResult')} items={report.does_not_preserve} />
            </details>
            {report.warnings.map((w) => <p key={w} className="alg-warn">{tm(w)}</p>)}
          </>
        )}
        {report && !valid && (
          <>
            <strong>{t('algo.unsupported')}</strong>
            <ul>{(report.issues || []).map((i) => <li key={`${i.code}-${i.message}`}>{tm(i.message)}</li>)}</ul>
          </>
        )}
      </div>

      {children}

      <button className="gv-calc-btn alg-run" disabled={disabled || !valid || !effectiveId} onClick={run}>
        {t('algo.run')}
      </button>
    </section>
  );
};

export default AlgorithmPanel;
