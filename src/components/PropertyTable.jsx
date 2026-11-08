import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import './PropertyTable.css';
import { useI18n } from '../i18n/I18nContext';
import CTLTrace from './CTLTrace';
import { markingToString } from '../services/token-value';
import { getDeadlockTrapProperty, getStructuralProperties } from '../services/tauri-api';

// 'computed': a missing value means the check ran but could not decide it (inconclusive).
// 'pending': still running. 'notRun': nothing was attempted yet.
const PhaseContext = createContext('computed');

const SectionCard = ({ title, icon, open = true, children }) => (
  <details className="prop-card" open={open}>
    <summary className="prop-card-header">
      <span className="prop-card-icon" aria-hidden="true">{icon}</span>
      <span className="prop-card-title">{title}</span>
      <span className="prop-chevron" aria-hidden="true">▾</span>
    </summary>
    <div className="prop-card-body">{children}</div>
  </details>
);

const PropRow = ({ code, label, value, desc, phase }) => {
  const { t } = useI18n();
  const contextPhase = useContext(PhaseContext);
  const val = renderVal(value, t, phase ?? contextPhase);
  // Long lists (self-loops, dead transitions...) get their own line instead of crowding the label.
  const stacked = typeof value === 'string' && value.length > 24;
  return (
    <div className={`prop-row${stacked ? ' prop-row-stacked' : ''}`} title={desc || label}>
      <span className="prop-row-left">
        {code && <code className="prop-code">{code}</code>}
        <span className="prop-label">{label}</span>
      </span>
      <span className="prop-val">{val}</span>
    </div>
  );
};

const DeadlockTrapRow = ({ filePath, initialValue }) => {
  const { t } = useI18n();
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    setResult(typeof initialValue === 'boolean' ? { value: initialValue } : null);
    setError(null);
  }, [filePath, initialValue]);

  const calculate = async () => {
    if (!filePath || loading) return;
    setLoading(true);
    setError(null);
    try {
      setResult({ value: await getDeadlockTrapProperty(filePath) });
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <>
      <div className="prop-row">
        <span className="prop-row-left" title={t('props.deadlockTrap.desc')}>
          <code className="prop-code">DT</code>
          <span className="prop-label">Deadlock-Trap</span>
        </span>
        <span className="deadlock-trap-actions">
          <span className="prop-val">
            {result?.value === null ? renderVal(null, t, 'computed') : renderVal(result?.value, t, 'notRun')}
          </span>
          <button className="deadlock-trap-button" onClick={calculate} disabled={!filePath || loading}>
            {loading ? t('props.calculating') : result ? t('props.recalculate') : t('props.calculate')}
          </button>
        </span>
      </div>
      {error && <p className="deadlock-trap-error" role="alert">{t('props.deadlockTrap.failed', { error })}</p>}
    </>
  );
};

const StructuralSubclassRows = ({ filePath, initialProperties }) => {
  const { t } = useI18n();
  const [computed, setComputed] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    setComputed(null);
    setError(null);
  }, [filePath]);

  const calculate = async () => {
    if (!filePath || loading) return;
    setLoading(true);
    setError(null);
    try {
      setComputed(await getStructuralProperties(filePath));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const properties = computed || initialProperties;
  return (
    <>
      <PropRow code="SMC" label={t('props.rows.SMC.label')} value={properties.state_machine_coverable} desc={t('props.rows.SMC.desc')} />
      <PropRow code="SMD" label={t('props.rows.SMD.label')} value={properties.state_machine_decomposable} desc={t('props.rows.SMD.desc')} />
      <PropRow code="SMA" label={t('props.rows.SMA.label')} value={properties.state_machine_allocatable} desc={t('props.rows.SMA.desc')} />
      <div className="prop-row prop-row-end">
        <span className="prop-label">{t('props.subclasses.combinatorial')}</span>
        <button className="deadlock-trap-button" onClick={calculate} disabled={!filePath || loading}>
          {loading ? t('props.calculating') : computed ? t('props.recalculate') : t('props.calculateOnDemand')}
        </button>
      </div>
      {error && <p className="deadlock-trap-error" role="alert">{t('props.subclasses.failed', { error })}</p>}
    </>
  );
};

function renderVal(val, t, phase = 'computed') {
  if (typeof val === 'boolean') {
    return val
      ? <span className="val-badge val-true">✓ {t('common.yes')}</span>
      : <span className="val-badge val-false">✗ {t('common.no')}</span>;
  }
  if (val === null || val === undefined) {
    if (phase === 'pending') return <span className="val-na" aria-busy="true">…</span>;
    if (phase === 'computed') {
      return <span className="val-badge val-unknown" title={t('common.inconclusiveHint')}>? {t('common.inconclusive')}</span>;
    }
    return <span className="val-na">—</span>;
  }
  return <span className="val-text">{String(val)}</span>;
}

const PropertyTable = ({ data, filePath, requiredProperties, requiredPropertiesLoading, badStates, behavioral, placeNames }) => {
  const { t, tm } = useI18n();
  const derived = useMemo(() => {
    if (!data) return {};

    const livenessData = data.liveness;
    const hasLiveness = Array.isArray(livenessData) && livenessData.length > 0;

    // DTr is independent of the liveness level: it requires a transition
    // that is never enabled in any marking reachable from M0.
    const deadTransitionNames = Array.isArray(data.dead_transitions)
      ? data.dead_transitions
      : null;

    // Safety is distinct from boundedness: every reachable place marking
    // must be at most one token.
    const liveAndSafe = data.safe === true && data.live === true;

    // DSt (Dead States) = !deadlock_free
    const deadStates = typeof data.deadlock_free === 'boolean'
      ? !data.deadlock_free
      : undefined;

    return {
      deadTransitionNames,
      liveAndSafe,
      deadStates,
      hasLiveness,
      livenessData,
    };
  }, [data]);

  if (!data) return null;

  const d = derived;
  const s = data.structural || {};
  const inv = data.invariant || {};
  const markingStr = data.initial_marking
    ? `[${markingToString(data.initial_marking)}]`
    : null;

  return (
    <PhaseContext.Provider value={data.analysis_status === 'structural_analysis_complete' ? 'notRun' : 'computed'}>
    <div className="props">
      {/* ── Behavioral Properties ── */}
      <SectionCard title={t('props.sections.behavioral')} icon="◇">
        {behavioral && (
          <p className="props-analysis-pending" role="status">
            {behavioral.loading ? t('props.behavior.computing') : behavioral.computed ? t('props.behavior.done') : t('props.behavior.waiting')}
          </p>
        )}
        {requiredPropertiesLoading && <p className="props-analysis-pending">{t('props.pendingBLS')}</p>}
        <PropRow code="B" label={t('props.rows.B.label')} value={requiredProperties?.bounded ?? data.bounded} phase={requiredPropertiesLoading ? 'pending' : requiredProperties ? 'computed' : 'notRun'} desc={t('props.rows.B.desc')} />
        <PropRow code="DSt" label={t('props.rows.DSt.label')} value={d.deadStates} desc={t('props.rows.DSt.desc')} />
        {d.deadTransitionNames?.length > 0 && (
          <PropRow code="DTr" label={t('props.rows.DTr.label')} value={d.deadTransitionNames.join(', ')} desc={t('props.rows.DTr.desc')} />
        )}
        {d.deadTransitionNames?.length === 0 && (
          <PropRow code="DTr" label={t('props.rows.DTr2.label')} value={false} desc={t('props.rows.DTr2.desc')} />
        )}
        {d.deadTransitionNames === null && (
          <PropRow code="DTr" label={t('props.rows.DTr3.label')} value={undefined} desc={t('props.rows.DTr3.desc')} />
        )}
        <PropRow code="REV" label={t('props.rows.REV.label')} value={data.reversible} desc={t('props.rows.REV.desc')} />
        <PropRow code="L" label={t('props.rows.L.label')} value={requiredProperties?.live ?? data.live} phase={requiredPropertiesLoading ? 'pending' : requiredProperties ? 'computed' : 'notRun'} desc={t('props.rows.L.desc')} />
        <PropRow code="Lp" label={t('props.rows.Lp.label')} value={data.place_live} desc={t('props.rows.Lp.desc')} />
        <PropRow code="L&S" label={t('props.rows.LS.label')} value={requiredProperties?.live_and_safe ?? d.liveAndSafe} phase={requiredPropertiesLoading ? 'pending' : requiredProperties ? 'computed' : 'notRun'} desc={t('props.rows.LS.desc')} />
        <PropRow code="DCF" label={t('props.rows.DCF.label')} value={data.dynamic_conflict_free} desc={t('props.rows.DCF.desc')} />
        {data.conflicts?.pair_count > 0 && (
          <details className="conf-list">
            <summary>{t('props.conflicts.list', { count: data.conflicts.pair_count })}</summary>
            <ul>
              {data.conflicts.pairs.map((pair) => (
                <li key={`${pair.first}|${pair.second}`}>
                  <div className="conf-head">
                    <strong>{pair.first} <span aria-hidden="true">⟷</span> {pair.second}</strong>
                    <span>{t('props.conflicts.states', { count: pair.states })}</span>
                  </div>
                  <details>
                    <summary>{t('props.conflicts.example', { state: pair.example_state })}</summary>
                    <CTLTrace steps={pair.example_path} placeNames={placeNames} kind="witness" />
                  </details>
                </li>
              ))}
            </ul>
            {data.conflicts.pair_count > data.conflicts.pairs.length && <p className="props-behavior-hint">{t('props.conflicts.more', { count: data.conflicts.pair_count - data.conflicts.pairs.length })}</p>}
          </details>
        )}
        <PropRow code="BSt" label={t('props.rows.BSt.label')} value={badStates ? badStates.safe : t('props.notConfigured')} phase="computed" desc={t('props.rows.BSt.desc')} />
        <PropRow code="PER" label={t('props.rows.PER.label')} value={data.persistent} desc={t('props.rows.PER.desc')} />
        
        {d.hasLiveness && (
          <div className="liveness-detail">
            <span className="prop-section-label">{t('props.livenessLevels')}</span>
            <div className="liveness-list">
              {d.livenessData.map((lv, i) => (
                <span key={i} className={`lv-badge lv-${lv.level.toLowerCase().replace('l', '')}`}>
                  {lv.name} {lv.level}
                </span>
              ))}
            </div>
          </div>
        )}
        {data.scc && (
          <div className="scc-detail">
            <PropRow code="SCC" label={t('props.scc.count')} value={String(data.scc.count)} desc={t('props.scc.desc')} />
            <PropRow label={t('props.scc.terminal')} value={String(data.scc.terminal_count)} desc={t('props.scc.terminalDesc')} />
            <PropRow label={t('props.scc.homeStates')} value={String(data.scc.home_state_count)} desc={t('props.scc.homeDesc')} />
            <details className="scc-list">
              <summary>{t('props.scc.list', { count: data.scc.components.length })}</summary>
              <ul>
                {data.scc.components.slice(0, 40).map((c) => (
                  <li key={c.id}>
                    <strong>{t('props.scc.item', { n: c.id + 1, size: c.size })}</strong>
                    {c.contains_initial && <span className="scc-tag">{t('props.scc.hasInitial')}</span>}
                    {c.terminal && <span className="scc-tag">{t('props.scc.isTerminal')}</span>}
                    {c.trivial && <span className="scc-tag">{t('props.scc.isTrivial')}</span>}
                    <code>{c.states.join(', ')}{c.size > c.states.length ? ', …' : ''}</code>
                  </li>
                ))}
              </ul>
              {data.scc.components.length > 40 && <p className="props-behavior-hint">{t('props.scc.more', { count: data.scc.components.length - 40 })}</p>}
            </details>
          </div>
        )}
        {Array.isArray(data.place_liveness) && data.place_liveness.length > 0 && (
          <div className="liveness-detail">
            <span className="prop-section-label">{t('props.placeLiveness')}</span>
            <div className="liveness-list">
              {data.place_liveness.map((place) => (
                <span key={place.place} className={`lv-badge ${place.live ? 'lv-4' : 'lv-0'}`}>
                  {place.name} {place.live ? t('props.liveBadge') : t('props.deadBadge')}
                </span>
              ))}
            </div>
          </div>
        )}
      </SectionCard>

      {/* ── Structural Properties ── */}
      <SectionCard title={t('props.sections.structural')} open={false} icon="▦">
        <PropRow code="ORD" label={t('props.rows.ORD.label')} value={s.ordinary} desc={t('props.rows.ORD.desc')} />
        <PropRow code="HOM" label={t('props.rows.HOM.label')} value={s.homogeneous} desc={t('props.rows.HOM.desc')} />
        <PropRow code="CON" label={t('props.rows.CON.label')} value={s.connected} desc={t('props.rows.CON.desc')} />
        <PropRow code="SC" label={t('props.rows.SC.label')} value={s.strongly_connected} desc={t('props.rows.SC.desc')} />
        <PropRow code="PUR" label={t('props.rows.PUR.label')} value={s.pure} desc={t('props.rows.PUR.desc')} />
        <PropRow code="SCF" label={t('props.rows.SCF.label')} value={s.static_conflict_free} desc={t('props.rows.SCF.desc')} />
        <PropRow code="SCON" label={t('props.rows.SCON.label')} value={s.structurally_concurrent} desc={t('props.rows.SCON.desc')} />
        <PropRow code="MG" label={t('props.rows.MG.label')} value={s.marked_graph} desc={t('props.rows.MG.desc')} />
        <PropRow code="FC" label={t('props.rows.FC.label')} value={s.free_choice} desc={t('props.rows.FC.desc')} />
        <PropRow code="EFC" label={t('props.rows.EFC.label')} value={s.extended_free_choice} desc={t('props.rows.EFC.desc')} />
        <PropRow code="NBM" label={t('props.rows.NBM.label')} value={s.non_blocking} desc={t('props.rows.NBM.desc')} />
        <PropRow code="SM" label={t('props.rows.SM.label')} value={s.state_machine} desc={t('props.rows.SM.desc')} />
        <PropRow code="ES" label={t('props.rows.ES.label')} value={s.extended_simple} desc={t('props.rows.ES.desc')} />
        <PropRow code="Ft0" label={t('props.rows.Ft0.label')} value={s.transitions_without_preset} desc={t('props.rows.Ft0.desc')} />
        <PropRow code="tF0" label={t('props.rows.tF0.label')} value={s.transitions_without_postset} desc={t('props.rows.tF0.desc')} />
        <PropRow code="Fp0" label={t('props.rows.Fp0.label')} value={s.places_without_preset} desc={t('props.rows.Fp0.desc')} />
        <PropRow code="pF0" label={t('props.rows.pF0.label')} value={s.places_without_postset} desc={t('props.rows.pF0.desc')} />
        <PropRow code="GFC" label={t('props.rows.GFC.label')} value={data.gfc} desc={t('props.rows.GFC.desc')} />
        <DeadlockTrapRow filePath={filePath} initialValue={data.deadlock_trap} />
        {requiredProperties?.warning && <p className="deadlock-trap-error" role="status">{tm(requiredProperties.warning)}</p>}
      </SectionCard>

      {/* ── Structural Sub-classes ── */}
      <SectionCard title={t('props.sections.subclasses')} open={false} icon="⊕">
        <StructuralSubclassRows filePath={filePath} initialProperties={s} />
      </SectionCard>

      {/* ── Invariants ── */}
      <SectionCard title={t('props.sections.invariants')} icon="∑">
        {requiredPropertiesLoading && <p className="props-analysis-pending">{t('props.pendingCSVSB')}</p>}
        <PropRow code="SB" label={t('props.rows.SB.label')} value={requiredProperties?.structurally_bounded ?? inv.structurally_bounded} phase={requiredPropertiesLoading ? 'pending' : requiredProperties ? 'computed' : 'notRun'} desc={t('props.rows.SB.desc')} />
        <PropRow code="CSV" label={t('props.rows.CSV.label')} value={requiredProperties?.conservative ?? inv.conservative} phase={requiredPropertiesLoading ? 'pending' : requiredProperties ? 'computed' : 'notRun'} desc={t('props.rows.CSV.desc')} />
        <PropRow code="CPI" label={t('props.rows.CPI.label')} value={inv.p_invariant_coverage} desc={t('props.rows.CPI.desc')} />
        <PropRow code="CTI" label={t('props.rows.CTI.label')} value={inv.t_invariant_coverage} desc={t('props.rows.CTI.desc')} />
        {inv.minimal_p_semiflows && inv.minimal_p_semiflows.length > 0 && (
          <PropRow code="P-Inv" label={t('props.rows.PInv.label')} value={inv.minimal_p_semiflows.length} />
        )}
        {inv.minimal_t_semiflows && inv.minimal_t_semiflows.length > 0 && (
          <PropRow code="T-Inv" label={t('props.rows.TInv.label')} value={inv.minimal_t_semiflows.length} />
        )}
      </SectionCard>

      {/* ── Analysis Info ── */}
      <SectionCard title={t('props.sections.info')} open={false} icon="ⓘ">
        <PropRow label={t('props.info.states')} value={data.analysis_info?.num_states ?? t('common.na')} />
        <PropRow label={t('props.info.duration')} value={
          data.analysis_info?.duration_ms != null
            ? `${Number(data.analysis_info.duration_ms).toFixed(2)} ms`
            : t('common.na')
        } />
        <PropRow label={t('props.info.selfLoops')} value={
          data.self_loops?.length > 0 ? data.self_loops.join(', ') : (data.has_self_loops === false ? t('props.info.none') : '—')
        } />
      </SectionCard>
    </div>
    </PhaseContext.Provider>
  );
};

export default PropertyTable;
