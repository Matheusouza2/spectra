import test from 'node:test';
import assert from 'node:assert/strict';
import { latexToPresentationText } from './latex.js';

test('converte quantificadores, subscritos e simbolos da definicao de limitacao', () => {
  const source = String.raw`\forall p\in P,\exists k_p:\forall M\in R(M_0), M(p)\le k_p`;

  assert.equal(
    latexToPresentationText(source),
    '∀ p ∈ P, ∃ kₚ: ∀ M ∈ R(M₀), M(p) ≤ kₚ',
  );
});

test('preserva texto desconhecido sem criar markup executavel', () => {
  const source = String.raw`\operatorname{Enabled}(M)=\varnothing`;

  assert.equal(
    latexToPresentationText(source),
    'Enabled(M) = ∅',
  );
});

test('converte bullet de pre e pos conjunto em simbolo matematico', () => {
  const source = String.raw`\forall t:{}^\bullet t\cap t^\bullet=\varnothing`;

  assert.equal(
    latexToPresentationText(source),
    '∀ t: • t ∩ t• = ∅',
  );
});

test('preserva a transposta como sobrescrito e a notacao de suporte', () => {
  assert.equal(latexToPresentationText(String.raw`y^TC=0`), 'yᵀC = 0');
  assert.equal(latexToPresentationText(String.raw`\operatorname{supp}(x)=P`), 'supp(x) = P');
  assert.equal(latexToPresentationText('supp(x)=P'), 'supp(x) = P');
});

test('converte triangle em simbolo geometrico', () => {
  assert.equal(latexToPresentationText(String.raw`\triangle p`), '△ p');
});

test('converte a notacao de disparo de Petri net', () => {
  assert.equal(latexToPresentationText(String.raw`M[t\rangle`), 'M[t⟩');
  assert.equal(latexToPresentationText(String.raw`\langle\sigma\rangle`), '⟨σ⟩');
});

test('converte reticencias matematicas', () => {
  assert.equal(latexToPresentationText(String.raw`[0,\ldots,0]`), '[0, …, 0]');
  assert.equal(latexToPresentationText(String.raw`x_1\cdots x_n`), 'x₁ ⋯ xₙ');
});
