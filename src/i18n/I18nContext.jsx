import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { DEFAULT_LANGUAGE, LANGUAGES, messages } from '../locales';
import { translateBackend } from './backend-messages';
import { syncTrayLabels } from '../services/desktop-shell';

const STORAGE_KEY = 'racon-language';
const I18nContext = createContext(null);

const isSupported = (code) => LANGUAGES.some((l) => l.code === code);

function readStoredLanguage() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isSupported(stored)) return stored;
  } catch { /* storage unavailable */ }
  return DEFAULT_LANGUAGE;
}

// Keys are "<namespace>.<rest>". <rest> may be a flat key ("rows.B.label") or a nested path.
const lookup = (lang, key) => {
  const dot = key.indexOf('.');
  if (dot < 0) return undefined;
  const ns = messages[lang]?.[key.slice(0, dot)];
  if (!ns) return undefined;
  const rest = key.slice(dot + 1);
  if (typeof ns[rest] === 'string') return ns[rest];
  return rest.split('.').reduce((node, part) => (node && typeof node === 'object' ? node[part] : undefined), ns);
};

export function translate(lang, key, params) {
  let text = lookup(lang, key) ?? lookup(DEFAULT_LANGUAGE, key);
  if (typeof text !== 'string') return key;
  if (params) {
    text = text.replace(/\{(\w+)\}/g, (m, name) => (params[name] !== undefined ? String(params[name]) : m));
  }
  return text;
}

export function I18nProvider({ children }) {
  const [language, setLanguageState] = useState(readStoredLanguage);

  useEffect(() => {
    document.documentElement.setAttribute('lang', language);
    try { localStorage.setItem(STORAGE_KEY, language); } catch { /* ignore */ }
    syncTrayLabels(language);
  }, [language]);

  const setLanguage = useCallback((code) => {
    if (isSupported(code)) setLanguageState(code);
  }, []);

  const t = useCallback((key, params) => translate(language, key, params), [language]);

  // Translates text produced by the Rust backend (Portuguese) into the UI language.
  const tm = useCallback((value) => {
    const text = value instanceof Error ? value.message : value;
    return typeof text === 'string' ? translateBackend(text, language) : value;
  }, [language]);

  const value = useMemo(() => ({ language, setLanguage, t, tm, languages: LANGUAGES }), [language, setLanguage, t, tm]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error('useI18n must be used within I18nProvider');
  return ctx;
}

// Render-prop helper for class components (e.g. error boundaries).
export function I18nConsumer({ children }) {
  const { t } = useI18n();
  return children(t);
}
