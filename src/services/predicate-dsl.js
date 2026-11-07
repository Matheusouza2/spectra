/**
 * Render predicate conjunctions (from `build_predicate`) in the query language.
 * A predicate is a disjunction of conjunctions of `low <= place <= upp` bounds.
 */
export function boundToDsl({ place, low, upp }, dsl) {
  if (upp != null && upp === low) return `${place} = ${low}`;
  const parts = [];
  if (low > 0 || upp == null) parts.push(`${place} >= ${low}`);
  if (upp != null) parts.push(`${place} <= ${upp}`);
  return parts.join(` ${dsl.and} `);
}

export function conjunctionToDsl(conjunction, dsl) {
  // `p >= 0` with no upper bound constrains nothing.
  const parts = conjunction.bounds
    .filter((b) => !(b.low === 0 && b.upp == null))
    .map((b) => boundToDsl(b, dsl));
  if (parts.length === 0) return 'TRUE';
  return `(${parts.join(` ${dsl.and} `)})`;
}

export function predicateToDsl(conjunctions, dsl) {
  return conjunctions.map((c) => conjunctionToDsl(c, dsl)).join(` ${dsl.or} `);
}

/** "exists path eventually (<predicate>)": the shortest-path style query. */
export function reachabilityQuery(predicate, dsl) {
  return `${dsl.exists} ${dsl.temporal.eventually} (${predicate})`;
}

/** "0-3, 7" -> [0, 1, 2, 3, 7]; null when something cannot be read. */
export function parseStateList(text) {
  const out = [];
  for (const part of text.split(',').map((p) => p.trim()).filter(Boolean)) {
    const match = /^M?(\d+)(?:\s*-\s*M?(\d+))?$/i.exec(part);
    if (!match) return null;
    const from = Number(match[1]);
    const to = match[2] === undefined ? from : Number(match[2]);
    if (to < from || to - from > 5000) return null;
    for (let n = from; n <= to; n += 1) out.push(n);
  }
  return out;
}
