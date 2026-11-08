// Audit map of every computation SPECTRA performs. Prose lives in locales/<lang>/method.json
// under `sections.<section>.items.<item>`; this file holds what must not be translated:
// formulas, the Rust crate that implements each item, and bibliography keys.
//
// `refs` uses ids from the backend catalog (R1…R13) or the local CLASSIC_REFS below.

export const CLASSIC_REFS = [
  { id: 'C1', citation: 'MURATA, T. Petri nets: properties, analysis and applications. Proceedings of the IEEE 77(4):541-580', year: 1989, doi: '10.1109/5.24143' },
  { id: 'C2', citation: 'TARJAN, R. Depth-first search and linear graph algorithms. SIAM Journal on Computing 1(2):146-160', year: 1972, doi: '10.1137/0201010' },
  { id: 'C3', citation: 'CLARKE, E. M.; EMERSON, E. A.; SISTLA, A. P. Automatic verification of finite-state concurrent systems using temporal logic specifications. ACM TOPLAS 8(2):244-263', year: 1986, doi: '10.1145/5397.5399' },
  { id: 'C4', citation: 'STARKE, P. H. Analyse von Petri-Netz-Modellen. Teubner, Stuttgart', year: 1990, doi: null },
];

export const SECTIONS = [
  {
    id: 'model',
    items: [
      { id: 'firing', crate: 'racon-core',
        formulas: ['enabled(t, m)  ⇔  ∀p: m(p) ≥ W(p, t)', "m′(p) = m(p) − W(p, t) + W(t, p)"], refs: ['C1'] },
      { id: 'state-equation', crate: 'racon-core',
        formulas: ['C = Post − Pre   (C ∈ ℤ^(|P|×|T|))', 'm = m₀ + C · x   (x = Parikh vector of the firing sequence)'], refs: ['C1'] },
    ],
  },
  {
    id: 'structural',
    items: [
      { id: 'classes', crate: 'racon-structural',
        formulas: ['ORD:  W(f) = 1 for every arc f', 'SM:   |•t| = |t•| = 1 for every t', 'MG:   |•p| = |p•| = 1 for every p', 'FC:   •t₁ ∩ •t₂ ≠ ∅ ⇒ •t₁ = •t₂ = {p}   (the shared place is the only input)', 'EFC:  •t₁ ∩ •t₂ ≠ ∅ ⇒ •t₁ = •t₂'], refs: ['C1', 'C4'] },
      { id: 'siphons-traps', crate: 'racon-structural',
        formulas: ['siphon S:  t• ∩ S ≠ ∅ ⇒ •t ∩ S ≠ ∅', 'trap Q:    •t ∩ Q ≠ ∅ ⇒ t• ∩ Q ≠ ∅'], refs: ['C1'] },
    ],
  },
  {
    id: 'invariants',
    items: [
      { id: 'p-invariants', crate: 'racon-invariants',
        formulas: ['yᵀ · C = 0', 'yᵀ · m = yᵀ · m₀   for every reachable m'], refs: ['C1'] },
      { id: 't-invariants', crate: 'racon-invariants',
        formulas: ['C · x = 0,   x ≥ 0'], refs: ['C1'] },
      { id: 'coverage', crate: 'racon-invariants, racon-structural',
        formulas: ['CPI: ∀p ∃y ≥ 0, yᵀC = 0, y(p) > 0   ⇒  bounded', 'CTI: ∀t ∃x ≥ 0, Cx = 0, x(t) > 0'], refs: ['C1'] },
    ],
  },
  {
    id: 'statespace',
    items: [
      { id: 'reachability', crate: 'racon-reachability',
        formulas: ['Reach(N, m₀) = { m | ∃σ ∈ T*: m₀ —σ→ m }', 'RG = (Reach, {(m, t, m′) | m —t→ m′}, m₀)', 'BFS: depth(m) = δ(m₀, m)   (Cormen et al., Thm 20.5); cost O(V + E)', 'DFS: DFS-VISIT with an explicit stack; cost Θ(V + E)'], refs: ['R15', 'C1'] },
      { id: 'coverability', crate: 'racon-coverability',
        formulas: ['ω + k = ω;   k ≤ ω', 'if m′ ≥ m (ancestor) and m′ ≠ m:  m′(p) ← ω wherever m′(p) > m(p)'], refs: ['R1', 'R2', 'R6'] },
      { id: 'mincov', crate: 'racon-coverability (mincov.rs)',
        formulas: ['Clover = λ(V) — the antichain of maximal ω-markings'], refs: ['R6', 'R7'] },
    ],
  },
  {
    id: 'behavioural',
    items: [
      { id: 'boundedness', crate: 'racon-coverability, racon-structural',
        formulas: ['place p is k-bounded:  ∀m ∈ Reach: m(p) ≤ k', 'unbounded ⇔ some ω-marking in the coverability tree'], refs: ['R1', 'C1'] },
      { id: 'deadlock-liveness', crate: 'racon-scc, racon-reachability',
        formulas: ['deadlock marking:  ∀t: ¬enabled(t, m)', 'L4(t):  ∀m ∈ Reach ∃σ: m —σ→ m′, enabled(t, m′)'], refs: ['C2', 'C4'] },
      { id: 'reversibility', crate: 'racon-scc',
        formulas: ['reversible ⇔ m₀ is a home state ⇔ every m ∈ Reach reaches m₀ (the finite RG is one SCC)'], refs: ['C2'] },
    ],
  },
  {
    id: 'verification',
    items: [
      { id: 'ctl', crate: 'racon-ctl, racon-ctl-dsl',
        formulas: ['EF φ = μZ. φ ∨ EX Z', 'EG φ = νZ. φ ∧ EX Z   (via SCCs)', 'AF φ = ¬EG ¬φ'], refs: ['C3', 'C2'] },
      { id: 'marking-test', crate: 'racon-invariants, racon-property',
        formulas: ['C · x = m* − m₀,  x ∈ ℕⁿ', 'no solution ⇒ Farkas certificate y:  yᵀC ≥ 0,  yᵀ(m* − m₀) < 0'], refs: ['C1'] },
      { id: 'bad-states', crate: 'racon-property',
        formulas: ['reachable bad state: ∃m ∈ Reach: φ(m)', 'witness = shortest firing sequence m₀ → m'], refs: [] },
      { id: 'evidence', crate: 'racon-evidence',
        formulas: [], refs: [] },
    ],
  },
  {
    id: 'reductions',
    items: [
      { id: 'stubborn', crate: 'racon-reduction (stubborn.rs)',
        formulas: ['explore only enabled(m) ∩ S(m), S(m) stubborn'], refs: ['R9', 'R10'] },
      { id: 'symmetry', crate: 'racon-reduction (symmetry.rs)',
        formulas: ['σ ∈ Aut(N):  σ(P) = P,  σ(T) = T,  ⟨x, y⟩ ∈ F ⇔ ⟨σx, σy⟩ ∈ F,  W(x, y) = W(σx, σy)', 'M [t⟩ M′  ⇔  σ(M) [σ(t)⟩ σ(M′)      (Lemma 3.3)', 'canrepr(M) = min<β { g⁻¹(M) | g ∈ G }   (lexicographic leader, Alg. 4.2)'], refs: ['R14'] },
      { id: 'structural-rules', crate: 'racon-reduction (query_rules.rs)',
        formulas: ['rewrite N → N′ using rules A–I, depending on places(φ)', 'reach(N, φ)  ⇔  reach(N′, φ)', 'rules A–F, H also preserve: ∃ reachable deadlock'], refs: ['R10'] },
    ],
  },
];
