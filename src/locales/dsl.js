// CTL query DSL vocabulary per UI language.
// The Rust parser (racon-ctl-dsl) accepts both vocabularies at once; this module
// only decides which one the interface shows, suggests and documents.
// Keep in sync with Tokenizer::match_keyword in crates/racon-ctl-dsl/src/parser.rs.

export const DSL = {
  'en-US': {
    forAll: 'for all paths',
    exists: 'exists path',
    forAllWords: 'for all',
    existsWords: 'exists',
    temporal: { always: 'always', eventually: 'eventually', next: 'next', until: 'until' },
    not: 'not',
    and: 'AND',
    or: 'OR',
    startRegex: /^(for\s+all\s+paths?|exists\s+(a\s+)?path)/i,
    afterQuantifierRegex: /(for\s+all\s+paths?|exists\s+(a\s+)?path)\s*$/,
    notPrefixRegex: /^not(\s|$)/,
    exampleSecond: (a, b, tr = 't0') => [
      `exists path eventually ${a} = 1`,
      `exists path next ${a} >= 1`,
      `exists path eventually enabled(${tr})`,
      `exists path eventually deadlock`,
      `exists path eventually (${a} = 1 AND ${b} = 0)`,
      `exists path eventually (${a} >= 1 OR ${b} >= 1)`,
      `exists path eventually not (${b} = 0)`,
      `for all paths next ${a} >= 1`,
      `for all paths always ${a} >= 0`,
      `for all paths always not (${a} = 0 AND ${b} = 0)`,
      `for all paths always TRUE`,
      `exists path eventually FALSE`,
      `exists path (${a} = 1) until (${b} = 0)`,
      `for all paths (${a} >= 1) until (${b} = 0)`,
    ],
    atomExample: 'p0 = 1',
    enabledExample: 'enabled(t0, t1)',
    deadlockExample: 'deadlock',
    quoteExample: '`B` >= 1',
    linearExample: 'p0 + p1 >= 2',
  },
  'pt-BR': {
    forAll: 'para todo caminho',
    exists: 'existe caminho',
    forAllWords: 'para todos',
    existsWords: 'existe',
    temporal: { always: 'sempre', eventually: 'eventualmente', next: 'próximo', until: 'até' },
    not: 'nao',
    and: 'E',
    or: 'OU',
    startRegex: /^(para\s+todos?\s+(os\s+)?caminhos?|existe\s+(um\s+)?caminho)/i,
    afterQuantifierRegex: /(para\s+todos?\s+(os\s+)?caminhos?|existe\s+(um\s+)?caminho)\s*$/,
    notPrefixRegex: /^(nao|não)(\s|$)/,
    exampleSecond: (a, b, tr = 't0') => [
      `existe caminho eventualmente ${a} = 1`,
      `existe caminho próximo ${a} >= 1`,
      `existe caminho eventualmente habilitada(${tr})`,
      `existe caminho eventualmente bloqueio`,
      `existe caminho eventualmente (${a} = 1 E ${b} = 0)`,
      `existe caminho eventualmente (${a} >= 1 OU ${b} >= 1)`,
      `existe caminho eventualmente nao (${b} = 0)`,
      `para todo caminho próximo ${a} >= 1`,
      `para todos os caminhos sempre ${a} >= 0`,
      `para todos os caminhos sempre nao (${a} = 0 E ${b} = 0)`,
      `para todos os caminhos sempre TRUE`,
      `existe caminho eventualmente FALSE`,
      `existe caminho (${a} = 1) até (${b} = 0)`,
      `para todo caminho (${a} >= 1) até (${b} = 0)`,
    ],
    atomExample: 'p0 = 1',
    enabledExample: 'habilitada(t0, t1)',
    deadlockExample: 'bloqueio',
    quoteExample: '`B` >= 1',
    linearExample: 'p0 + p1 >= 2',
  },
};

export const dslFor = (language) => DSL[language] ?? DSL['en-US'];

// Grammar reference rows shown in the editor; meanings come from the `ctl` locale namespace.
export function grammarSections(language) {
  const dsl = dslFor(language);
  const { always, eventually, next, until } = dsl.temporal;
  const pt = language === 'pt-BR';
  const c = pt ? 'condição' : 'condition';
  return [
    { labelKey: 'ctl.grammar.quantifiers', items: [
      { syntax: dsl.forAll, meaningKey: 'ctl.grammar.ag' },
      { syntax: dsl.exists, meaningKey: 'ctl.grammar.ef' },
    ] },
    { labelKey: 'ctl.grammar.temporal', items: [
      { syntax: `${always} (${c})`, meaningKey: 'ctl.grammar.g' },
      { syntax: `${eventually} (${c})`, meaningKey: 'ctl.grammar.f' },
      { syntax: `${next} (${c})`, meaningKey: 'ctl.grammar.x' },
      { syntax: `${dsl.exists} (${c}1) ${until} (${c}2)`, meaningKey: 'ctl.grammar.u' },
      { syntax: `${dsl.forAll} (${c}1) ${until} (${c}2)`, meaningKey: 'ctl.grammar.au' },
    ] },
    { labelKey: 'ctl.grammar.atomic', items: [
      { syntax: 'TRUE / FALSE', meaningKey: 'ctl.grammar.constants' },
      { syntax: `${dsl.not} (${c})`, meaningKey: 'ctl.grammar.not' },
      { syntax: dsl.atomExample, meaningKey: 'ctl.grammar.place' },
      { syntax: dsl.enabledExample, meaningKey: 'ctl.grammar.enabled' },
      { syntax: dsl.deadlockExample, meaningKey: 'ctl.grammar.deadlock' },
      { syntax: dsl.linearExample, meaningKey: 'ctl.grammar.linear' },
      { syntax: dsl.quoteExample, meaningKey: 'ctl.grammar.quoted' },
    ] },
    { labelKey: 'ctl.grammar.boolean', items: [
      { syntax: `(A) ${dsl.and} (B)`, meaningKey: 'ctl.grammar.and' },
      { syntax: `(A) ${dsl.or} (B)`, meaningKey: 'ctl.grammar.or' },
    ] },
  ];
}
