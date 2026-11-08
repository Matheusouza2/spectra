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
//   temporal  := "sempre" (AG) | "eventualmente" (EF) | "próximo" (EX)
//   until     := quantifier (condition) "até" (condition)      -- E[φ U ψ] / A[φ U ψ]
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
// Scans the text before the caret into tokens and derives what the grammar allows next,
// so the list only offers valid continuations (never the whole vocabulary).

const QUANT_RE = /^\s*(for\s+all\s+paths?|exists\s+(?:a\s+)?path|para\s+todos?\s+(?:os\s+)?caminhos?|existe\s+(?:um\s+)?caminho)(?![\p{L}\d_])/iu;
const TOKEN_SCAN_RE = /\s*(<=|>=|!=|=|<|>|\(|\)|,|\d+|[\p{L}_][\p{L}\d_]*)/gyu;
const TEMPORAL_WORDS = new Set(['always', 'eventually', 'next', 'sempre', 'eventualmente', 'próximo', 'proximo']);
const UNTIL_WORDS = new Set(['until', 'até', 'ate']);
const NOT_WORDS = new Set(['not', 'nao', 'não']);
const AND_WORDS = new Set(['and', 'e']);
const OR_WORDS = new Set(['or', 'ou']);
const ENABLED_WORDS = new Set(['enabled', 'habilitada']);
const COMPARATORS = ['=', '!=', '<', '>', '<=', '>='];

// Returns { items, start }: the suggestions and where in the text the accepted one replaces from.
function completionsAt(text, caretPos, { placeNames = [], transitionNames = [] }, dsl) {
  const before = text.slice(0, caretPos);
  const quantifiers = [dsl.forAll, dsl.exists];

  const quant = before.match(QUANT_RE);
  if (!quant || quant[0].length === before.length) {
    // Still writing (or just finished) the path quantifier.
    const start = before.length - before.trimStart().length;
    const typed = before.slice(start).toLowerCase();
    return { items: quantifiers.filter((q) => q.toLowerCase().startsWith(typed)), start };
  }

  const rest = before.slice(quant[0].length);
  const partial = (rest.match(/[\p{L}\d_]+$/u) || [''])[0];
  const head = rest.slice(0, rest.length - partial.length);

  let state = 'afterQuant';
  let hasTemporal = false;
  const stack = [];
  let pendingEnabled = false;
  TOKEN_SCAN_RE.lastIndex = 0;
  let m;
  while ((m = TOKEN_SCAN_RE.exec(head)) !== null) {
    const tok = m[1];
    const w = tok.toLowerCase();
    if (tok === '(') {
      stack.push(pendingEnabled ? 'enabled' : 'group');
      state = pendingEnabled ? 'enabledArg' : 'expr';
      pendingEnabled = false;
    } else if (tok === ')') {
      stack.pop();
      state = 'afterExpr';
    } else if (tok === ',') {
      state = 'enabledArg';
    } else if (COMPARATORS.includes(tok)) {
      state = 'afterOp';
    } else if (/^\d+$/.test(tok)) {
      state = 'afterExpr';
    } else if (state === 'enabledArg') {
      state = 'enabledNext';
    } else if (TEMPORAL_WORDS.has(w)) {
      hasTemporal = true;
      state = 'expr';
    } else if (UNTIL_WORDS.has(w) || NOT_WORDS.has(w) || AND_WORDS.has(w) || OR_WORDS.has(w)) {
      state = 'expr';
    } else if (w === 'true' || w === 'false') {
      state = 'afterExpr';
    } else if (ENABLED_WORDS.has(w)) {
      pendingEnabled = true;
    } else {
      state = 'afterPlace'; // a place name
    }
  }
  if (pendingEnabled) state = 'afterEnabledWord';

  const t = dsl.temporal;
  const enabledWord = dsl.enabledExample.split('(')[0];
  let pool;
  switch (state) {
    case 'afterQuant':
      pool = [t.always, t.eventually, t.next, '('];
      break;
    case 'expr':
      pool = ['TRUE', 'FALSE', dsl.not, '(', `${enabledWord}(`, ...placeNames];
      break;
    case 'afterPlace':
      pool = COMPARATORS;
      break;
    case 'afterOp':
      pool = ['0', '1', '2'];
      break;
    case 'afterExpr': {
      pool = [dsl.and, dsl.or];
      if (stack.length > 0) pool.unshift(')');
      else if (!hasTemporal) pool.unshift(t.until);
      break;
    }
    case 'enabledArg':
      pool = transitionNames;
      break;
    case 'enabledNext':
      pool = [',', ')'];
      break;
    case 'afterEnabledWord':
      pool = ['('];
      break;
    default:
      pool = [];
  }

  const lower = partial.toLowerCase();
  const items = pool.filter((s) => !lower || s.toLowerCase().startsWith(lower));
  // Nothing left to complete once the typed word is itself the only match.
  if (items.length === 1 && items[0].toLowerCase() === lower) return { items: [], start: caretPos - partial.length };
  return { items: items.slice(0, 8), start: caretPos - partial.length };
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
  const replaceStartRef = useRef(0);
  const { addToast } = useToast();
  const { t, tm, language } = useI18n();
  const dsl = useMemo(() => dslFor(language), [language]);
  const grammar = useMemo(() => grammarSections(language), [language]);
  const examples = useMemo(() => buildExamples(placeNames, dsl, transitionNames), [placeNames, dsl, transitionNames]);

  const updateSuggestions = useCallback(async (text, caretPos, showList = true) => {
    const requestId = ++validationRequestRef.current;
    const { items, start } = completionsAt(text, caretPos, { placeNames, transitionNames }, dsl);
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
      } catch {
        // validation is advisory here; completions are computed locally
      }
    }
    if (requestId !== validationRequestRef.current) return;
    replaceStartRef.current = start;
    setSuggestions(items);
    setShowSuggestions(showList && items.length > 0);
    setSelectedIdx(-1);
  }, [filePath, placeNames, transitionNames, t, dsl, language]);

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
    const prefix = formula.slice(0, Math.min(replaceStartRef.current, caretPos));
    const after = formula.slice(caretPos).replace(/^[\p{L}\d_]+/u, '');
    const glue = /[(]$/.test(suggestion) || suggestion === ')' || suggestion === ',' ? '' : ' ';
    const newFormula = prefix + suggestion + glue + after;
    const newPos = prefix.length + suggestion.length + glue.length;

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
