import React, { useState, useRef, useCallback } from 'react';
import { analyzePnt } from '../services/tauri-api';
import { useToast } from './Toast';
import { open } from '@tauri-apps/plugin-dialog';
import { useI18n } from '../i18n/I18nContext';
import './FileUpload.css';

const FileUpload = ({ onAnalysisComplete, onFileSelect, value = null, variant = 'bar' }) => {
  const [pickedFile, setSelectedFile] = useState(null);
  const selectedFile = value ?? pickedFile;
  const [loading, setLoading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const { addToast } = useToast();
  const { t, tm } = useI18n();
  const dropRef = useRef(null);

  const handleFileSelect = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Petri Net', extensions: ['pnt'] }],
      });
      if (selected) {
        setSelectedFile(selected);
        if (onFileSelect) onFileSelect(selected);
      }
    } catch (err) {
      addToast(t('upload.selectFailed', { error: tm(err) }), 'error');
    }
  }, [addToast, onFileSelect, t]);

  const handleUpload = useCallback(async () => {
    if (!selectedFile) return;
    setLoading(true);
    try {
      const result = await analyzePnt(selectedFile);
      onAnalysisComplete(result, selectedFile);
      const toastType = result.analysis_status === 'structural_analysis_complete'
        ? 'info'
        : result.analysis_message ? 'warning' : 'success';
      addToast(tm(result.analysis_message) || t('upload.success'), toastType, 7000);
    } catch (err) {
      addToast(t('upload.analysisFailed', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [selectedFile, onAnalysisComplete, addToast, t]);

  const handleDrop = useCallback(async (e) => {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer?.files?.[0];
    if (file && file.name.endsWith('.pnt')) {
      setSelectedFile(file.path);
      if (onFileSelect) onFileSelect(file.path);
    } else {
      addToast(t('upload.dropPnt'), 'warning');
    }
  }, [addToast, onFileSelect, t]);

  const handleDragOver = useCallback((e) => { e.preventDefault(); setDragOver(true); }, []);
  const handleDragLeave = useCallback(() => setDragOver(false), []);

  const analyzeLabel = loading ? (
    <span className="fu-loading"><span className="spinner-sm" />{t('upload.analyzing')}</span>
  ) : t('upload.analyze');

  if (variant === 'hero') {
    const fileName = selectedFile ? selectedFile.split('/').pop() : null;
    return (
      <div className="fu-hero">
        <div
          ref={dropRef}
          role="button"
          tabIndex={0}
          className={`fu-hero-drop ${dragOver ? 'fu-drop-active' : ''} ${fileName ? 'has-file' : ''}`}
          onClick={handleFileSelect}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleFileSelect(); } }}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          {fileName ? (
            <>
              <span className="fu-hero-file" title={selectedFile}>{fileName}</span>
              <span className="fu-hero-sub">{t('upload.change')}</span>
            </>
          ) : (
            <>
              <span className="fu-hero-title">{t('upload.heroTitle')}</span>
              <span className="fu-hero-sub">{t('upload.heroSub')} <code>.pnt</code></span>
            </>
          )}
        </div>
        <button type="button" className="fu-hero-btn" onClick={handleUpload} disabled={!selectedFile || loading}>
          {analyzeLabel}
        </button>
      </div>
    );
  }

  return (
    <div className="fu-bar">
      <div
        ref={dropRef}
        className={`fu-drop ${dragOver ? 'fu-drop-active' : ''}`}
        onClick={handleFileSelect}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
      >
        {selectedFile ? (
          <span className="fu-name" title={selectedFile}>
            {selectedFile.split('/').pop()}
          </span>
        ) : (
          <span className="fu-hint">
            {t('upload.load')} <code>.pnt</code>
          </span>
        )}
      </div>
      <button
        className="fu-btn"
        onClick={handleUpload}
        disabled={!selectedFile || loading}
      >
        {loading ? (
          <span className="fu-loading">
            <span className="spinner-sm" />
            {t('upload.analyzing')}
          </span>
        ) : (
          t('upload.analyze')
        )}
      </button>
    </div>
  );
};

export default FileUpload;
