const COMMANDS = new Map([
  ['\\forall', '∀'],
  ['\\exists', '∃'],
  ['\\in', '∈'],
  ['\\le', '≤'],
  ['\\ge', '≥'],
  ['\\ne', '≠'],
  ['\\neg', '¬'],
  ['\\cap', '∩'],
  ['\\cup', '∪'],
  ['\\bigcup', '⋃'],
  ['\\rightarrow', '→'],
  ['\\to', '→'],
  ['\\varnothing', '∅'],
  ['\\emptyset', '∅'],
  ['\\land', '∧'],
  ['\\lor', '∨'],
  ['\\Rightarrow', '⇒'],
  ['\\wedge', '∧'],
  ['\\vee', '∨'],
  ['\\bullet', '•'],
  ['\\triangle', '△'],
  ['\\rangle', '⟩'],
  ['\\langle', '⟨'],
  ['\\sigma', 'σ'],
  ['\\ldots', '…'],
  ['\\cdots', '⋯'],
  ['\\dots', '…'],
  ['\\leq', '≤'],
  ['\\geq', '≥'],
]);

const SUBSCRIPT = {
  '0': '₀', '1': '₁', '2': '₂', '3': '₃', '4': '₄',
  '5': '₅', '6': '₆', '7': '₇', '8': '₈', '9': '₉',
  a: 'ₐ', e: 'ₑ', h: 'ₕ', i: 'ᵢ', j: 'ⱼ', k: 'ₖ',
  l: 'ₗ', m: 'ₘ', n: 'ₙ', o: 'ₒ', p: 'ₚ', r: 'ᵣ',
  s: 'ₛ', t: 'ₜ', u: 'ᵤ', v: 'ᵥ', x: 'ₓ',
};

const SUPERSCRIPT = {
  '0': '⁰', '1': '¹', '2': '²', '3': '³', '4': '⁴',
  '5': '⁵', '6': '⁶', '7': '⁷', '8': '⁸', '9': '⁹',
  '+': '⁺', '-': '⁻', '(': '⁽', ')': '⁾', n: 'ⁿ', i: 'ⁱ', T: 'ᵀ',
};

function scriptText(value, table) {
  return [...value].map((character) => table[character] || character).join('');
}

function readGroup(source, start) {
  if (source[start] !== '{') return { value: source[start] || '', next: start + 1 };
  let depth = 0;
  for (let i = start; i < source.length; i += 1) {
    if (source[i] === '{') depth += 1;
    if (source[i] === '}') {
      depth -= 1;
      if (depth === 0) return { value: source.slice(start + 1, i), next: i + 1 };
    }
  }
  return { value: source.slice(start + 1), next: source.length };
}

function replaceScripts(source) {
  let result = '';
  for (let i = 0; i < source.length; i += 1) {
    if (source[i] !== '^' && source[i] !== '_') {
      result += source[i];
      continue;
    }
    const script = source[i] === '^' ? SUPERSCRIPT : SUBSCRIPT;
    const group = readGroup(source, i + 1);
    result += scriptText(group.value, script);
    i = group.next - 1;
  }
  return result;
}

export function latexToPresentationText(latex) {
  if (latex == null) return '';
  let result = String(latex).replace(/\\operatorname\s*\{([^{}]*)\}/g, '$1');
  result = result.replace(/\\text\s*\{([^{}]*)\}/g, '$1');
  for (const [command, symbol] of COMMANDS) result = result.split(command).join(symbol);
  result = result.replace(/\\([a-zA-Z]+)/g, '$1');
  result = result.replace(/[{}]/g, '');
  result = replaceScripts(result);
  result = result.replace(/\s*([∀∃∈≤≥≠¬∩∪⋃→∧∨⇒…⋯])\s*/g, ' $1 ');
  result = result.replace(/\s+([,;])/g, '$1').replace(/([,;])\s*/g, '$1 ');
  result = result.replace(/\s*:\s*/g, ': ').replace(/\s*=\s*/g, ' = ');
  return result.replace(/\s+/g, ' ').trim();
}
