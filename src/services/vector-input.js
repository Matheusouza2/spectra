/**
 * Read "3, 0, 1" into one entry per slot: a number, 'missing' (not typed yet) or 'invalid'.
 * Shared by the marking and vector fields.
 */
export const MAX_ABS = 2147483647; // the backend takes 32-bit values per place

export function parseVector(text, count, { allowNegative = false } = {}) {
  const trimmed = text.trim();
  let tokens = trimmed === '' ? [] : text.split(',').map((part) => part.trim());
  // "3, 0," is a value still being typed, not an empty entry.
  if (tokens.length > 0 && tokens[tokens.length - 1] === '' && text.trimEnd().endsWith(',')) tokens = tokens.slice(0, -1);

  const pattern = allowNegative ? /^-?\d+$/ : /^\d+$/;
  const cells = Array.from({ length: count }, (_, i) => {
    if (i >= tokens.length) return { kind: 'missing' };
    const raw = tokens[i];
    if (!pattern.test(raw) || Math.abs(Number(raw)) > MAX_ABS) return { kind: 'invalid', raw };
    return { kind: 'value', value: Number(raw) };
  });
  const firstInvalid = cells.findIndex((c) => c.kind === 'invalid');
  return {
    cells,
    count: tokens.length,
    extra: Math.max(0, tokens.length - count),
    firstInvalid,
    valid: count > 0 && tokens.length === count && firstInvalid === -1,
    values: cells.map((c) => c.value),
  };
}
