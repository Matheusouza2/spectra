import { useEffect, useState } from 'react';
import { readLogEntries } from '../services/app-logger';
import { useI18n } from '../i18n/I18nContext';
import './DiagnosticsPanel.css';

export default function DiagnosticsPanel({ onClose }) {
  const { t, language } = useI18n();
  const [entries, setEntries] = useState([]);
  const [error, setError] = useState('');
  const refresh = () => readLogEntries().then(setEntries).catch((err) => setError(String(err)));

  useEffect(() => { refresh(); }, []);

  return (
    <div className="diagnostics-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="diagnostics-panel" role="dialog" aria-modal="true" aria-labelledby="diagnostics-title">
        <header className="diagnostics-header">
          <div><span className="panel-kicker">{t('diagnostics.kicker')}</span><h2 id="diagnostics-title">{t('diagnostics.title')}</h2></div>
          <div className="diagnostics-actions">
            <button className="diagnostics-refresh" onClick={refresh}>{t('diagnostics.refresh')}</button>
            <button className="diagnostics-close" onClick={onClose} aria-label={t('common.close')}>×</button>
          </div>
        </header>
        <p className="diagnostics-location">{t('diagnostics.location')}</p>
        {error && <p className="diagnostics-error">{t('diagnostics.readError', { error })}</p>}
        {!error && entries.length === 0 ? <p className="diagnostics-empty">{t('diagnostics.empty')}</p> : (
          <div className="diagnostics-list">
            {entries.map((entry, index) => (
              <article className={`diagnostics-entry level-${entry.level?.toLowerCase()}`} key={`${entry.timestamp}-${index}`}>
                <time>{new Date(Number(entry.timestamp) * 1000).toLocaleString(language)}</time>
                <span className="diagnostics-level">{entry.level}</span>
                <strong>{entry.event}</strong>
                <p>{entry.message}</p>
                <small>{entry.source}</small>
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
