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
            <a href={ref.url} target="_blank" rel="noreferrer">{ref.url}</a>
            <em> — {tm(ref.access)}</em>
          </li>
        );
      })}
    </ul>
  );
};

const AlgorithmCard = ({ entry, references, selected, disabledReason, onSelect, type, name }) => {
  const { t, tm } = useI18n();
  const unavailable = !entry.selectable || Boolean(disabledReason);
  return (
    <div className={`alg-card ${selected ? 'is-selected' : ''} ${unavailable ? 'is-unavailable' : ''}`}>
      <label className="alg-card-head">
        <input type={type} name={name} checked={selected} disabled={unavailable} onChange={() => onSelect(entry.id)} />
        <span className="alg-card-title">{tm(entry.name)}</span>
        <span className={`alg-badge alg-badge-${entry.status}`}>{STATUS_KEY[entry.status] ? t(STATUS_KEY[entry.status]) : entry.status}</span>
      </label>
      <p className="alg-card-desc">{tm(entry.description)}</p>
      {(!entry.selectable || disabledReason) && (
        <p className="alg-card-blocked">{t('algo.unavailable')}: {disabledReason || tm(entry.when_not_to_use)}</p>
      )}
      <details className="alg-details">
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
      </details>
    </div>
  );
};

/**
 * Algorithm / reduction selection (spec 2026 §5). Validates every change with
 * the Rust capability matrix and never enables an incompatible option without
 * showing the reason.
 */
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
  const [stubborn, setStubborn] = useState(false);
  const [report, setReport] = useState(null);

  useEffect(() => {
    let active = true;
    loadCatalog().then((data) => active && setCatalog(data)).catch((e) => active && setCatalogError(String(e)));
    return () => { active = false; };
  }, []);

  useEffect(() => { setObjective(recommended); }, [recommended, resetKey]);

  const netClass = extended ? 'EXTENDED_PT' : 'PT_WEIGHTED';
  const explorations = useMemo(
    () => (catalog ? catalog.algorithms.filter((a) => a.role === 'exploration' && a.objectives.includes(objective)) : []),
    [catalog, objective],
  );
  const reductions = useMemo(
    () => (catalog ? catalog.algorithms.filter((a) => a.role === 'reduction' && a.objectives.includes(objective)) : []),
    [catalog, objective],
  );

  // Keep the exploration choice coherent with the objective.
  useEffect(() => {
    if (!explorations.length) return;
    if (!explorations.some((a) => a.id === exploration && a.selectable)) {
      const first = explorations.find((a) => a.selectable);
      if (first) setExploration(first.id);
    }
  }, [explorations, exploration]);

  const request = useMemo(() => ({
    objective,
    mode,
    exploration: mode === 'automatic' ? null : exploration,
    reductions: stubborn && mode !== 'none' ? ['partial_order_stubborn'] : [],
    net_class: netClass,
  }), [objective, mode, exploration, stubborn, netClass]);

  useEffect(() => {
    if (!catalog) return undefined;
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
  const hidden = catalog.algorithms.filter((a) => !a.selectable);

  const run = () => onRun({
    exploration: effectiveId,
    stubborn: request.reductions.includes('partial_order_stubborn'),
    experimental: Boolean(report?.experimental),
    doesNotPreserve: report?.does_not_preserve || [],
    preserves: report?.preserves || [],
    mode,
  });

  return (
    <section className="alg-panel" aria-label={t('algo.ariaLabel')}>
      <h3>{t('algo.title')}</h3>

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
      <p className="alg-muted">{t(OBJECTIVES.find((o) => o.id === objective).hintKey)} {t('algo.coverageNote')}</p>

      <label className="alg-field">
        {t('algo.mode.label')}
        <select value={mode} onChange={(e) => setMode(e.target.value)}>
          {MODES.map((m) => <option key={m.id} value={m.id}>{t(m.labelKey)}</option>)}
          <option value="comparison" disabled>{t('algo.mode.comparison')}</option>
        </select>
        <small>{t(MODES.find((m) => m.id === mode).hintKey)}</small>
      </label>

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
      ) : (
        <fieldset className="alg-group">
          <legend>{t('algo.explorationLegend')}</legend>
          {explorations.map((entry) => (
            <AlgorithmCard key={entry.id} entry={entry} references={catalog.references} type="radio" name="alg-exploration"
              selected={exploration === entry.id}
              disabledReason={!entry.supported_net_classes.includes(netClass)
                ? t('algo.monotonicityBlocked') : null}
              onSelect={setExploration} />
          ))}
        </fieldset>
      )}

      {mode === 'manual' && (
        <fieldset className="alg-group">
          <legend>{t('algo.reductionsLegend')}</legend>
          {reductions.length === 0 && <p className="alg-muted">{t('algo.noReductions')}</p>}
          {reductions.map((entry) => (
            <AlgorithmCard key={entry.id} entry={entry} references={catalog.references} type="checkbox" name="alg-reduction"
              selected={stubborn && entry.id === 'partial_order_stubborn'}
              disabledReason={!entry.supported_net_classes.includes(netClass)
                ? t('algo.reductionBlocked') : null}
              onSelect={() => setStubborn((v) => !v)} />
          ))}
        </fieldset>
      )}

      <details className="alg-details alg-planned">
        <summary>{t('algo.proposed', { count: hidden.length })}</summary>
        {hidden.map((entry) => (
          <AlgorithmCard key={entry.id} entry={entry} references={catalog.references} type="radio" name="alg-hidden"
            selected={false} onSelect={() => {}} />
        ))}
      </details>

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
