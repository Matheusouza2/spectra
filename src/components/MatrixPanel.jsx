import React, { useState, useEffect, useCallback, useRef } from 'react';
import { getIncidenceMatrix } from '../services/tauri-api';
import { useToast } from './Toast';
import './MatrixPanel.css';
import { tokenValueToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

function MatrixTable({ places, transitions, matrix, initialMarking, full = false }) {
  const { t } = useI18n();
  return (
      <div className={`mat-table-wrap ${full ? 'full' : ''}`}>
        <table className="mat-table">
          <thead>
            <tr>
              <th className="mat-corner"></th>
              {transitions.map((tName, j) => (
                <th key={j} className="mat-col-header" title={t('matrix.transitionTitle', { name: tName })}>
                  <span className="mat-col-name">{tName}</span>
                  <span className="mat-col-idx">t{j}</span>
                </th>
              ))}
              <th className="mat-col-header mat-col-m0" title={t('matrix.initialMarking')}>
                <span className="mat-col-name">M₀</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {matrix.map((row, i) => (
              <tr key={i}>
                <th className="mat-row-header" title={t('matrix.placeTitle', { name: places[i] })}>
                  <span className="mat-row-name">{places[i]}</span>
                  <span className="mat-row-idx">p{i}</span>
                </th>
                {row.map((rawVal, j) => {
                  const val = BigInt(rawVal);
                  return (
                  <td
                    key={j}
                    className={`mat-cell ${val > 0n ? 'pos' : val < 0n ? 'neg' : 'zero'}`}
                    title={`${places[i] ?? `p${i}`} ↔ ${transitions[j] ?? `t${j}`}: ${val > 0n ? '+' : ''}${val}`}
                  >
                    {val > 0n ? `+${val}` : String(val)}
                  </td>
                  );
                })}
                <td className="mat-cell mat-m0-cell">
                  {initialMarking?.[i] ? tokenValueToString(initialMarking[i]) : '—'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
  );
}

function ExpandIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M9.5 2.5h4v4M6.5 13.5h-4v-4M13.5 2.5 9 7M2.5 13.5 7 9" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
      <path d="m3.5 3.5 9 9M12.5 3.5l-9 9" />
    </svg>
  );
}

/** Full-size matrix in a native modal dialog (Esc closes, focus is trapped by the browser). */
function MatrixDialog({ open, onClose, children, title }) {
  const ref = useRef(null);
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      className="mat-dialog"
      aria-label={title}
      onClose={onClose}
      onClick={(e) => { if (e.target === ref.current) onClose(); }}
    >
      {open && children}
    </dialog>
  );
}

/**
 * MatrixPanel — shows incidence matrix W = Post - Pre
 *
 * Features:
 * - HTML table with place rows × transition columns
 * - Color-coded cells: green (positive), red (negative), neutral (zero)
 * - State equation explanation: M' = M + W·σ
 * - Row/column headers with place/transition names
 */
const MatrixPanel = ({ filePath, initialMarking }) => {
  const [matrixData, setMatrixData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const { addToast } = useToast();
  const { t, tm } = useI18n();

  const fetchMatrix = useCallback(async () => {
    if (!filePath) return;
    setLoading(true);
    try {
      const data = await getIncidenceMatrix(filePath);
      setMatrixData(data);
    } catch (err) {
      addToast(t('matrix.loadFailed', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, addToast, t]);

  useEffect(() => {
    fetchMatrix();
  }, [filePath]);

  if (!filePath) {
    return (
      <div className="mat-empty">
        {t('matrix.selectFile')}
      </div>
    );
  }

  if (loading) {
    return <div className="mat-loading">{t('matrix.calculating')}</div>;
  }

  if (!matrixData) {
    return (
      <div className="mat-empty">
        <span>{t('matrix.unavailable')}</span>
        <button className="inv-recalc-btn" onClick={fetchMatrix}>
          {t('matrix.calculate')}
        </button>
      </div>
    );
  }

  const { places, transitions, matrix } = matrixData;
  const numPlaces = places?.length || 0;
  const numTrans = transitions?.length || 0;

  return (
    <div className="mat-layout">
      <div className="mat-section">
        <div className="mat-section-header">
          <span className="mat-section-icon">⟐</span>
          <span className="mat-section-title">{t('matrix.title')}</span>
          <span className="mat-dims">{numPlaces}×{numTrans}</span>
          <button
            type="button"
            className="mat-expand-btn"
            data-tooltip={t('matrix.expand')}
            aria-label={t('matrix.expand')}
            onClick={() => setExpanded(true)}
            disabled={!matrix || matrix.length === 0}
          >
            <ExpandIcon />
          </button>
          <div className="mat-legend" aria-label={t('matrix.legend')}>
            <span className="mat-legend-item">
              <span className="mat-legend-swatch pos" /> {t('matrix.positive')}
            </span>
            <span className="mat-legend-item">
              <span className="mat-legend-swatch neg" /> {t('matrix.negative')}
            </span>
            <span className="mat-legend-item">
              <span className="mat-legend-swatch zero" /> {t('matrix.zero')}
            </span>
          </div>
        </div>
        <div className="mat-section-body">
          <p className="mat-desc">
            <strong>W</strong> = <strong>Post</strong> − <strong>Pre</strong>. {t('matrix.cellDesc')}
          </p>

          {matrix && matrix.length > 0 ? (
            <MatrixTable places={places} transitions={transitions} matrix={matrix} initialMarking={initialMarking} />
          ) : (
            <p className="mat-empty-inline">{t('matrix.empty')}</p>
          )}
        </div>
      </div>

      <MatrixDialog open={expanded} onClose={() => setExpanded(false)} title={t('matrix.title')}>
        <div className="mat-dialog-head">
          <h2>{t('matrix.title')} <span className="mat-dims">{numPlaces}×{numTrans}</span></h2>
          <div className="mat-legend" aria-label={t('matrix.legend')}>
            <span className="mat-legend-item"><span className="mat-legend-swatch pos" /> {t('matrix.positive')}</span>
            <span className="mat-legend-item"><span className="mat-legend-swatch neg" /> {t('matrix.negative')}</span>
            <span className="mat-legend-item"><span className="mat-legend-swatch zero" /> {t('matrix.zero')}</span>
          </div>
          <button type="button" className="mat-expand-btn" data-tooltip={t('matrix.collapse')} aria-label={t('matrix.collapse')} onClick={() => setExpanded(false)}>
            <CloseIcon />
          </button>
        </div>
        <MatrixTable places={places} transitions={transitions} matrix={matrix} initialMarking={initialMarking} full />
      </MatrixDialog>
    </div>
  );
};

export default MatrixPanel;
