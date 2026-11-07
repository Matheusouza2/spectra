/** Semantic token DTO helpers shared by graph and marking views. */
export function tokenValueToString(token) {
  if (token && token.kind === 'omega') return 'ω';
  if (token && token.kind === 'finite') return token.value;
  return String(token ?? 0);
}

export function markingToString(marking) {
  return Array.isArray(marking) ? marking.map(tokenValueToString).join(', ') : String(marking);
}

/** Legacy numeric adapter used only by the interactive token game input. */
export function tokenValueToNumber(token) {
  if (token && token.kind === 'omega') return 0;
  const value = token && token.kind === 'finite' ? token.value : token;
  const number = Number(value ?? 0);
  return Number.isSafeInteger(number) && number >= 0 ? number : 0;
}

export function markingToNumbers(marking) {
  return Array.isArray(marking) ? marking.map(tokenValueToNumber) : [];
}
