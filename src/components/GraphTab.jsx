import React, { useEffect, useState, useCallback, useRef } from 'react';
import { getReachabilityGraph, getCoverageTree, getMinimalCoverabilitySet, runAdvancedAnalysis } from '../services/tauri-api';
import AlgorithmPanel, { SET_METHOD_IDS } from './AlgorithmPanel';
import { loadGraphGenerationConfig, saveGraphGenerationConfig, validateGraphGenerationConfig } from '../services/graph-config';
import { useToast } from './Toast';
import useAnalysisGraph from '../hooks/useAnalysisGraph';
import { useI18n } from '../i18n/I18nContext';
import './GraphViewer.css';

const LEVEL_STEP = 5;

const GraphTab = ({ filePath, analysisData, bounded = null, extended = false, onGraphProperties, onGraphGenerated, transitionNames }) => {
  const [graphData, setGraphData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [graphType, setGraphType] = useState(null);
  const [requestedType, setRequestedType] = useState(null);
  const [error, setError] = useState(null);
  const [config, setConfig] = useState(loadGraphGenerationConfig);
  const [coverSet, setCoverSet] = useState(null);
  const [lastRun, setLastRun] = useState(null);
  const [setupOpen, setSetupOpen] = useState(true);
  const [stageSize, setStageSize] = useState(0);
  const requestIdRef = useRef(0);
  const { addToast } = useToast();
  const { t, tm, language } = useI18n();
  const fmt = (n) => Number(n).toLocaleString(language);

  const {
    containerRef,
    selectedNode,
    setSelectedNode,
    visibleDepth,
    maxDepth,
    renderGraph,
    showMore,
    hasMore,
    markingDisplay,
    transitionLabel,
  } = useAnalysisGraph({ transitionNames });

  useEffect(() => {
    requestIdRef.current += 1;
    setGraphData(null);
    setGraphType(null);
    setCoverSet(null);
    setLastRun(null);
    setError(null);
    setLoading(false);
    setSetupOpen(true);
  }, [filePath]);

  const computeGraph = useCallback(async (selection) => {
    if (!filePath || loading || !selection?.exploration) return;
    const requestId = ++requestIdRef.current;
    const configError = validateGraphGenerationConfig(config);
    if (configError) {
      const label = t(`graphs.limitLabel.${configError.key}`);
      setError(t('graphs.limitError', { label, min: fmt(configError.min), max: fmt(configError.max) }));
      return;
    }

    const limits = {
      maxNodes: Number(config.maxNodes),
      maxEdges: Number(config.maxEdges),
      memoryLimitMb: Number(config.memoryLimitMb),
    };
    saveGraphGenerationConfig(limits);
    setConfig(limits);
    const normalizedConfig = { ...limits, algorithm: selection.exploration, stubborn: selection.stubborn, symmetry: selection.symmetry };
    const mode = selection.exploration.startsWith('reachability') ? 'reachability' : 'coverage';
    const isSetMethod = SET_METHOD_IDS.has(selection.exploration);
    const isMincov = selection.exploration === 'coverability_mincov' || isSetMethod;
    setRequestedType(isMincov ? 'mincov' : mode);
    setLoading(true);
    setError(null);
    try {
      if (isMincov) {
        let data;
        if (isSetMethod) {
          const result = await runAdvancedAnalysis(filePath, selection.exploration, { limit: limits.maxNodes });
          data = { ...result.summary, elapsed_ms: result.elapsed_ms, method: result.name };
        } else {
          data = await getMinimalCoverabilitySet(filePath, normalizedConfig);
        }
        if (requestIdRef.current !== requestId) return;
        setGraphData(null);
        setGraphType(null);
        setCoverSet(data);
      } else {
        const data = mode === 'reachability'
          ? await getReachabilityGraph(filePath, normalizedConfig)
          : await getCoverageTree(filePath, normalizedConfig);
        if (requestIdRef.current !== requestId) return;
        setCoverSet(null);
        setGraphData(data);
        setGraphType(mode);
        // The backend saves complete, unreduced graphs; the app learns about it here.
        onGraphGenerated?.(mode);
        // A complete, unreduced reachability graph settles DSt / DTr / REV for the Analysis tab.
        if (mode === 'reachability' && !data.display_truncated && !selection.stubborn && !selection.symmetry) {
          onGraphProperties?.(data);
        }
        setSelectedNode(null);
      }
      setLastRun(selection);
      setSetupOpen(false);
    } catch (err) {
      if (requestIdRef.current !== requestId) return;
      const msg = t('graphs.runFailed', { algorithm: selection.exploration, error: tm(String(err)) });
      setError(msg);
      addToast(msg, 'error', 7000);
    } finally {
      if (requestIdRef.current === requestId) setLoading(false);
    }
  }, [filePath, loading, config, addToast, setSelectedNode, t, tm, onGraphGenerated, onGraphProperties]);

  useEffect(() => {
    if (!graphData || !graphData.nodes || !graphData.nodes.length) return;
    const depth = computeDepth(graphData.nodes, graphData.edges || []);
    const maxD = Math.max(...Object.values(depth).filter((d) => d >= 0), 0);
    renderGraph(graphData, graphType, maxD);
  }, [graphData, graphType, visibleDepth, renderGraph, stageSize]);

  // Re-layout when the canvas box changes (setup panel toggled, window resized).
  useEffect(() => {
    const el = containerRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return undefined;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setStageSize(el.clientWidth * 10000 + el.clientHeight));
    });
    observer.observe(el);
    return () => { cancelAnimationFrame(frame); observer.disconnect(); };
  }, [containerRef, graphData]);

  const limitField = (key, labelKey, min, max, step) => (
    <label className="gv-field">
      <span>{t(labelKey)}</span>
      <input type="number" min={min} max={max} step={step} value={config[key]}
        onChange={(event) => setConfig((current) => ({ ...current, [key]: event.target.value === '' ? '' : Number(event.target.value) }))} />
      <small>{t('graphs.range', { min: fmt(min), max: fmt(max) })}</small>
    </label>
  );

  const ResourceSettings = (
    <details className="gv-limits">
      <summary>{t('graphs.limits')}</summary>
      <div className="gv-limits-grid">
        {limitField('maxNodes', 'graphs.maxStates', 1, 10000000, 100000)}
        {limitField('maxEdges', 'graphs.maxEdges', 1, 50000000, 500000)}
        {limitField('memoryLimitMb', 'graphs.memoryBudget', 64, 8192, 256)}
      </div>
      <p>{t('graphs.limitsNote')}</p>
    </details>
  );

  if (!filePath) {
    return <div className="gv-stage-empty">{t('graphs.selectNet')}</div>;
  }

  const isRG = graphType === 'reachability';
  const hasGraph = Boolean(graphData?.nodes?.length);
  const totalNodes = graphData?.total_nodes ?? graphData?.nodes?.length ?? 0;
  const totalEdges = graphData?.total_edges ?? graphData?.edges?.length ?? 0;
  const depthMap = hasGraph && selectedNode ? computeDepth(graphData.nodes, graphData.edges || []) : null;

  const kindLabel = coverSet
    ? t('graphs.minCovTitle')
    : graphType ? (isRG ? t('graphs.reachabilityGraph') : t('graphs.coverageTree')) : null;

  const toolbar = (
    <div className="gv-toolbar">
      <button type="button" className="gv-tool-btn" aria-expanded={setupOpen} aria-controls="gv-setup" onClick={() => setSetupOpen((v) => !v)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
          <path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0" /><circle cx="16" cy="6" r="2" /><circle cx="10" cy="12" r="2" /><circle cx="18" cy="18" r="2" />
        </svg>
        {t(setupOpen ? 'graphs.hideSetup' : 'graphs.showSetup')}
      </button>
      {kindLabel && <span className="gv-kind">{kindLabel}</span>}
      {hasGraph && (
        <span className="gv-stats">
          <span><b>{fmt(totalNodes)}</b> {t('graphs.statStates')}</span>
          <span><b>{fmt(totalEdges)}</b> {t('graphs.statEdges')}</span>
          <span>{t('graphs.depth')} <b>{visibleDepth}</b>/{maxDepth}</span>
          {!isRG && graphData.max_states_exceeded && <span className="gv-warning">{t('graphs.limitExceeded')}</span>}
        </span>
      )}
      <span className="gv-toolbar-spacer" />
      {hasGraph && hasMore && (
        <button type="button" className="gv-tool-btn" onClick={showMore}>+{LEVEL_STEP} {t('graphs.levels')}</button>
      )}
    </div>
  );

  const notices = (
    <>
      {lastRun && (
        <div className="gv-notice" role="note">
          {t('graphs.ranWith')} <strong>{lastRun.exploration}</strong>{lastRun.stubborn ? ' + stubborn sets' : ''}{lastRun.symmetry ? ' + symmetry' : ''}
          {lastRun.experimental ? ` — ${t('graphs.experimental')}` : '.'}
          {lastRun.doesNotPreserve?.length > 0 && <> {t('graphs.doNotConclude')}: {lastRun.doesNotPreserve.map(tm).join(', ')}.</>}
        </div>
      )}
      {hasGraph && graphData.display_truncated && (
        <div className="gv-notice">{t('graphs.truncated', { nodes: fmt(totalNodes), edges: fmt(totalEdges), shownNodes: fmt(graphData.nodes.length), shownEdges: fmt(graphData.edges.length) })}</div>
      )}
      {error && <div className="gv-notice gv-notice-error" role="alert">{tm(error)}</div>}
    </>
  );

  let viewport;
  if (loading) {
    const label = t(requestedType === 'reachability' ? 'graphs.kind.reachability' : requestedType === 'mincov' ? 'graphs.kind.mincov' : 'graphs.kind.coverage');
    viewport = <div className="gv-stage-empty"><span className="spinner-sm" /> {t('graphs.generating', { label })}</div>;
  } else if (coverSet) {
    viewport = (
      <div className="gv-cover" aria-label={t('graphs.minCovTitle')}>
        <h4>{t('graphs.minCovTitle')} ({coverSet.method || 'MinCov'}) — {coverSet.bounded ? t('graphs.netBounded') : t('graphs.netUnbounded')}</h4>
        <p>
          {t('graphs.minCovStats', { markings: coverSet.elements.length, vertices: coverSet.tree_vertices, accelerations: coverSet.accelerations, ms: coverSet.elapsed_ms })}
          {coverSet.unbounded_places.length > 0 && <> {t('graphs.unboundedPlaces')}: {coverSet.unbounded_places.join(', ')}.</>}
          {' '}{t('graphs.minCovNote')} {t('graphs.minCovRef')}
        </p>
        <div className="gv-cover-scroll">
          <table>
            <thead><tr>{coverSet.place_names.map((n) => <th key={n}>{n}</th>)}</tr></thead>
            <tbody>
              {coverSet.elements.map((row, index) => (
                <tr key={index}>{row.map((v, i) => <td key={i} className={v === 'omega' ? 'is-omega' : ''}>{v === 'omega' ? 'ω' : v}</td>)}</tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    );
  } else if (graphData && !hasGraph) {
    viewport = <div className="gv-stage-empty">{t('graphs.noNodes')}</div>;
  } else if (!graphData) {
    viewport = (
      <div className="gv-stage-empty gv-stage-intro">
        <strong>{t('graphs.notGenerated')}</strong>
        <span>{t('graphs.notGeneratedHint')}</span>
        {!setupOpen && <button type="button" className="gv-tool-btn" onClick={() => setSetupOpen(true)}>{t('graphs.showSetup')}</button>}
      </div>
    );
  }

  return (
    <div className={`gv-workbench ${setupOpen ? 'is-setup-open' : ''}`}>
      <aside className="gv-setup" id="gv-setup" hidden={!setupOpen}>
        <AlgorithmPanel extended={extended} disabled={loading} onRun={computeGraph} bounded={bounded ?? analysisData?.bounded ?? null} resetKey={filePath}>
          {ResourceSettings}
        </AlgorithmPanel>
      </aside>

      <section className="gv-stage">
        {toolbar}
        {notices}
        <div className="gv-viewport">
          {/* The canvas stays mounted so d3 keeps a stable container between renders. */}
          <div ref={containerRef} className="gv-canvas" hidden={!hasGraph || loading || Boolean(coverSet)} />
          {viewport}
          {hasGraph && !loading && !coverSet && (
            selectedNode ? (
              <aside className="gv-inspector" aria-label={t('graphs.selectedNode')}>
                <header>
                  <span>{t('graphs.selectedNode')}</span>
                  <button type="button" className="gv-inspector-close" aria-label={t('common.close')} onClick={() => setSelectedNode(null)}>×</button>
                </header>
                <dl>
                  <dt>ID</dt><dd className="gv-mono">M{selectedNode.id}</dd>
                  <dt>{t('graphs.marking')}</dt><dd className="gv-mono">[{markingDisplay(selectedNode.marking)}]</dd>
                  <dt>{t('graphs.outgoing')}</dt>
                  <dd className="gv-mono">{(graphData.edges || []).filter((e) => e.source === selectedNode.id).map((e) => `${e.is_abstract ? '≈' : ''}${transitionLabel(e.transition)}`).join(', ') || '—'}</dd>
                  <dt>{t('graphs.depth')}</dt><dd className="gv-mono">{depthMap?.[selectedNode.id] ?? '—'}</dd>
                  {!isRG && selectedNode.is_omega && (<><dt>ω</dt><dd className="gv-omega">{t('graphs.containsOmega')}</dd></>)}
                </dl>
              </aside>
            ) : <p className="gv-hint">{t('graphs.clickNode')}</p>
          )}
        </div>
      </section>
    </div>
  );
};

function computeDepth(nodes, edges) {
  const adj = {};
  const depth = {};
  nodes.forEach((n) => { depth[n.id] = -1; adj[n.id] = []; });
  edges.forEach((e) => {
    if (!adj[e.source]) adj[e.source] = [];
    adj[e.source].push(e.target);
  });
  if (nodes.length === 0) return depth;
  const rootId = nodes[0].id;
  depth[rootId] = 0;
  const queue = [rootId];
  while (queue.length > 0) {
    const u = queue.shift();
    (adj[u] || []).forEach((v) => {
      if (depth[v] === -1) {
        depth[v] = depth[u] + 1;
        queue.push(v);
      }
    });
  }
  return depth;
}

export default GraphTab;
