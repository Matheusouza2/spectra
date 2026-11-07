import React, { useId, useMemo } from 'react';
import './VectorField.css';
import { parseVector } from '../services/vector-input';
import { useI18n } from '../i18n/I18nContext';

/**
 * One comma-separated field for a vector with one value per named slot
 * (places or transitions), plus a circle per slot showing the typed value.
 * Circles with a value > 0 are filled, 0 is outlined, untyped slots are dashed.
 */
export default function VectorField({ label, help, text, onChange, onEnter, names, allowNegative = false, placeholder, inputRef, emptyHint }) {
  const { t } = useI18n();
  const id = useId();
  const parsed = useMemo(() => parseVector(text, names.length, { allowNegative }), [text, names.length, allowNegative]);
  const nameOf = (i) => names[i] || `#${i}`;

  const status = (() => {
    if (names.length === 0) return { tone: 'muted', text: '' };
    if (emptyHint && text.trim() === '') return { tone: 'muted', text: emptyHint };
    if (parsed.firstInvalid >= 0) {
      return { tone: 'error', text: t('vector.status.invalid', { value: parsed.cells[parsed.firstInvalid].raw, name: nameOf(parsed.firstInvalid) }) };
    }
    if (parsed.extra > 0) return { tone: 'error', text: t('vector.status.tooMany', { expected: names.length, got: parsed.count }) };
    if (parsed.count < names.length) {
      return { tone: 'muted', text: t('vector.status.partial', { count: parsed.count, total: names.length, next: nameOf(parsed.count) }) };
    }
    return { tone: 'ok', text: t('vector.status.complete', { total: names.length }) };
  })();

  return (
    <div className="vf-field">
      <label className="vf-label" htmlFor={`${id}-input`}>{label}</label>
      <input
        id={`${id}-input`}
        ref={inputRef}
        className={`vf-input ${status.tone === 'error' ? 'invalid' : ''}`}
        value={text}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Enter') onEnter?.(); }}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        inputMode="numeric"
        aria-invalid={status.tone === 'error'}
        aria-describedby={`${id}-help ${id}-status`}
      />
      {help && <p id={`${id}-help`} className="vf-help">{help}</p>}
      <p id={`${id}-status`} className={`vf-status ${status.tone}`} role="status">{status.text}</p>

      <ol className="vf-slots" aria-label={label}>
        {parsed.cells.map((cell, i) => (
          <li key={i} className={`vf-slot ${cell.kind}`}>
            <span
              className={`vf-circle ${cell.kind === 'value' && cell.value > 0 ? 'has' : ''} ${cell.kind === 'value' && cell.value < 0 ? 'neg' : ''}`}
              aria-label={`${nameOf(i)}: ${cell.kind === 'value' ? cell.value : cell.kind === 'invalid' ? t('vector.invalidShort') : t('vector.missing')}`}
            >
              {cell.kind === 'value' ? cell.value : cell.kind === 'invalid' ? '!' : '–'}
            </span>
            <span className="vf-slot-name" title={nameOf(i)}>{nameOf(i)}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}
