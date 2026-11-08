import React, { useEffect, useMemo, useRef, useState } from 'react';
import './EvidenceViewer.css';
import LatexFormula from './LatexFormula';
import { evidenceTitle } from './evidence-label';
import { getAnalysisEvidence } from '../services/tauri-api';
import { useI18n } from '../i18n/I18nContext';
import { useToast } from './Toast';

const GROUPS = [
  ['evidence.groups.properties', ['B', 'DST', 'DTR', 'REV', 'L', 'LS', 'DCF']],
  ['evidence.groups.structural', ['ORD', 'HOM', 'CON', 'SC', 'PUR', 'SCF', 'MG', 'FC', 'EFC']],
  ['evidence.groups.invariants', ['P_INVARIANTS', 'T_INVARIANTS', 'CPI', 'CTI', 'CSV', 'SB']],
];

const statusLabelKey = {
  PROVEN: 'evidence.status.proven',
  DISPROVEN: 'evidence.status.disproven',
  INCONCLUSIVE: 'evidence.status.inconclusive',
};

const STATUS_MARK = { proven: '✓', disproven: '✗', inconclusive: '?' };
const keyOf = (item) => `${item.property}-${item.sequence ?? 0}`;
const statusOf = (item) => (item.status || 'INCONCLUSIVE').toLowerCase();

function resultLabel(item, t) {
  return item.result === true ? t('evidence.yes') : item.result === false ? t('evidence.no') : '—';
}

// Plain-text form of a certificate, for pasting into a report or a message.
function certificateText(evidence, title, tm) {
  const proof = evidence.proof;
  if (!proof) return title;
  const lines = [`${title} — ${tm(evidence.definition?.name) || ''}`.trim(), '', tm(proof.conclusion)];
  if (proof.assumptions?.length) lines.push('', ...proof.assumptions.map((a) => `• ${tm(a)}`));
  if (proof.facts?.length) lines.push('', ...proof.facts.map((f) => `${tm(f.label)}: ${tm(f.value)}`));
  if (proof.derivation?.length) lines.push('', ...proof.derivation.map((d, i) => `${i + 1}. ${tm(d.text)}`));
  if (proof.completeness) lines.push('', tm(proof.completeness));
  return lines.join('\n');
}

function EvidenceDetail({ evidence }) {
  const { t, tm } = useI18n();
  const { addToast } = useToast();
  const [technicalOpen, setTechnicalOpen] = useState(false);
  const status = statusOf(evidence);
  const certificate = evidence.certificate || evidence.counterexample;
  const method = (evidence.method || '').replaceAll('_', ' ');
  const invariantCertificate = certificate?.Invariant;
  const proof = evidence.proof;
  const proofKindKey = {
    PROOF: 'evidence.proofKind.proof',
    COUNTERPROOF: 'evidence.proofKind.counterproof',
    ALGEBRAIC_CERTIFICATE: 'evidence.proofKind.algebraic',
    INCONCLUSIVE: 'evidence.proofKind.inconclusive',
  }[proof?.kind] || statusLabelKey[evidence.status || 'INCONCLUSIVE'];

  return (
    <article className={`ev-detail ev-${status}`} aria-labelledby="ev-detail-title">
      <header className="ev-detail-head">
        <div className="ev-detail-heading">
          <h2 id="ev-detail-title">{evidenceTitle(evidence)}</h2>
          <p>{tm(evidence.definition?.name)}</p>
        </div>
        <div className="ev-verdict" aria-label={t('evidence.result')}>
          <span className="ev-verdict-mark" aria-hidden="true">{STATUS_MARK[status]}</span>
          <span className="ev-verdict-result">{resultLabel(evidence, t)}</span>
        </div>
        {proof?.conclusion && (
          <p className="ev-guarantee">{tm(proof.conclusion)}</p>
        )}
        <dl className="ev-detail-meta">
          <div><dt>{t('evidence.proofKindLabel')}</dt><dd>{proofKindKey ? t(proofKindKey) : status}</dd></div>
          <div><dt>{t('evidence.method')}</dt><dd>{method || '—'}</dd></div>
          <div>
            <dt>{t('evidence.scope')}</dt>
            <dd>{evidence.metadata?.exact_state_space ? t('evidence.scopeExact') : t('evidence.scopeStructural')}</dd>
          </div>
        </dl>
      </header>

      <section className="ev-block">
        <h3>{t('evidence.definition')}</h3>
        <LatexFormula className="evidence-formula" value={evidence.definition?.formula_latex} />
        <p className="ev-prose">{tm(evidence.definition?.explanation)}</p>
      </section>

      <section className="ev-block">
        <div className="ev-block-head">
          <h3>{t('evidence.certificate')}</h3>
          {proof && (
            <button
              type="button"
              className="ev-copy"
              onClick={() => {
                navigator.clipboard?.writeText(certificateText(evidence, evidenceTitle(evidence), tm))
                  .then(() => addToast(t('evidence.copied'), 'success'))
                  .catch(() => addToast(t('evidence.copyFailed'), 'error'));
              }}
            >
              {t('evidence.copy')}
            </button>
          )}
        </div>
        {proof ? (
          <div className="proof-certificate">
            {proof.assumptions?.length > 0 && (
              <div className="proof-stage">
                <span className="proof-label">{t('evidence.assumptions')}</span>
                <ul className="proof-assumptions">
                  {proof.assumptions.map((assumption) => <li key={assumption}>{tm(assumption)}</li>)}
                </ul>
              </div>
            )}
            {proof.facts?.length > 0 && (
              <div className="proof-stage">
                <span className="proof-label">{t('evidence.verifiedData')}</span>
                <div className="proof-facts">
                  {proof.facts.map((fact) => (
                    <div className="proof-fact" key={`${fact.label}-${fact.value}`}>
                      <span>{tm(fact.label)}</span>
                      {fact.formula_latex ? <LatexFormula value={fact.formula_latex} /> : <span />}
                      <strong>{tm(fact.value)}</strong>
                    </div>
                  ))}
                </div>
              </div>
            )}
            {proof.tables?.map((table) => (
              <div className="proof-stage proof-table-wrap" key={table.title}>
                <span className="proof-label">{tm(table.title)}</span>
                <div className="proof-table-scroll">
                  <table className="proof-table">
                    <thead><tr>{table.columns.map((column) => <th key={column} scope="col">{tm(column)}</th>)}</tr></thead>
                    <tbody>{table.rows.map((row, rowIndex) => <tr key={`${table.title}-${rowIndex}`}>{row.map((cell, cellIndex) => <td key={`${rowIndex}-${cellIndex}`}>{tm(cell)}</td>)}</tr>)}</tbody>
                  </table>
                </div>
              </div>
            ))}
            {proof.derivation?.length > 0 && (
              <div className="proof-stage">
                <span className="proof-label">{t('evidence.verification')}</span>
                <ol className="proof-derivation">
                  {proof.derivation.map((step, i) => (
                    <li className="proof-step" key={`${i}-${step.text}`}>
                      <span className="proof-step-n" aria-hidden="true">{i + 1}</span>
                      <div><p>{tm(step.text)}</p>{step.formula_latex ? <LatexFormula value={step.formula_latex} /> : null}</div>
                    </li>
                  ))}
                </ol>
              </div>
            )}
            {proof.completeness && (
              <div className="proof-stage proof-coverage">
                <span className="proof-label">∴ {t('evidence.coverage')}</span>
                <p>{tm(proof.completeness)}</p>
              </div>
            )}
          </div>
        ) : <p className="evidence-muted">{t('evidence.noCertificate')}</p>}
      </section>

      {invariantCertificate && Object.prototype.hasOwnProperty.call(invariantCertificate, 'realizability') && (
        <section className="ev-block evidence-realizability">
          <h3>{t('evidence.realizability')}</h3>
          <p>{t('evidence.algebraicallyValid')}: <strong>{t('evidence.yesUpper')}</strong></p>
          <p>
            {t('evidence.realizable')}:{' '}
            <strong>
              {invariantCertificate.realizability === true
                ? t('evidence.yesUpper')
                : invariantCertificate.realizability === false
                  ? t('evidence.noUpper')
                  : t('evidence.status.inconclusive')}
            </strong>
          </p>
          {invariantCertificate.realization_witness?.length > 0 && (
            <p className="evidence-witness">
              {t('evidence.witness')}: {invariantCertificate.realization_witness.map((transition) => `t${transition}`).join(' → ')}
            </p>
          )}
        </section>
      )}

      <section className="ev-block ev-technical">
        <button className="technical-data-toggle" type="button" aria-expanded={technicalOpen} onClick={() => setTechnicalOpen((value) => !value)}>
          {technicalOpen ? t('evidence.hideTechnical') : t('evidence.showTechnical')}
        </button>
        {technicalOpen && (certificate ? <pre className="evidence-json">{JSON.stringify(certificate, null, 2)}</pre> : <p className="evidence-muted">{t('evidence.noArtifact')}</p>)}
      </section>

      <footer className="ev-provenance">
        <h3>{t('evidence.provenance')}</h3>
        <p>{tm(evidence.metadata?.algorithm)}</p>
        {evidence.references?.length > 0 && (
          <ul>{evidence.references.map((reference) => <li key={`${reference.author}-${reference.title}`}>{reference.author} — <em>{reference.title}</em></li>)}</ul>
        )}
      </footer>
    </article>
  );
}

export default function EvidenceViewer({ analysisData, filePath, graphKey = '', graphReady = true }) {
  const { t, tm } = useI18n();
  const [evidence, setEvidence] = useState(analysisData?.evidence || []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [selected, setSelected] = useState(null);
  const [filter, setFilter] = useState('all');
  const detailRef = useRef(null);

  useEffect(() => {
    let current = true;
    setEvidence(analysisData?.evidence || []);
    if (!filePath || analysisData?.evidence?.length) return () => { current = false; };
    setLoading(true);
    setError(null);
    getAnalysisEvidence(filePath)
      .then((result) => { if (current) setEvidence(result); })
      .catch((reason) => { if (current) setError(String(reason)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [filePath, analysisData, graphKey]);

  const grouped = useMemo(() => {
    const map = new Map();
    evidence.forEach((item) => {
      const items = map.get(item.property) || [];
      items.push(item);
      map.set(item.property, items);
    });
    return GROUPS.map(([titleKey, ids]) => ({
      titleKey,
      items: ids.flatMap((id) => map.get(id) || []).filter((item) => filter === 'all' || statusOf(item) === filter),
    })).filter((group) => group.items.length > 0);
  }, [evidence, filter]);

  const counts = useMemo(() => evidence.reduce((acc, item) => {
    acc[statusOf(item)] = (acc[statusOf(item)] || 0) + 1;
    return acc;
  }, { proven: 0, disproven: 0, inconclusive: 0 }), [evidence]);

  const visible = grouped.flatMap((group) => group.items);
  const active = visible.find((item) => keyOf(item) === selected) || visible[0] || null;

  const onIndexKey = (event) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    const rows = [...event.currentTarget.querySelectorAll('.ev-row')];
    const at = rows.indexOf(document.activeElement);
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + (event.key === 'ArrowDown' ? 1 : -1)))];
    next?.focus();
    next?.click();
  };

  const choose = (item) => {
    setSelected(keyOf(item));
    if (window.matchMedia?.('(max-width: 960px)').matches) {
      requestAnimationFrame(() => detailRef.current?.scrollIntoView({ block: 'start' }));
    }
  };

  if (!analysisData) return null;

  const total = evidence.length;
  return (
    <div className="evidence-screen">
      <header className="ev-head">
        <div className="ev-head-text">
          <h1>{t('evidence.title')}</h1>
          <p>{t('evidence.intro')}</p>
        </div>
        {total > 0 && (
          <div className="ev-summary">
            <div className="ev-summary-bar" role="img" aria-label={['proven', 'disproven', 'inconclusive'].map((k) => `${counts[k]} ${t(statusLabelKey[k.toUpperCase()])}`).join(', ')}>
              {['proven', 'disproven', 'inconclusive'].map((k) => counts[k] > 0 && <span key={k} className={`ev-seg seg-${k}`} style={{ flexGrow: counts[k] }} />)}
            </div>
            <div className="ev-filters" role="group" aria-label={t('evidence.filter')}>
              <button type="button" className={filter === 'all' ? 'on' : ''} aria-pressed={filter === 'all'} onClick={() => setFilter('all')}>
                {t('evidence.all')} <b>{total}</b>
              </button>
              {['proven', 'disproven', 'inconclusive'].map((k) => (
                <button key={k} type="button" className={`${filter === k ? 'on' : ''} f-${k}`} aria-pressed={filter === k} disabled={counts[k] === 0} onClick={() => setFilter(k)}>
                  <span aria-hidden="true">{STATUS_MARK[k]}</span> {t(statusLabelKey[k.toUpperCase()])} <b>{counts[k]}</b>
                </button>
              ))}
            </div>
          </div>
        )}
      </header>

      {!graphReady && <p className="ev-pending" role="status">{t('evidence.needsGraph')}</p>}

      {loading ? (
        <p className="ev-empty" role="status">{t('evidence.computing')}</p>
      ) : error ? (
        <p className="ev-empty" role="alert">{t('evidence.failed', { error: tm(error) })}</p>
      ) : total === 0 ? (
        <p className="ev-empty">{t('evidence.none')}</p>
      ) : (
        <div className="ev-layout">
          <nav className="ev-index" aria-label={t('evidence.title')} onKeyDown={onIndexKey}>
            {grouped.map((group) => (
              <section key={group.titleKey} className="ev-index-group">
                <h2>{t(group.titleKey)}</h2>
                <ul>
                  {group.items.map((item) => {
                    const status = statusOf(item);
                    const isActive = active && keyOf(item) === keyOf(active);
                    return (
                      <li key={keyOf(item)}>
                        <button type="button" className={`ev-row ev-${status} ${isActive ? 'active' : ''}`} aria-current={isActive ? 'true' : undefined} onClick={() => choose(item)}>
                          <span className="ev-row-mark" aria-hidden="true">{STATUS_MARK[status]}</span>
                          <span className="ev-row-name">
                            <strong>{evidenceTitle(item)}</strong>
                            <span>{tm(item.definition?.name)}</span>
                          </span>
                          <span className="ev-row-result">{resultLabel(item, t)}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
          </nav>
          <div className="ev-pane" ref={detailRef} key={active ? keyOf(active) : 'none'}>
            {active && <EvidenceDetail evidence={active} />}
          </div>
        </div>
      )}
    </div>
  );
}
