// Standard CTL notation: ASCII connectives become logic symbols.
// The backend already parenthesises every operand, so the text is the executed formula.
export function prettyCtl(text) {
  return String(text ?? '')
    .replace(/ AND /g, ' ∧ ')
    .replace(/ OR /g, ' ∨ ')
    .replace(/ -> /g, ' → ')
    .replace(/!\(/g, '¬(');
}
