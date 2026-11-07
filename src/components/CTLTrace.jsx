import React from 'react';
import { tokenValueToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';

/**
 * A CTL witness / counterexample drawn as a chain of markings.
 * Each node lists the places that hold tokens (by name); each edge carries the
 * fired transition. The last node is the target (witness) or the violation.
 */
export default function CTLTrace({ steps, nodes, placeNames, kind, infinite }) {
  const { t } = useI18n();
  if (!steps || steps.length === 0) return null;

  return (
    <ol className={`ctl-trace-graph ${kind}`} aria-label={t(kind === 'counterexample' ? 'ctl.counterexample' : 'ctl.witness')}>
      {steps.map((step, i) => {
        const held = (step.marking || [])
          .map((token, p) => ({ name: placeNames?.[p] || `p${p}`, text: tokenValueToString(token) }))
          .filter((cell) => cell.text !== '0');
        const last = i === steps.length - 1;
        return (
          <li key={i} className="ctl-trace-step">
            {i > 0 && (
              <span className="ctl-trace-edge" aria-hidden={false}>
                <span className="ctl-trace-edge-name">{step.transition_name || `t${step.transition}`}</span>
                <span className="ctl-trace-edge-line" aria-hidden="true" />
              </span>
            )}
            <div className={`ctl-trace-node ${i === 0 ? 'start' : ''} ${last ? 'end' : ''}`}>
              <span className="ctl-trace-node-id">
                M{nodes?.[i] ?? i}
                {i === 0 && <em>{t('ctl.traceStart')}</em>}
                {last && i > 0 && <em>{t(kind === 'circuit' ? 'ctl.traceCircuitEnd' : kind === 'witness' ? 'ctl.traceTarget' : 'ctl.traceViolation')}</em>}
              </span>
              {held.length === 0 ? (
                <span className="ctl-trace-empty">{t('ctl.traceEmpty')}</span>
              ) : (
                <ul className="ctl-trace-places">
                  {held.map((cell) => (
                    <li key={cell.name}>
                      <span>{cell.name}</span>
                      <b>{cell.text}</b>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </li>
        );
      })}
      {infinite && <li className="ctl-trace-loop">{t('ctl.traceLoop')}</li>}
    </ol>
  );
}
