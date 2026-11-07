import React, { useState, useEffect, useCallback, useRef } from 'react';
import { getNetInfo, getNetStructure, tgGetEnabled, tgFireTransition } from '../services/tauri-api';
import { useToast } from './Toast';
import useTokenGameGraph from '../hooks/useTokenGameGraph';
import './TokenGame.css';
import { markingToNumbers } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

const MAX_HISTORY = 500;
const PLAY_INTERVAL = 1200;

function markingStr(m) {
  return '[' + m.join(', ') + ']';
}

const TokenGame = ({ filePath, active }) => {
  const [netInfo, setNetInfo] = useState(null);
  const [netStructure, setNetStructure] = useState(null);
  const [marking, setMarking] = useState(null);
  const [enabled, setEnabled] = useState([]);
  const [deadlock, setDeadlock] = useState(false);
  const [history, setHistory] = useState([]);
  const [loading, setLoading] = useState(true);
  const [firing, setFiring] = useState(null);
  const [animating, setAnimating] = useState(false);
  const [playing, setPlaying] = useState(false);
  const playRef = useRef(null);
  const { addToast } = useToast();
  const { t, tm } = useI18n();

  const {
    containerRef,
    graphReady,
    initGraph,
    updateMarking,
    animateAndUpdate,
  } = useTokenGameGraph({ immediateLabel: t('token.immediate') });

  useEffect(() => {
    async function init() {
      if (!filePath) return;
      setLoading(true);
      try {
        const [info, structure] = await Promise.all([
          getNetInfo(filePath),
          getNetStructure(filePath),
        ]);
        setNetInfo(info);
        const normalizedStructure = {
          ...structure,
          places: structure.places?.map((place) => ({
            ...place,
            tokens: markingToNumbers([place.tokens])[0] ?? 0,
          })),
        };
        setNetStructure(normalizedStructure);
        const m = markingToNumbers(info.initial_marking);
        setMarking(m);
        const res = await tgGetEnabled(filePath, m);
        setEnabled(res.enabled);
        setDeadlock(res.deadlock);
        setHistory([]);
      } catch (err) {
        addToast(t('token.loadFailed', { error: tm(err) }), 'error');
      } finally {
        setLoading(false);
      }
    }
    init();
  }, [filePath, addToast, t]);

  // Initialize G6 graph when tab is active, data ready, and container exists
  useEffect(() => {
    if (active && netStructure && marking && containerRef.current) {
      initGraph(netStructure, marking);
    }
  }, [active, netStructure, marking, initGraph, containerRef]);

  const handleFire = useCallback(async (tIdx) => {
    if (!filePath || firing || animating) return;
    if (!enabled.includes(tIdx)) return;
    setFiring(tIdx);
    try {
      const res = await tgFireTransition(filePath, marking, tIdx);
      const nextMarking = markingToNumbers(res.marking);
      setHistory((prev) => {
        const next = [...prev, { transition: tIdx, before: [...marking], after: [...nextMarking] }];
        return next.length > MAX_HISTORY ? next.slice(-MAX_HISTORY) : next;
      });
      if (res.marking.some((v) => v.kind === 'omega')) {
        addToast(t('token.omegaUnsupported'), 'error');
        setFiring(null);
        return;
      }
      if (nextMarking.some((v) => v < 0)) {
        addToast(t('token.negativeMarking'), 'error');
        setFiring(null);
        return;
      }
      setMarking(nextMarking);
      setEnabled(res.enabled);
      setDeadlock(res.deadlock);
      // Trigger animation; keep locked until animation finishes
      if (graphReady) {
        setAnimating(true);
        animateAndUpdate(marking, nextMarking, tIdx, () => {
          setAnimating(false);
        });
        // For non-animation path, unlock after data update
        setFiring(null);
      } else {
        setFiring(null);
      }
    } catch (err) {
      addToast(t('token.failure', { error: tm(err) }), 'error');
      setFiring(null);
    }
  }, [filePath, marking, firing, animating, enabled, addToast, graphReady, animateAndUpdate, t]);

  const handleBack = useCallback(async () => {
    if (history.length === 0 || !filePath) return;
    const newHistory = history.slice(0, -1);
    const prevMarking = newHistory.length > 0
      ? newHistory[newHistory.length - 1].after
      : markingToNumbers(netInfo.initial_marking);
    setHistory(newHistory);
    setMarking(prevMarking);
    try {
      const res = await tgGetEnabled(filePath, prevMarking);
      setEnabled(res.enabled);
      setDeadlock(res.deadlock);
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    }
    if (graphReady) updateMarking(prevMarking);
  }, [history, filePath, netInfo, addToast, graphReady, updateMarking, t]);

  const handleReset = useCallback(async () => {
    if (!filePath || !netInfo) return;
    const initMarking = markingToNumbers(netInfo.initial_marking);
    setHistory([]);
    setMarking(initMarking);
    try {
      const res = await tgGetEnabled(filePath, initMarking);
      setEnabled(res.enabled);
      setDeadlock(res.deadlock);
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    }
    if (graphReady) updateMarking(initMarking);
  }, [filePath, netInfo, addToast, graphReady, updateMarking, t]);

  const playingRef = useRef(false);
  useEffect(() => { playingRef.current = playing; }, [playing]);
  const enabledRef = useRef([]);
  useEffect(() => { enabledRef.current = enabled; }, [enabled]);
  const handleFireRef = useRef(handleFire);
  useEffect(() => { handleFireRef.current = handleFire; }, [handleFire]);

  useEffect(() => {
    if (!playing || deadlock || firing || animating) {
      if (playRef.current) { clearTimeout(playRef.current); playRef.current = null; }
      return;
    }
    const step = async () => {
      if (!playingRef.current) return;
      const currentEnabled = enabledRef.current;
      if (currentEnabled.length === 0) { setPlaying(false); return; }
      const randIdx = Math.floor(Math.random() * currentEnabled.length);
      await handleFireRef.current(currentEnabled[randIdx]);
      if (playingRef.current) playRef.current = setTimeout(step, PLAY_INTERVAL);
    };
    step();
    return () => { if (playRef.current) { clearTimeout(playRef.current); playRef.current = null; } };
  }, [playing]);

  const togglePlay = useCallback(() => {
    if (playing) { setPlaying(false); }
    else {
      if (deadlock) { addToast(t('token.deadlockStop'), 'warning'); return; }
      setPlaying(true);
    }
  }, [playing, deadlock, addToast, t]);

  if (!filePath) return <div className="tg-empty">{t('token.selectFile')}</div>;
  if (loading) return <div className="tg-loading">{t('token.loading')}</div>;
  if (!netInfo) return <div className="tg-empty">{t('token.netLoadFailed')}</div>;

  const maxTokens = Math.max(1, ...(marking || [0]));

  return (
    <div className="tg">
      <div className="tg-bar">
        <div className="tg-bar-left">
          <div className="tg-heading">
            <span className="tg-eyebrow">{t('token.eyebrow')}</span>
            <strong>Token Game</strong>
          </div>
          <div className="tg-state-chip">
            <span>{t('token.currentMarking')}</span>
            <code>{markingStr(marking || [])}</code>
          </div>
          <span className="tg-step">
            {t('token.step')} <strong>{history.length}</strong>
          </span>
          {deadlock && <span className="tg-deadlock-badge">{t('token.deadlock')}</span>}
          {playing && <span className="tg-play-badge">▶ {t('token.playing')}</span>}
        </div>
        <div className="tg-bar-actions">
          <button
            className={`tg-btn tg-play-btn ${playing ? 'playing' : ''}`}
            onClick={togglePlay}
            disabled={deadlock && !playing}
          >
            {playing ? `⏸ ${t('token.pause')}` : `▶ ${t('token.play')}`}
          </button>
          <button className="tg-btn" onClick={handleBack} disabled={history.length === 0}>
            ← {t('token.back')}
          </button>
          <button className="tg-btn" onClick={handleReset} disabled={history.length === 0 && !playing}>
            ↺ {t('token.reset')}
          </button>
        </div>
      </div>

      <div className="tg-workspace">
        <section className="tg-graph-card">
          <div className="tg-graph-header">
            <div>
              <span className="tg-eyebrow">{t('token.runningNet')}</span>
              <h2>{t('token.flow')}</h2>
            </div>
            <div className="tg-graph-meta">
              <span><i className="tg-meta-dot places" />{t('token.placesCount', { count: netInfo.place_names.length })}</span>
              <span><i className="tg-meta-dot transitions" />{t('token.transitionsCount', { count: netInfo.transition_names.length })}</span>
            </div>
          </div>
          <div ref={containerRef} className="tg-canvas" />
          <div className="tg-graph-footer">
            <span>{t('token.dragHint')}</span>
            <span>{t('token.scrollHint')}</span>
          </div>
        </section>

        <aside className="tg-sidebar">
          <section className="tg-panel tg-marking-panel">
            <div className="tg-panel-header">
              <div>
                <span className="tg-eyebrow">{t('token.currentState')}</span>
                <strong>{t('token.places')}</strong>
              </div>
              <span className="tg-panel-count">{netInfo.place_names.length}</span>
            </div>
            <div className="tg-place-list">
              {netInfo.place_names.map((name, i) => (
                <div key={i} className="tg-place">
                  <div className="tg-place-top">
                    <span className="tg-place-name">{name}</span>
                    <span className={`tg-place-val ${marking[i] === 0 ? 'zero' : ''}`}>
                      {marking[i]}
                    </span>
                  </div>
                  <div className="tg-bar-track">
                    <div className="tg-bar-fill" style={{ width: `${Math.max(2, (marking[i] / maxTokens) * 100)}%` }} />
                  </div>
                </div>
              ))}
            </div>
          </section>

          <section className="tg-panel tg-transition-panel">
            <div className="tg-panel-header">
              <div>
                <span className="tg-eyebrow">{t('token.nextAction')}</span>
                <strong>{t('token.transitions')}</strong>
              </div>
              <span className="tg-enabled-badge">{t('token.enabledCount', { count: enabled.length })}</span>
            </div>
            <p className="tg-panel-hint">
              {deadlock ? t('token.noTransitionHint') : t('token.clickHint')}
            </p>
            <div className="tg-trans-grid">
              {netInfo.transition_names.map((name, i) => {
                const isEnabled = enabled.includes(i);
                const isFiring = firing === i;
                return (
                  <button
                    key={i}
                    className={`tg-tbtn ${isEnabled ? 'enabled' : 'disabled'} ${isFiring ? 'firing' : ''}`}
                    onClick={() => handleFire(i)}
                    disabled={!isEnabled || firing !== null || animating || playing}
                    title={isEnabled ? t('token.fire', { name }) : t('token.notEnabled')}
                  >
                    <span className="tg-tbtn-name">{name}</span>
                    <span className="tg-tbtn-idx">t{i}</span>
                  </button>
                );
              })}
            </div>
          </section>
        </aside>
      </div>

      {history.length > 0 && (
        <div className="tg-history">
          <div className="tg-panel-header">
            <div>
              <span className="tg-eyebrow">{t('token.firingSequence')}</span>
              <strong>{t('token.history', { count: history.length })}</strong>
            </div>
          </div>
          <div className="tg-history-list">
            {history.map((h, i) => (
              <div key={i} className="tg-h-item">
                <span className="tg-h-mark">{markingStr(h.before)}</span>
                <span className="tg-h-arrow">→</span>
                <span className="tg-h-trans">{netInfo.transition_names[h.transition] || `t${h.transition}`}</span>
                <span className="tg-h-arrow">→</span>
                <span className="tg-h-mark">{markingStr(h.after)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default TokenGame;
