import Logo from './components/Logo';
import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import './App.css';
import { ThemeProvider } from './contexts/ThemeContext';
import { ToastProvider } from './components/Toast';
import { useToast } from './components/Toast';
import ErrorBoundary from './components/ErrorBoundary';
import SettingsMenu from './components/SettingsMenu';
import { useI18n } from './i18n/I18nContext';
import FileUpload from './components/FileUpload';
import BadStatesEditor from './components/BadStatesEditor';
import PropertyTable from './components/PropertyTable';
import PetriNetGraph from './components/PetriNetGraph';
import GraphTab from './components/GraphTab';
import Verification from './components/Verification';
import AdvancedMethods from './components/AdvancedMethods';
import { queryGate } from './components/BoundedGate';
import { deriveGraphProperties } from './services/graph-properties';
import TokenGame from './components/TokenGame';
import InvariantsViewer from './components/InvariantsViewer';
import MatrixPanel from './components/MatrixPanel';
import VectorTest from './components/VectorTest';
import AnalysisOverview from './components/AnalysisOverview';
import InvariantOverview from './components/InvariantOverview';
import EvidenceViewer from './components/EvidenceViewer';
import DiagnosticsPanel from './components/DiagnosticsPanel';
import MethodologyView from './components/MethodologyView';
import { logEvent } from './services/app-logger';
import { getNetStructure, getRequiredInitialProperties, parseMercuryText, analyzePnt, getInvariants, computeBehavioralProperties, getGraphCacheStatus } from './services/tauri-api';
import { loadGraphGenerationConfig } from './services/graph-config';

const TABS = [
  { id: 'analysis', labelKey: 'tabs.analysis' },
  { id: 'graphs', labelKey: 'tabs.graphs' },
  { id: 'invariants', labelKey: 'tabs.invariants' },
  { id: 'evidence', labelKey: 'tabs.evidence' },
  { id: 'verify', labelKey: 'tabs.verify', needsGraph: true },
  { id: 'advanced', labelKey: 'tabs.advanced' },
  { id: 'tokens', labelKey: 'tabs.tokens' },
];

function AppContent() {
  const [selectedFile, setSelectedFile] = useState(null);
  const [analysisResult, setAnalysisResult] = useState(null);
  const [requiredProperties, setRequiredProperties] = useState(null);
  const [badStates, setBadStates] = useState(null);
  const [requiredPropertiesLoading, setRequiredPropertiesLoading] = useState(false);
  const requiredPropertiesFileRef = useRef(null);
  const gate = queryGate(requiredProperties, requiredPropertiesLoading, Boolean(selectedFile));
  const [referencesOpen, setReferencesOpen] = useState(false);
  const closeReferences = useCallback(() => setReferencesOpen(false), []);
  const [activeTab, setActiveTab] = useState('analysis');
  const [visitedTabs, setVisitedTabs] = useState(new Set(['analysis']));
  const [ctlResult, setCtlResult] = useState(null);
  const [invariantInfo, setInvariantInfo] = useState(null);
  const [welcomeMode, setWelcomeMode] = useState('file');
  const [graphProps, setGraphProps] = useState(null);
  const [behavior, setBehavior] = useState(null);
  const [behaviorLoading, setBehaviorLoading] = useState(false);
  const [netStructure, setNetStructure] = useState(null);
  const [netStructureLoading, setNetStructureLoading] = useState(false);
  const [mercuryScript, setMercuryScript] = useState('');
  const [mercuryLoading, setMercuryLoading] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const { addToast } = useToast();
  const { t, tm } = useI18n();

  useEffect(() => {
    logEvent('INFO', 'frontend', 'application.ui_ready', 'Interface carregada');
    const onError = (event) => logEvent('ERROR', 'frontend', 'window.error', event.message || 'Erro JavaScript não tratado');
    const onRejection = (event) => logEvent('ERROR', 'frontend', 'promise.unhandled_rejection', event.reason?.message ?? event.reason ?? 'Promise rejeitada sem tratamento');
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);
    return () => {
      window.removeEventListener('error', onError);
      window.removeEventListener('unhandledrejection', onRejection);
    };
  }, []);

  const handleTabClick = (tab) => {
    setActiveTab(tab);
    logEvent('INFO', 'frontend', 'tab.activated', `Aba ativada: ${tab}`);
    setVisitedTabs((prev) => {
      if (prev.has(tab)) return prev;
      return new Set([...prev, tab]);
    });
  };

  useEffect(() => {
    if (!selectedFile) return;
    setNetStructureLoading(true);
    getNetStructure(selectedFile)
      .then(setNetStructure)
      .catch(() => {})
      .finally(() => setNetStructureLoading(false));
  }, [selectedFile]);

  // B / L / S need the saved graphs; the summary is computed again once they exist.
  const [graphStatus, setGraphStatus] = useState({ reachability: false, coverability: false });
  const graphReady = graphStatus.reachability || graphStatus.coverability;
  const graphKey = `${graphStatus.reachability}-${graphStatus.coverability}`;

  useEffect(() => {
    if (!selectedFile || !analysisResult) return undefined;
    const filePath = selectedFile;
    const runKey = `${filePath}|${graphKey}`;
    if (requiredPropertiesFileRef.current === runKey) return undefined;
    let current = true;
    const sameFile = requiredPropertiesFileRef.current?.startsWith(`${filePath}|`);
    requiredPropertiesFileRef.current = runKey;
    if (!sameFile) setRequiredProperties(null);
    setRequiredPropertiesLoading(true);
    getRequiredInitialProperties(filePath)
      .then((result) => { if (current) setRequiredProperties(result); })
      .catch((error) => { if (current) setRequiredProperties({ warning: String(error) }); })
      .finally(() => { if (current) setRequiredPropertiesLoading(false); });
    return () => { current = false; };
  }, [selectedFile, analysisResult, graphKey]);

  const handleFileSelect = (file) => {
    if (file !== selectedFile) {
      setAnalysisResult(null);
      setRequiredProperties(null);
      setBadStates(null);
      setRequiredPropertiesLoading(false);
      requiredPropertiesFileRef.current = null;
      setNetStructure(null);
      setCtlResult(null);
      setInvariantInfo(null);
      setGraphProps(null);
      setBehavior(null);
      setGraphStatus({ reachability: false, coverability: false });
      setActiveTab('analysis');
      setVisitedTabs(new Set(['analysis']));
    }
    setSelectedFile(file);
    logEvent('INFO', 'frontend', 'file.selected', `Arquivo selecionado: ${file?.split('/').pop() ?? 'desconhecido'}`);
  };

  // Behavioural properties (liveness, SCCs, conflicts…) need the complete reachability graph,
  // so they are filled in automatically once the Graphs tab has produced one.
  const behaviorRunRef = useRef(false);
  const selectedFileRef = useRef(selectedFile);
  selectedFileRef.current = selectedFile;
  const computeBehavior = useCallback(async (file) => {
    if (!file || behaviorRunRef.current) return;
    behaviorRunRef.current = true;
    setBehaviorLoading(true);
    try {
      const result = await computeBehavioralProperties(file, loadGraphGenerationConfig());
      if (selectedFileRef.current === file) setBehavior(result);
    } catch (err) {
      logEvent('ERROR', 'frontend', 'behavior.failed', String(err));
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      behaviorRunRef.current = false;
      setBehaviorLoading(false);
    }
  }, [addToast, t, tm]);

  const handleGraphProperties = useCallback((graph) => {
    const transitionIds = (netStructure?.transitions || []).map((tr) => tr.id);
    const derived = deriveGraphProperties(graph.nodes, graph.edges, transitionIds);
    if (!derived) return;
    const names = new Map((netStructure?.transitions || []).map((tr) => [tr.id, tr.name]));
    setGraphProps({
      num_states: graph.total_nodes ?? graph.nodes.length,
      deadlock_free: derived.deadlockFree,
      deadlock_nodes: derived.deadlockNodes.map((id) => `M${id}`),
      dead_transitions: derived.deadTransitions.map((id) => names.get(id) ?? `t${id}`),
      reversible: derived.reversible,
      non_reversible_nodes: derived.nonReversibleNodes.map((id) => `M${id}`),
    });
  }, [netStructure]);

  // Which graphs are saved for this net (they survive restarts); asked again after each generation.
  const refreshGraphStatus = useCallback(async (file) => {
    if (!file) return;
    try {
      const status = await getGraphCacheStatus(file);
      if (selectedFileRef.current === file) setGraphStatus(status);
    } catch (error) {
      logEvent('ERROR', 'frontend', 'graph.status.failed', `Estado do cache de grafos indisponível: ${String(error)}`);
    }
  }, []);

  useEffect(() => {
    setGraphStatus({ reachability: false, coverability: false });
    refreshGraphStatus(selectedFile);
  }, [selectedFile, refreshGraphStatus]);

  // The behavioural properties are read from the saved reachability graph as soon as it exists.
  useEffect(() => {
    if (selectedFile && graphStatus.reachability) computeBehavior(selectedFile);
  }, [selectedFile, graphStatus.reachability, computeBehavior]);

  // CPI / CTI come from the invariants, which the first analysis does not compute.
  useEffect(() => {
    if (!selectedFile || !analysisResult) return undefined;
    let current = true;
    getInvariants(selectedFile)
      .then((inv) => { if (current) setInvariantInfo(inv); })
      .catch(() => {});
    return () => { current = false; };
  }, [selectedFile, analysisResult]);

  // Transition names by id, so graphs can show real names instead of t0, t1…
  const transitionNames = useMemo(() => {
    const names = [];
    (netStructure?.transitions || []).forEach((tr) => { names[tr.id] = tr.name; });
    return names;
  }, [netStructure]);

  // Later, more complete sources fill what the first (structural) analysis left undecided.
  const propertyData = (() => {
    if (!analysisResult) return analysisResult;
    let data = { ...analysisResult };
    const states = behavior?.state_count ?? requiredProperties?.state_count ?? graphProps?.num_states;
    if (data.analysis_info?.num_states == null && states != null) {
      data.analysis_info = { ...data.analysis_info, num_states: states };
    }
    if (invariantInfo && !data.invariant) data.invariant = invariantInfo;
    if (graphProps) {
      data = {
        ...data,
        deadlock_free: data.deadlock_free ?? graphProps.deadlock_free,
        deadlock_nodes: data.deadlock_free == null ? graphProps.deadlock_nodes : data.deadlock_nodes,
        dead_transitions: data.dead_transitions ?? graphProps.dead_transitions,
        reversible: data.reversible ?? graphProps.reversible,
        non_reversible_nodes: data.reversible == null ? graphProps.non_reversible_nodes : data.non_reversible_nodes,
      };
    }
    if (behavior) {
      data = {
        ...data,
        deadlock_free: behavior.deadlock_free,
        deadlock_nodes: behavior.deadlock_nodes,
        dead_transitions: behavior.dead_transitions,
        reversible: behavior.reversible,
        live: data.live ?? behavior.live,
        place_live: behavior.place_live,
        safe: data.safe ?? behavior.safe,
        persistent: behavior.persistent,
        gfc: behavior.gfc,
        dynamic_conflict_free: behavior.dynamic_conflict_free,
        liveness: behavior.liveness,
        scc: behavior.scc,
        conflicts: behavior.conflicts,
        place_liveness: behavior.place_liveness,
        analysis_info: { ...data.analysis_info, num_states: behavior.state_count },
      };
    }
    return data;
  })();

  const handleGoHome = () => {
    if (!analysisResult && !selectedFile) return;
    setSelectedFile(null);
    setAnalysisResult(null);
    setRequiredProperties(null);
    setRequiredPropertiesLoading(false);
    requiredPropertiesFileRef.current = null;
    setBadStates(null);
    setNetStructure(null);
    setCtlResult(null);
    setInvariantInfo(null);
    setGraphProps(null);
    setBehavior(null);
    setActiveTab('analysis');
    setVisitedTabs(new Set(['analysis']));
    setWelcomeMode('file');
    logEvent('INFO', 'frontend', 'home.opened', 'Retorno à tela inicial');
  };

  const handleAnalysisComplete = (result, sourceFile) => {
    if (sourceFile && sourceFile !== selectedFile) return;
    requiredPropertiesFileRef.current = null;
    setRequiredProperties(null);
    setRequiredPropertiesLoading(false);
    logEvent('INFO', 'frontend', 'analysis.displayed', 'Resultado da análise exibido');
    setAnalysisResult(result);
    setActiveTab('analysis');
    setVisitedTabs(new Set(['analysis']));
    setCtlResult(null);
    setInvariantInfo(null);
    setGraphProps(null);
    setBehavior(null);
  };

  const handleMercuryParse = useCallback(async () => {
    if (!mercuryScript.trim() || mercuryLoading) return;
    setMercuryLoading(true);
    try {
      const path = await parseMercuryText(mercuryScript);
      handleFileSelect(path);
      setMercuryScript('');
      // Auto-analyze after Mercury parse
      const result = await analyzePnt(path);
      handleAnalysisComplete(result);
      const toastType = result.analysis_status === 'structural_analysis_complete'
        ? 'info'
        : result.analysis_message ? 'warning' : 'success';
      addToast(tm(result.analysis_message) || t('upload.success'), toastType, 7000);
    } catch (err) {
      addToast(t('app.mercuryError', { error: tm(err) }), 'error');
    } finally {
      setMercuryLoading(false);
    }
  }, [mercuryScript, mercuryLoading, addToast, handleAnalysisComplete, handleFileSelect, t]);

  return (
    <div className="app">
      <header className="header">
        {analysisResult ? (
          <button type="button" className="header-brand header-home" onClick={handleGoHome} title={t('app.home')} aria-label={t('app.home')}>
            <Logo size={26} className="header-logo" />
            <span className="header-title">SPECTRA</span>
            <svg className="header-home-icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M2 7.5 8 2.5l6 5M3.5 6.5V13h3.5V9.5h2V13h3.5V6.5" />
            </svg>
          </button>
        ) : (
          <div className="header-brand" title={t("app.fullName")}>
            <Logo size={26} className="header-logo" />
            <span className="header-title">SPECTRA</span>
          </div>
        )}
        {analysisResult && (
          <nav className="tabs" role="tablist" aria-label={t('app.sections')}>
            {TABS.map((tab) => (
              <button
                key={tab.id}
                role="tab"
                aria-selected={activeTab === tab.id}
                className={`tab-btn ${activeTab === tab.id ? 'active' : ''}`}
                onClick={() => handleTabClick(tab.id)}
                disabled={tab.needsGraph && !graphReady}
                title={tab.needsGraph && !graphReady ? t('tabs.needsGraph') : undefined}
              >
                {t(tab.labelKey)}
              </button>
            ))}
          </nav>
        )}
        <div className="header-spacer" />
        {analysisResult && (
          <div className="header-file-control">
            <FileUpload
              value={selectedFile}
              onAnalysisComplete={handleAnalysisComplete}
              onFileSelect={handleFileSelect}
            />
          </div>
        )}
        <div className="header-end">
          <SettingsMenu onOpenLogs={() => setDiagnosticsOpen(true)} onOpenReferences={() => setReferencesOpen(true)} />
        </div>
      </header>

      {referencesOpen && (
        <main className="content content-references">
          <ErrorBoundary><MethodologyView onClose={closeReferences} /></ErrorBoundary>
        </main>
      )}

      {analysisResult && (
        <>
          <main className="content" hidden={referencesOpen}>
            {visitedTabs.has('analysis') && (
              <div role="tabpanel" hidden={activeTab !== 'analysis'}>
                <ErrorBoundary>
                  <div className="analysis-screen">
                    <AnalysisOverview data={propertyData} requiredProperties={requiredProperties} pending={requiredPropertiesLoading} />
                    <div className="analysis-grid">
                      <section className="analysis-left panel" aria-label={t('app.networkView')}>
                        <div className="panel-header">
                          <div>
                            <span className="panel-kicker">{t('app.structure')}</span>
                            <span className="panel-title">{t('app.originalNet')}</span>
                          </div>
                          <span className="panel-hint">{t('app.graphHint')}</span>
                        </div>
                        {netStructureLoading ? (
                          <div className="panel-loading">{t('app.loadingStructure')}</div>
                        ) : (
                          <PetriNetGraph data={netStructure} />
                        )}
                      </section>

                      <aside className="analysis-right" aria-label={t('app.netProperties')}>
                        <div className="analysis-properties-header">
                          <div>
                            <h2>{t('app.properties')}</h2>
                          </div>
                        </div>
                        <PropertyTable
                          data={propertyData}
                          filePath={selectedFile}
                          requiredProperties={requiredProperties}
                          requiredPropertiesLoading={requiredPropertiesLoading}
                          badStates={badStates}
                          placeNames={netStructure?.places?.map((p) => p.name)}
                          behavioral={{ computed: Boolean(behavior), loading: behaviorLoading }}
                        />
                      </aside>
                    </div>
                    <BadStatesEditor
                      filePath={selectedFile}
                      onResult={setBadStates}
                      extended={Boolean(netStructure?.extended)}
                      hasImmediate={Boolean(netStructure?.transitions?.some((tr) => tr.immediate))}
                    />
                  </div>
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('graphs') && (
              <div role="tabpanel" hidden={activeTab !== 'graphs'}>
                <ErrorBoundary>
                  <GraphTab
                    filePath={selectedFile}
                    analysisData={analysisResult}
                    bounded={requiredProperties?.bounded ?? analysisResult?.bounded ?? null}
                    onGraphProperties={handleGraphProperties}
                    onGraphGenerated={() => refreshGraphStatus(selectedFile)}
                    transitionNames={transitionNames}
                    extended={Boolean(netStructure?.extended)}
                  />
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('invariants') && (
              <div role="tabpanel" hidden={activeTab !== 'invariants'}>
                <ErrorBoundary>
                  <div className="invariants-layout">
                    <InvariantOverview analysisData={analysisResult} invariant={invariantInfo} />
                    <div className="inv-page">
                      <div className="inv-page-main">
                        <InvariantsViewer
                          filePath={selectedFile}
                          analysisData={analysisResult}
                          onLoaded={setInvariantInfo}
                        />
                      </div>
                      <div className="inv-page-tools">
                        <MatrixPanel filePath={selectedFile} initialMarking={analysisResult?.initial_marking} />
                        <VectorTest filePath={selectedFile} />
                      </div>
                    </div>
                  </div>
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('verify') && graphReady && (
              <div role="tabpanel" hidden={activeTab !== 'verify'}>
                <ErrorBoundary>
                  <Verification
                    filePath={selectedFile}
                    gate={gate}
                    ctlResult={ctlResult}
                    onCtlResult={setCtlResult}
                    extended={Boolean(netStructure?.extended)}
                    placeNames={netStructure?.places?.map((p) => p.name)}
                    transitionNames={netStructure?.transitions?.map((tr) => tr.name)}
                  />
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('advanced') && (
              <div role="tabpanel" hidden={activeTab !== 'advanced'}>
                <ErrorBoundary>
                  <AdvancedMethods
                    filePath={selectedFile}
                    extended={Boolean(netStructure?.extended)}
                    placeNames={netStructure?.places?.map((p) => p.name)}
                  />
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('evidence') && (
              <div role="tabpanel" hidden={activeTab !== 'evidence'}>
                <ErrorBoundary>
                  <EvidenceViewer analysisData={analysisResult} filePath={selectedFile} graphKey={graphKey} graphReady={graphReady} />
                </ErrorBoundary>
              </div>
            )}

            {visitedTabs.has('tokens') && (
              <div role="tabpanel" hidden={activeTab !== 'tokens'}>
                <ErrorBoundary>
                  <TokenGame filePath={selectedFile} active={activeTab === 'tokens'} />
                </ErrorBoundary>
              </div>
            )}
          </main>
        </>
      )}

      {diagnosticsOpen && <DiagnosticsPanel onClose={() => setDiagnosticsOpen(false)} />}

      {!analysisResult && (
        <main className="content welcome" hidden={referencesOpen}>
          <section className="welcome-intro">
            <div className="welcome-brand">
              <Logo size={48} />
              <h1 className="welcome-title">SPECTRA</h1>
            </div>
            <p className="welcome-lead">{t('app.lead')}</p>
            <ul className="welcome-features">
              {['structure', 'invariants', 'statespace', 'verify', 'play'].map((k) => (
                <li key={k}>
                  <strong>{t(`app.features.${k}.title`)}</strong>
                  <span>{t(`app.features.${k}.text`)}</span>
                </li>
              ))}
            </ul>
            <p className="welcome-fullname">{t('app.fullName')}</p>
          </section>

          <section className="welcome-start" aria-label={t('app.start')}>
            <h2 className="welcome-start-title">{t('app.startTitle')}</h2>
            <div className="welcome-switch" role="tablist" aria-label={t('app.startTitle')}>
              {['file', 'mercury'].map((m) => (
                <button
                  key={m}
                  type="button"
                  role="tab"
                  aria-selected={welcomeMode === m}
                  className={welcomeMode === m ? 'on' : ''}
                  onClick={() => setWelcomeMode(m)}
                >
                  {t(`app.mode.${m}`)}
                </button>
              ))}
            </div>

            {welcomeMode === 'file' ? (
              <FileUpload
                variant="hero"
                value={selectedFile}
                onAnalysisComplete={handleAnalysisComplete}
                onFileSelect={handleFileSelect}
              />
            ) : (
              <div className="welcome-mercury">
                <label className="welcome-label" htmlFor="mercury-script">{t('app.mercuryLabel')}</label>
                <textarea
                  id="mercury-script"
                  className="welcome-textarea"
                  placeholder={`${t('app.mercuryPlaceholder')}\n\n${t('app.example')}:\nSPN Model{\n  place P0;\n  place P1;\n\n  timedTransition T0(\n    inputs = [P0],\n    outputs = [P1],\n    delay = 0.0\n  );\n}\n\nmain {\n}`}
                  value={mercuryScript}
                  onChange={(e) => setMercuryScript(e.target.value)}
                  rows={14}
                  spellCheck={false}
                />
                <button
                  className="welcome-parse-btn"
                  onClick={handleMercuryParse}
                  disabled={!mercuryScript.trim() || mercuryLoading}
                >
                  {mercuryLoading ? t('app.processing') : t('app.parseAnalyze')}
                </button>
              </div>
            )}
          </section>
        </main>
      )}
    </div>
  );
}

function App() {
  return (
    <ThemeProvider>
      <ToastProvider>
        <AppContent />
      </ToastProvider>
    </ThemeProvider>
  );
}

export default App;
