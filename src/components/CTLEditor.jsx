import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import './CTLEditor.css';
import { useToast } from './Toast';
import { checkCtlFormula, validateCtlFormula } from '../services/tauri-api';
import { loadGraphGenerationConfig } from '../services/graph-config';
import { useI18n } from '../i18n/I18nContext';
import { dslFor, grammarSections } from '../locales/dsl';
import CTLResult from './CTLResult';
import { prettyCtl } from '../services/ctl-format';
import BoundedGate from './BoundedGate';

// ── Syntax highlighting (mirror layer behind the textarea) ──
// Both vocabularies are highlighted at once, like the Rust parser accepts them.
const L = '(?<![\\p{L}\\d_])';
const R = '(?![\\p{L}\\d_])';
const TOKEN_RE = new RegExp([
  `(?<quant>${L}(?:for\\s+all\\s+paths?|exists\\s+(?:a\\s+)?path|para\\s+todos?\\s+(?:os\\s+)?caminhos?|existe\\s+(?:um\\s+)?caminho)${R})`,
  `(?<temporal>${L}(?:always|eventually|next|until|sempre|eventualmente|pr[óo]ximo|at[ée])${R})`,
  `(?<bool>${L}(?:AND|OR|E|OU|not|nao|não)${R})`,
  `(?<const>${L}(?:TRUE|FALSE)${R})`,
  `(?<place>${L}[\\p{L}_][\\p{L}\\d_]*(?=\\s*(?:>=|<=|!=|=|<|>)))`,
  `(?<num>${L}\\d+${R})`,
  `(?<op>>=|<=|!=|=|<|>)`,
  `(?<paren>[()])`,
].join('|'), 'giu');

function highlight(text) {
  const out = [];
  let last = 0;
  for (const m of text.matchAll(TOKEN_RE)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const kind = Object.keys(m.groups).find((k) => m.groups[k] !== undefined);
    out.push(<span key={m.index} className={`tok tok-${kind}`}>{m[0]}</span>);
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

// ── Grammar Rules ──
// CTL formula in Portuguese DSL:
//   formula := quantifier temporal (condition)
//   quantifier := "para todo caminho" | "existe caminho"
//   temporal  := "sempre" (AG) | "eventualmente" (EF) | "próximo" (EX) | "até" (EU)
//   condition := proposition | "nao" proposition
//   proposition := TRUE | FALSE | place_expr
//   place_expr := "p<idx> = <int>"
//   binary_op := "E" | "OU"
//
// Full form: quantifier temporal ( condition [ E/OU condition ]... )

function buildExamples(placeNames = [], dsl, transitionNames = []) {
  const firstPlace = placeNames[0] || 'p0';
  const secondPlace = placeNames[1] || placeNames[0] || 'p1';
  return dsl.exampleSecond(firstPlace, secondPlace, transitionNames[0] || 't0');
}

// ── Context-sensitive autocomplete ──

function tokenize(text) {
  return text.split(/\s+/).filter(Boolean);
}

function lastWord(text, caretPos) {
  const before = text.slice(0, caretPos);
  const match = before.match(/(\S+)$/);
  return match ? match[1].toLowerCase() : '';
}

function textBeforeCaret(text, caretPos) {
  return text.slice(0, caretPos).toLowerCase();
}

function currentContext(text, caretPos, dsl) {
  const before = textBeforeCaret(text, caretPos);
  const tokens = tokenize(before);
  if (tokens.length === 0) return 'start';

  const last = tokens[tokens.length - 1];

  // After binary operator → new formula
  if (last === dsl.and.toLowerCase() || last === dsl.or.toLowerCase()) return 'start';

  // Check if we're inside a parenthesized condition
  const parenDepth = (before.match(/\(/g) || []).length - (before.match(/\)/g) || []).length;
  if (parenDepth > 0) {
    // Inside condition parent — check what came before
    const beforeParen = before.lastIndexOf('(');
    const insideParen = before.slice(beforeParen + 1).trim();

    // After 'nao ' → show conditions
    if (dsl.notPrefixRegex.test(insideParen)) return 'conditions';

    // After '=' → show number
    if (insideParen.endsWith('=')) return 'number';

    // After a place name pattern like 'p0 p1' → show 'E', 'OU' or ')'
    if (/p\d+\s*=?\s*\d*$/.test(insideParen)) {
      const afterEq = insideParen.split('=');
      if (afterEq.length > 1 && afterEq[1].trim().length > 0) return 'operators';
      if (afterEq.length > 1) return 'number';
      return 'conditions';
    }

    // Inside paren, typing a place name
    if (insideParen.match(/p\d*\s*$/)) return 'placeNames';

    // General condition keywords
    return 'conditions';
  }

  // After quantifier → show temporal operators
  if (dsl.afterQuantifierRegex.test(before)) return 'afterPath';

  // After temporal → expect '('
  const hasTemporal = Object.values(dsl.temporal).map((w) => w.toLowerCase()).some((t) =>
    before.endsWith(t) || before.endsWith(t + ' ')
  );
  if (hasTemporal) return 'expectParen';

  return 'start';
}

function filterSuggestions(context, partial, placeNames, dsl) {
  const { always, eventually, next, until } = dsl.temporal;
  const lower = partial.toLowerCase();

  // Build pool based on context
  let pool;
  switch (context) {
    case 'start':
      pool = [dsl.forAll, dsl.exists];
      break;
    case 'afterPath':
      pool = [`${always} (`, `${eventually} (`, `${next} (`, `${until} (`];
      break;
    case 'expectParen':
      pool = ['('];
      break;
    case 'conditions':
      pool = [
        'TRUE',
        'FALSE',
        `${dsl.not} (`,
        ...(placeNames || []).map((n, i) => `${n} =`),
        ...(placeNames ? [] : ['p0 =', 'p1 =', 'p2 =']),
      ];
      break;
    case 'placeNames':
      pool = placeNames || ['p0', 'p1', 'p2', 'p3', 'p4', 'p5'];
      break;
    case 'number':
      pool = ['0', '1', '2'];
      break;
    case 'operators':
      pool = [')', `${dsl.and} (`, `${dsl.or} (`];
      break;
    default:
      pool = [];
  }

  if (!partial) return pool.slice(0, 7);
  return pool.filter((s) => s.toLowerCase().includes(lower)).slice(0, 7);
}

function validateFormula(text, t, dsl) {
  const trimmed = text.trim();
  if (!trimmed) return { valid: false, message: t('ctl.emptyFormula') };
  const startsWithQuant = dsl.startRegex.test(trimmed);
  if (!startsWithQuant) return { valid: false, message: t('ctl.mustStart', { forAll: dsl.forAll, exists: dsl.exists }) };
  const parens = (trimmed.match(/\(/g) || []).length === (trimmed.match(/\)/g) || []).length;
  if (!parens) return { valid: false, message: t('ctl.unbalanced') };
  return { valid: true, message: '' };
}

function validationMessage(validationResult, t) {
  const detail = validationResult?.error;
  if (!detail) return t('ctl.invalidQuery');
  return `${detail.message}${detail.line ? ` (${detail.line}:${detail.column})` : ''}`;
}

const CTLEditor = ({ filePath, onResult, placeNames, result, gate, embedded = false, inject, transitionNames }) => {
  const blocked = gate ? !gate.allowed : false;
  const [formula, setFormula] = useState('');
  const [loading, setLoading] = useState(false);
  const [suggestions, setSuggestions] = useState([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [selectedIdx, setSelectedIdx] = useState(-1);
  const [validation, setValidation] = useState({ valid: true, message: '' });
  const [preview, setPreview] = useState('');
  const textareaRef = useRef(null);
  const mirrorRef = useRef(null);
  const validationRequestRef = useRef(0);
  const { addToast } = useToast();
  const { t, tm, language } = useI18n();
  const dsl = useMemo(() => dslFor(language), [language]);
  const grammar = useMemo(() => grammarSections(language), [language]);
  const examples = useMemo(() => buildExamples(placeNames, dsl, transitionNames), [placeNames, dsl, transitionNames]);

  const updateSuggestions = useCallback(async (text, caretPos, showList = true) => {
    const requestId = ++validationRequestRef.current;
    const ctx = currentContext(text, caretPos, dsl);
    const partial = lastWord(text, caretPos);
    let filtered = [];
    if (!text.trim() && requestId === validationRequestRef.current) setPreview('');
    if (filePath && text.trim()) {
      try {
        const validationResult = await validateCtlFormula(filePath, text, language);
        if (requestId === validationRequestRef.current) {
          setPreview(validationResult.valid ? (validationResult.preview || '') : '');
          setValidation(validationResult.valid
            ? { valid: true, message: '' }
            : { valid: false, message: validationMessage(validationResult, t) });
        }
        filtered = (validationResult.suggestions || []).filter((suggestion) =>
          !partial || suggestion.toLowerCase().includes(partial.toLowerCase()),
        );
      } catch {
        filtered = [];
      }
    }
    if (filtered.length === 0) filtered = filterSuggestions(ctx, partial, placeNames, dsl);
    if (requestId !== validationRequestRef.current) return;
    setSuggestions(filtered);
    setShowSuggestions(showList && filtered.length > 0);
    setSelectedIdx(-1);
  }, [filePath, placeNames, t, dsl, language]);

  const handleChange = useCallback((e) => {
    const val = e.target.value;
    setFormula(val);
    setValidation(validateFormula(val, t, dsl));
    const caretPos = e.target.selectionStart;
    updateSuggestions(val, caretPos);
  }, [updateSuggestions, t, dsl]);

  const acceptSuggestion = useCallback((suggestion) => {
    const ta = textareaRef.current;
    if (!ta) return;
    const caretPos = ta.selectionStart;
    const before = formula.slice(0, caretPos);
    const after = formula.slice(caretPos);
    const match = before.match(/(\S+)$/);

    let prefix, newFormula, newPos;
    if (match) {
      prefix = before.slice(0, before.length - match[1].length);
      newFormula = prefix + suggestion + ' ' + after;
      newPos = prefix.length + suggestion.length + 1;
    } else {
      newFormula = suggestion + ' ' + after;
      newPos = suggestion.length + 1;
    }

    setFormula(newFormula);
    setValidation(validateFormula(newFormula, t, dsl));
    requestAnimationFrame(() => {
      ta.focus();
      ta.setSelectionRange(newPos, newPos);
      updateSuggestions(newFormula, newPos);
    });
    setShowSuggestions(false);
  }, [formula, updateSuggestions, t, dsl]);

  const handleKeyDown = useCallback((e) => {
    if (showSuggestions && suggestions.length > 0) {
      if (e.key === 'ArrowDown') { e.preventDefault(); setSelectedIdx((prev) => (prev + 1) % suggestions.length); return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); setSelectedIdx((prev) => (prev <= 0 ? suggestions.length - 1 : prev - 1)); return; }
      if (e.key === 'Tab' || e.key === 'Enter') {
        if (selectedIdx >= 0 && selectedIdx < suggestions.length) {
          e.preventDefault();
          acceptSuggestion(suggestions[selectedIdx]);
          return;
        }
      }
      if (e.key === 'Escape') { setShowSuggestions(false); return; }
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      checkFormula();
    }
  }, [showSuggestions, suggestions, selectedIdx, acceptSuggestion]);

  const checkFormula = useCallback(async () => {
    const trimmed = formula.trim();
    const v = validateFormula(trimmed, t, dsl);
    if (!v.valid) { addToast(v.message, 'warning'); return; }
    if (!filePath) { addToast(t('ctl.selectFile'), 'warning'); return; }
    if (blocked) return;
    setLoading(true);
    try {
      const validationResult = await validateCtlFormula(filePath, trimmed, language);
      if (!validationResult.valid) {
        const message = validationMessage(validationResult, t);
        setValidation({ valid: false, message });
        addToast(message, 'warning');
        return;
      }
      const result = await checkCtlFormula(filePath, trimmed, loadGraphGenerationConfig());
      onResult(result);
      addToast(t('ctl.checkDone'), 'success');
    } catch (err) {
      addToast(t('common.errorWith', { error: tm(err) }), 'error');
    } finally {
      setLoading(false);
    }
  }, [formula, filePath, onResult, addToast, t, dsl, language, blocked]);

  const insertExample = useCallback((ex) => {
    setFormula(ex);
    setValidation(validateFormula(ex, t, dsl));
    setShowSuggestions(false);
    textareaRef.current?.focus();
    // Focus fires onFocus with the previous (stale) text; this newer request supersedes it.
    updateSuggestions(ex, ex.length, false);
  }, [t, dsl, updateSuggestions]);

  const syncScroll = useCallback((e) => {
    if (mirrorRef.current) {
      mirrorRef.current.scrollTop = e.target.scrollTop;
      mirrorRef.current.scrollLeft = e.target.scrollLeft;
    }
  }, []);

  const insertSnippet = useCallback((snippet) => {
    const ta = textareaRef.current;
    const pos = ta ? ta.selectionStart : formula.length;
    const end = ta ? ta.selectionEnd : formula.length;
    const needsSpace = pos > 0 && !/\s$/.test(formula.slice(0, pos));
    const next = formula.slice(0, pos) + (needsSpace ? ' ' : '') + snippet + formula.slice(end);
    const caret = pos + (needsSpace ? 1 : 0) + snippet.length;
    setFormula(next);
    setValidation(validateFormula(next, t, dsl));
    setShowSuggestions(false);
    requestAnimationFrame(() => {
      ta?.focus();
      ta?.setSelectionRange(caret, caret);
      updateSuggestions(next, caret, false);
    });
  }, [formula, t, dsl, updateSuggestions]);

  const showError = !validation.valid && formula.trim();
  const errorId = 'ctl-formula-error';

  // A query built elsewhere (the Predicates mode) lands in the editor, validated like typed text.
  useEffect(() => {
    if (!inject?.text) return;
    setFormula(inject.text);
    setValidation(validateFormula(inject.text, t, dsl));
    setShowSuggestions(false);
    updateSuggestions(inject.text, inject.text.length, false);
    textareaRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inject?.nonce]);

  return (
    <div className="ctl">
      <div className="ctl-main">
        {!embedded && <header className="ctl-head">
          <h2 className="ctl-title">{t('ctl.checker')}</h2>
          <p className="ctl-desc">{t('ctl.subtitle')}</p>
        </header>}

        {!embedded && <BoundedGate gate={gate} />}

        <section className="ctl-bench" aria-label={t('ctl.checker')}>
          <div className={`formula-field ${showError ? 'invalid' : ''}`}>
            <pre className="formula-mirror" ref={mirrorRef} aria-hidden="true">
              {highlight(formula)}{'\n'}
            </pre>
            <textarea
              ref={textareaRef}
              className="formula-input"
              value={formula}
              onChange={handleChange}
              onKeyDown={handleKeyDown}
              onScroll={syncScroll}
              onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
              onFocus={(e) => updateSuggestions(formula, e.target.selectionStart)}
              onClick={(e) => updateSuggestions(formula, e.target.selectionStart)}
              placeholder={t('ctl.placeholder')}
              aria-label={t('ctl.formula')}
              aria-invalid={showError ? 'true' : 'false'}
              aria-describedby={showError ? errorId : undefined}
              spellCheck={false}
              autoCapitalize="off"
              autoComplete="off"
              rows={4}
            />
            {showSuggestions && suggestions.length > 0 && (
              <div className="auto-dropdown" role="listbox">
                {suggestions.map((s, i) => (
                  <div
                    key={s}
                    role="option"
                    aria-selected={i === selectedIdx}
                    className={`auto-item ${i === selectedIdx ? 'selected' : ''}`}
                    onMouseDown={() => acceptSuggestion(s)}
                  >
                    <span className="auto-item-text">{s}</span>
                    {i === selectedIdx && <kbd className="auto-item-hint">Enter</kbd>}
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="ctl-standard" aria-live="polite">
            <span className="ctl-standard-label">{t('ctl.standardCtl')}</span>
            {preview && !showError
              ? <code className="ctl-standard-formula">{prettyCtl(preview)}</code>
              : <span className="ctl-standard-empty">{t('ctl.standardEmpty')}</span>}
          </div>

          <div className="ctl-bench-bar">
            <p id={errorId} className={`ctl-status ${showError ? 'error' : ''}`} role="status">
              {showError ? validation.message : (formula.trim() ? t('ctl.wellFormed') : t('ctl.idle'))}
            </p>
            <div className="ctl-actions">
              <button type="button" className="ctl-clear-btn" disabled={!formula} onClick={() => { setFormula(''); setValidation({ valid: true, message: '' }); textareaRef.current?.focus(); updateSuggestions('', 0, false); }}>
                {t('ctl.clear')}
              </button>
              <button type="button" className="ctl-check-btn" onClick={checkFormula} disabled={blocked || loading || !formula.trim() || !validation.valid}>
                {loading ? t('ctl.verifying') : t('ctl.verify')}
                {!loading && <kbd>Ctrl ↵</kbd>}
              </button>
            </div>
          </div>
        </section>

        <CTLResult result={result} loading={loading} />
      </div>

      <aside className="ctl-ref" aria-label={t('ctl.reference')}>
        <section className="ctl-ref-block">
          <h3 className="ctl-ref-title">{t('ctl.examples')}</h3>
          <p className="ctl-ref-note">{t('ctl.pathHint')}</p>
          <ul className="ctl-examples-list">
            {examples.map((ex, i) => (
              <li key={i}>
                <button type="button" className="ctl-example-btn" onClick={() => insertExample(ex)} title={t('ctl.insert')}>
                  <code>{highlight(ex)}</code>
                </button>
              </li>
            ))}
          </ul>
        </section>

        <section className="ctl-ref-block">
          <h3 className="ctl-ref-title">{t('ctl.grammarRef')}</h3>
          {grammar.map((section, i) => (
            <div key={i} className="ctl-grammar-section">
              <h4 className="ctl-grammar-label">{t(section.labelKey)}</h4>
              <dl className="ctl-grammar-list">
                {section.items.map((item, j) => (
                  <div key={j} className="ctl-grammar-item">
                    <dt>
                      <button type="button" className="ctl-chip" onClick={() => insertSnippet(item.syntax)} title={t('ctl.insert')}>
                        {item.syntax}
                      </button>
                    </dt>
                    <dd className="ctl-grammar-meaning">{t(item.meaningKey)}</dd>
                  </div>
                ))}
              </dl>
            </div>
          ))}
        </section>
      </aside>
    </div>
  );
};

export default CTLEditor;
