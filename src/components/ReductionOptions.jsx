import React, { useEffect, useState } from 'react';
import './ReductionOptions.css';
import { useI18n } from '../i18n/I18nContext';

const key = (scope, option) => `spectra.reduction.${scope}.${option}`;

/** Remembered on/off flag for one reduction option of one query panel. */
export function useReductionFlag(scope, option) {
  const [value, setValue] = useState(() => {
    try { return localStorage.getItem(key(scope, option)) === '1'; } catch { return false; }
  });
  useEffect(() => {
    try { localStorage.setItem(key(scope, option), value ? '1' : '0'); } catch { /* storage unavailable */ }
  }, [scope, option, value]);
  return [value, setValue];
}

/**
 * Opt-in reductions for a query. `options` is a list of
 * `{ id, checked, onChange, disabled }` (disabled options are hidden); labels and hints come from `reduction.<id>.*`.
 */
export default function ReductionOptions({ options }) {
  const { t } = useI18n();
  // An option the net cannot use is not offered at all; with none left the group disappears.
  const available = options.filter((option) => !option.disabled);
  if (available.length === 0) return null;
  return (
    <fieldset className="ro">
      <legend>{t('reduction.legend')}</legend>
      {available.map((option) => (
        <label key={option.id} className="ro-item">
          <input
            type="checkbox"
            checked={Boolean(option.checked)}
            onChange={(event) => option.onChange(event.target.checked)}
          />
          <span>
            <strong>{t(`reduction.${option.id}.label`)}</strong>
            <small>{t(`reduction.${option.id}.hint`)}</small>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
