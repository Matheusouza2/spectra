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
 * `{ id, checked, onChange, disabled, disabledReason }`; labels and hints come from `reduction.<id>.*`.
 */
export default function ReductionOptions({ options }) {
  const { t } = useI18n();
  return (
    <fieldset className="ro">
      <legend>{t('reduction.legend')}</legend>
      {options.map((option) => (
        <label key={option.id} className={`ro-item${option.disabled ? ' off' : ''}`} title={option.disabled ? option.disabledReason : undefined}>
          <input
            type="checkbox"
            checked={Boolean(option.checked) && !option.disabled}
            disabled={option.disabled}
            onChange={(event) => option.onChange(event.target.checked)}
          />
          <span>
            <strong>{t(`reduction.${option.id}.label`)}</strong>
            <small>{option.disabled && option.disabledReason ? option.disabledReason : t(`reduction.${option.id}.hint`)}</small>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
