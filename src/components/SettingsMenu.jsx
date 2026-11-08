import { useCallback, useEffect, useRef, useState } from 'react';
import { useTheme } from '../contexts/ThemeContext';
import { useI18n } from '../i18n/I18nContext';
import './SettingsMenu.css';

export default function SettingsMenu({ onOpenLogs, onOpenReferences }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef(null);
  const triggerRef = useRef(null);
  const { theme, setTheme } = useTheme();
  const { language, setLanguage, languages, t } = useI18n();

  const close = useCallback((restoreFocus = true) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!open) return undefined;
    const onPointerDown = (e) => {
      if (!rootRef.current?.contains(e.target)) close(false);
    };
    const onKeyDown = (e) => {
      if (e.key === 'Escape') close();
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open, close]);

  const openLogs = () => {
    close(false);
    onOpenLogs();
  };

  const openReferences = () => {
    close(false);
    onOpenReferences();
  };

  return (
    <div className="settings" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className="settings-trigger"
        aria-label={t('common.settings')}
        title={t('common.settings')}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls="settings-panel"
        onClick={() => setOpen((v) => !v)}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>

      {open && (
        <div className="settings-panel" id="settings-panel" role="dialog" aria-label={t('common.settings')}>
          <div className="settings-group" role="radiogroup" aria-label={t('common.language')}>
            <span className="settings-label">{t('common.language')}</span>
            <div className="settings-segment">
              {languages.map((l) => (
                <button
                  key={l.code}
                  type="button"
                  role="radio"
                  aria-checked={language === l.code}
                  className="settings-option"
                  onClick={() => setLanguage(l.code)}
                  title={l.label}
                >
                  {l.label}
                </button>
              ))}
            </div>
          </div>

          <div className="settings-group" role="radiogroup" aria-label={t('common.theme')}>
            <span className="settings-label">{t('common.theme')}</span>
            <div className="settings-segment">
              {['light', 'dark'].map((value) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={theme === value}
                  className="settings-option"
                  onClick={() => setTheme(value)}
                >
                  {t(value === 'light' ? 'common.themeLight' : 'common.themeDark')}
                </button>
              ))}
            </div>
          </div>

          <button type="button" className="settings-item" onClick={openReferences}>
            {t('method.open')}
          </button>

          <button type="button" className="settings-item" onClick={openLogs}>
            {t('common.viewLogs')}
          </button>
        </div>
      )}
    </div>
  );
}
