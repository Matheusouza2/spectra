import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { getInvariants, getNetInfo } from '../services/tauri-api';
import { useToast } from './Toast';
import { useI18n } from '../i18n/I18nContext';
import './InvariantsViewer.css';

const algebraicValue = (value) => BigInt(value);

const toBig = (value) => BigInt(value);

/** Sparse view of a semiflow: only entries with a non-zero weight. */
function support(vector, names, fallback) {
  return vector
    .map((v, i) => ({ weight: toBig(v), index: i, name: names?.[i] || `${fallback}${i}` }))
    .filter((e) => e.weight !== 0n);
}

function Term({ term, showSign, tone }) {
  const abs = term.weight < 0n ? -term.weight : term.weight;
  return (
    <span className="inv-term">
      {showSign && <span className="inv-op">{term.weight < 0n ? '−' : '+'}</span>}
      <span className={`inv-chip ${tone}`}>
        {abs !== 1n && <b className="inv-coeff">{String(abs)}</b>}
        <span className="inv-chip-name">{term.name}</span>
      </span>
    </span>
  );
}

function Coverage({ names, covered, tone, label }) {
  const missing = names.filter((_, i) => !covered.has(i));
  return (
    <div className="inv-coverage">
      <div className="inv-coverage-strip" role="img" aria-label={`${label}: ${names.length - missing.length}/${names.length}`}>
        {names.map((name, i) => (
          <span key={i} className={`inv-cell ${covered.has(i) ? `on ${tone}` : ''}`} title={`${name}${covered.has(i) ? '' : ' ✗'}`} />
        ))}
      </div>
      <p className="inv-coverage-text">
        <b>{names.length - missing.length}</b> / {names.length} {label}
        {missing.length > 0 && <span className="inv-missing"> — {missing.slice(0, 8).join(', ')}{missing.length > 8 ? '…' : ''}</span>}
      </p>
    </div>
  );
}


/** One-sided invariants: sub (non-increasing, ≤) and sur (non-decreasing, ≥). */
function Inequality({ items, names, effectNames, fallback, kind, rel, tone, idPrefix, t }) {
  if (!Array.isArray(items)) return null;
  if (items.length === 0) {
    return (
      <div className="inv-oneside">
        <h3 className="inv-oneside-title">
          <span className="inv-rel" aria-hidden="true">{rel}</span>
          {t(`inv.${kind}Title`)} <span className="inv-count">0</span>
        </h3>
        <p className="inv-oneside-desc">{t('inv.noOneSide')}</p>
      </div>
    );
  }
  return (
    <div className="inv-oneside">
      <h3 className="inv-oneside-title">
        <span className="inv-rel" aria-hidden="true">{rel}</span>
        {t(`inv.${kind}Title`)} <span className="inv-count">{items.length}</span>
      </h3>
      <p className="inv-oneside-desc">{t(`inv.${idPrefix}${kind}Desc`)}</p>
      <ol className="inv-list">
        {items.map((item, i) => {
          const set = support(item.coefficients, names, fallback);
          const effect = idPrefix === 't' ? support(item.residual, effectNames, 'p') : [];
          return (
            <li key={i} className="inv-item">
              <span className="inv-id">{kind === 'sub' ? 'σ' : 'ς'}<sub>{i + 1}</sub></span>
              <div className="inv-eq-wrap">
                <div className="inv-eq" role="math">
                  {idPrefix === 'p'
                    ? set.map((e, k) => <Term key={e.index} term={e} showSign={k > 0 || e.weight < 0n} tone={tone} />)
                    : set.map((e, k) => (
                      <span key={e.index} className="inv-seq">
                        {k > 0 && <span className="inv-arrow" aria-hidden="true">→</span>}
                        <span className={`inv-chip ${tone}`}>{e.weight > 1n && <b className="inv-coeff">{String(e.weight)}×</b>}<span className="inv-chip-name">{e.name}</span></span>
                      </span>
                    ))}
                  {idPrefix === 'p' && (
                    <>
                      <span className="inv-op">{rel}</span>
                      <span className="inv-const">{item.initial_value ?? t('inv.constant')}</span>
                    </>
                  )}
                </div>
                {idPrefix === 't' && (
                  <p className="inv-effect">
                    {t('inv.netEffect')}:{' '}
                    {effect.length === 0
                      ? '0'
                      : effect.map((e) => (
                        <span key={e.index} className={`inv-delta ${e.weight < 0n ? 'neg' : 'pos'}`}>
                          {e.name} {e.weight < 0n ? '−' : '+'}{String(e.weight < 0n ? -e.weight : e.weight)}
                        </span>
                      ))}
                  </p>
                )}
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

/** Basis of ALL invariants: spans every integer invariant, including ones with negative entries. */
function Basis({ basis, names, fallback, tone, t }) {
  if (!basis || !Array.isArray(basis.vectors)) return null;
  return (
    <details className="inv-basis">
      <summary>{t('inv.basisSummary', { count: basis.vectors.length, rank: basis.rank })}</summary>
      <p className="inv-basis-desc">{t('inv.basisDesc', { dimension: basis.dimension })}</p>
      {basis.vectors.length === 0 ? <p className="inv-none">{t('inv.basisEmpty')}</p> : (
        <ol className="inv-list">
          {basis.vectors.map((vector, i) => {
            const set = support(vector, names, fallback);
            return (
              <li key={i} className="inv-item">
                <span className="inv-id">b<sub>{i + 1}</sub></span>
                <div className="inv-eq" role="math" aria-label={set.map((e) => `${e.weight} ${e.name}`).join(', ')}>
                  {set.map((e, k) => <Term key={e.index} term={e} showSign={k > 0 || e.weight < 0n} tone={tone} />)}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </details>
  );
}

function Verdict({ value, t }) {
  if (typeof value !== 'boolean') return <span className="inv-verdict na">—</span>;
  return value
    ? <span className="inv-verdict yes"><span aria-hidden="true">✓</span> {t('common.yes')}</span>
    : <span className="inv-verdict no"><span aria-hidden="true">✗</span> {t('common.no')}</span>;
}

function Section({ id, title, count, coverage, verdictLabel, verdict, strip, children, help, t }) {
  return (
    <section className="inv-section" aria-labelledby={id}>
      <header className="inv-section-head">
        <h2 id={id}>{title} <span className="inv-count">{count}</span></h2>
        <div className="inv-section-verdict"><span>{verdictLabel}</span><Verdict value={verdict} t={t} /></div>
      </header>
      {strip}
      {children}
      <details className="inv-help">
        <summary>{help.summary}</summary>
        <p>{help.text}</p>
      </details>
    </section>
  );
}

const InvariantsViewer = ({ filePath, analysisData, onLoaded }) => {
  const [invData, setInvData] = useState(null);
  const [netInfo, setNetInfo] = useState(null);
  const [loading, setLoading] = useState(false);
  const { addToast } = useToast();
  const { t, tm } = useI18n();

  const fetchData = useCallback(async () => {
    if (!filePath) return;
    setLoading(true);
    try {
      const [inv, info] = await Promise.all([getInvariants(filePath), getNetInfo(filePath)]);
      setInvData(inv);
      setNetInfo(info);
      onLoaded?.(inv);
    } catch (err) {
      addToast(t('inv.loadFailed', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [filePath, addToast, t]);

  useEffect(() => { fetchData(); }, [filePath]);

  const inv = invData || analysisData?.invariant;
  const placeNames = netInfo?.place_names;
  const transNames = netInfo?.transition_names;

  const pSets = useMemo(() => (inv?.minimal_p_semiflows || []).map((v) => support(v, placeNames, 'p')), [inv, placeNames]);
  const tSets = useMemo(() => (inv?.minimal_t_semiflows || []).map((v) => support(v, transNames, 't')), [inv, transNames]);
  const pCovered = useMemo(() => new Set(pSets.flatMap((s) => s.map((e) => e.index))), [pSets]);
  const tCovered = useMemo(() => new Set(tSets.flatMap((s) => s.map((e) => e.index))), [tSets]);

  if (!filePath) return <p className="inv-empty">{t('inv.selectFile')}</p>;
  if (loading) return <p className="inv-empty" role="status">{t('inv.loading')}</p>;
  if (!inv) {
    return (
      <div className="inv-empty">
        <p>{t('inv.unavailable')}</p>
        <button type="button" className="inv-recalc-btn" onClick={fetchData}>{t('inv.calculate')}</button>
      </div>
    );
  }

  const pNames = placeNames || (inv.minimal_p_semiflows?.[0] || []).map((_, i) => `p${i}`);
  const tNames = transNames || (inv.minimal_t_semiflows?.[0] || []).map((_, i) => `t${i}`);
  const unitWeights = pSets.some((s) => s.every((e) => e.weight === 1n));

  return (
    <div className="inv-layout">
      <Section
        id="inv-p" t={t} title={t('inv.pSection')} count={pSets.length}
        verdictLabel={t('inv.pCoverage')} verdict={inv.p_invariant_coverage}
        strip={pNames.length > 0 && <Coverage names={pNames} covered={pCovered} tone="p" label={t('inv.places')} />}
        help={{ summary: t('inv.whatIs'), text: t('inv.pDesc') }}
      >
        {pSets.length === 0 ? <p className="inv-none">{t('inv.noP')}</p> : (
          <ol className="inv-list">
            {pSets.map((set, i) => (
              <li key={i} className="inv-item">
                <span className="inv-id">y<sub>{i + 1}</sub></span>
                <div className="inv-eq" role="math" aria-label={set.map((e) => `${e.weight} ${e.name}`).join(', ')}>
                  {set.map((e, k) => <Term key={e.index} term={e} showSign={k > 0 || e.weight < 0n} tone="p" />)}
                  <span className="inv-op">=</span>
                  <span className="inv-const">{t('inv.constant')}</span>
                </div>
              </li>
            ))}
          </ol>
        )}
        <Inequality items={inv.p_sub_invariants} names={placeNames} fallback="p" kind="sub" rel="≤" tone="p" idPrefix="p" t={t} />
        <Inequality items={inv.p_sur_invariants} names={placeNames} fallback="p" kind="sur" rel="≥" tone="p" idPrefix="p" t={t} />
        <Basis basis={inv.p_basis} names={placeNames} fallback="p" tone="p" t={t} />
      </Section>

      <Section
        id="inv-t" t={t} title={t('inv.tSection')} count={tSets.length}
        verdictLabel={t('inv.tCoverage')} verdict={inv.t_invariant_coverage}
        strip={tNames.length > 0 && <Coverage names={tNames} covered={tCovered} tone="t" label={t('inv.transitions')} />}
        help={{ summary: t('inv.whatIs'), text: t('inv.tDesc') }}
      >
        {tSets.length === 0 ? <p className="inv-none">{t('inv.noT')}</p> : (
          <ol className="inv-list">
            {tSets.map((set, i) => (
              <li key={i} className="inv-item">
                <span className="inv-id">x<sub>{i + 1}</sub></span>
                <div className="inv-eq" role="math" aria-label={set.map((e) => `${e.weight} ${e.name}`).join(', ')}>
                  {set.filter((e) => e.weight > 0n).map((e, k) => (
                    <span key={e.index} className="inv-seq">
                      {k > 0 && <span className="inv-arrow" aria-hidden="true">→</span>}
                      <span className="inv-chip t">{e.weight > 1n && <b className="inv-coeff">{String(e.weight)}×</b>}<span className="inv-chip-name">{e.name}</span></span>
                    </span>
                  ))}
                </div>
              </li>
            ))}
          </ol>
        )}
        <Inequality items={inv.t_sub_invariants} names={transNames} effectNames={placeNames} fallback="t" kind="sub" rel="≤" tone="t" idPrefix="t" t={t} />
        <Inequality items={inv.t_sur_invariants} names={transNames} effectNames={placeNames} fallback="t" kind="sur" rel="≥" tone="t" idPrefix="t" t={t} />
        <Basis basis={inv.t_basis} names={transNames} fallback="t" tone="t" t={t} />
      </Section>

      <section className="inv-section" aria-labelledby="inv-d">
        <header className="inv-section-head"><h2 id="inv-d">{t('inv.derived')}</h2></header>
        <dl className="inv-facts">
          <div><dt>{t('inv.structurallyBounded')}</dt><dd><Verdict value={inv.structurally_bounded} t={t} /></dd></div>
          <div><dt>{t('inv.conservative')}</dt><dd><Verdict value={inv.conservative} t={t} /></dd></div>
          {pSets.length > 0 && <div><dt>{t('inv.totalP')}</dt><dd>{unitWeights ? t('inv.totalPYes') : t('inv.totalPNo')}</dd></div>}
        </dl>
        {inv.structurally_bounded === true && inv.conservative === true && (
          <p className="inv-callout">{t('inv.conservativeAndBounded')}</p>
        )}
        <details className="inv-help"><summary>{t('inv.whatIs')}</summary><p>{t('inv.derivedDesc')}</p></details>
      </section>
    </div>
  );
};

export default InvariantsViewer;
