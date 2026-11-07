// src/services/tauri-mock.js

/**
 * Mock implementation of Tauri's invoke function for browser-based development.
 */
const finite = (value) => ({ kind: 'finite', value: String(value) });
const semanticMarking = (values) => values.map(finite);
const semanticVector = (values) => values.map(String);
const mockEvidence = [
  'B', 'DST', 'DTR', 'REV', 'L', 'LS', 'DCF',
  'ORD', 'HOM', 'CON', 'SC', 'PUR', 'SCF', 'MG', 'FC', 'EFC',
  'P_INVARIANTS', 'T_INVARIANTS', 'CPI', 'CTI', 'CSV', 'SB',
].map((property) => ({
  property,
  sequence: null,
  result: true,
  status: 'PROVEN',
  definition: { name: property, formula_latex: 'evidência de desenvolvimento (mock)', explanation: 'O mock mantém o mesmo contrato de DTO do backend.' },
  method: 'REACHABILITY_GRAPH',
  certificate: { kind: 'mock', property },
  counterexample: null,
  proof: {
    kind: 'PROOF',
    proposition: 'evidência de desenvolvimento (mock)',
    assumptions: ['Espaço de estados exato simulado para desenvolvimento.'],
    facts: [{ label: 'Estados verificados', value: '10', formula_latex: '|R(M_0)|' }],
    derivation: [{ text: 'O mock fornece uma derivação mínima para exercitar a mesma interface do backend.', formula_latex: null }],
    conclusion: `${property} = TRUE`,
    completeness: 'Contrato simulado; não representa uma análise científica real.',
    tables: [],
  },
  assumptions: [],
  references: [],
  metadata: { analyzer_version: 'mock', algorithm: 'mock', exact_state_space: true, node_count: 10, edge_count: 12 },
}));

export const invoke = async (cmd, args) => {
  console.log(`[Tauri Mock] Invoking command: ${cmd}`, args);

  // Simulate network delay
  await new Promise(r => setTimeout(r, 300));

  switch (cmd) {
    case 'analyze_pnt':
      return {
        initial_marking: semanticMarking([1, 0, 0]),
        bounded: true,
        exact_graph_complete: true,
        analysis_status: "exact_reachability_complete",
        bad_state_safe: true,
        safe: true,
        live: true,
        place_live: true,
        unbounded_places: [],
        place_liveness: [
          { place: 0, name: "P1", live: true },
          { place: 1, name: "P2", live: true },
          { place: 2, name: "P3", live: true }
        ],
        dead_places: [],
        deadlock_free: true,
        dynamic_conflict_free: true,
        persistent: true,
        gfc: true,
        deadlock_trap: null,
        deadlock_nodes: [],
        dead_transitions: [],
        reversible: true,
        non_reversible_nodes: [],
        liveness: [
          { transition: 0, name: "t0", level: "L4" },
          { transition: 1, name: "t1", level: "L4" }
        ],
        has_self_loops: false,
        self_loops: [],
        structural: {
          pure: true,
          ordinary: true,
          homogeneous: true,
          connected: true,
          strongly_connected: true,
          marked_graph: true,
          free_choice: true,
          extended_free_choice: true,
          state_machine: true,
          static_conflict_free: true,
          structurally_concurrent: true,
          extended_simple: true,
          non_blocking: true,
          state_machine_coverable: true,
          state_machine_decomposable: true,
          state_machine_allocatable: true,
          transitions_without_preset: true,
          transitions_without_postset: true,
          places_without_preset: true,
          places_without_postset: true
        },
        invariant: {
          minimal_p_semiflows: [semanticVector([1, 0, 1]), semanticVector([0, 1, 0])],
          minimal_t_semiflows: [semanticVector([1, 1, 0])],
          structurally_bounded: true,
          conservative: true,
          p_invariant_coverage: true,
          t_invariant_coverage: true
        },
        analysis_info: {
          num_states: 10,
          duration_ms: 5
        },
        net_places: 3,
        net_transitions: 2,
        total_arcs: 6,
        total_tokens: finite(1),
        evidence: mockEvidence,
      };

    case 'check_cycle_fairness':
      return { fair: true, state_fair: true };

    case 'get_deadlock_trap_property':
      return true;

    case 'get_required_initial_properties':
      return {
        conservative: false,
        structurally_bounded: true,
        bounded: true,
        live: false,
        live_and_safe: false,
        warning: null,
      };

    case 'get_analysis_evidence':
      return mockEvidence;

    case 'check_place_boundedness':
      return { place: 'P1', bounded: true, visited: 1 };

    case 'check_transition_dead':
      return { transition: 'T1', dead: false, path: [], visited: 1 };

    case 'check_bad_states':
      return { safe: true, results: [] };

    case 'get_net_info':
      return {
        num_places: 3,
        num_transitions: 2,
        place_names: ["P1", "P2", "P3"],
        transition_names: ["T1", "T2"],
        initial_marking: semanticMarking([1, 0, 0]),
      };

    case 'get_algorithm_catalog':
      return {
        catalog_version: 'mock',
        references: [],
        algorithms: [
          { id: 'reachability_bfs', name: 'Alcançabilidade em largura (BFS)', role: 'exploration', status: 'stable', selectable: true, objectives: ['reachability'], supported_net_classes: ['PT_WEIGHTED', 'EXTENDED_PT'], description: 'Mock', when_to_use: '', when_not_to_use: '', output: 'grafo exato', preserves: [], does_not_preserve: [], reference_ids: [], implementation_version: 'mock', notes: '' },
          { id: 'coverability_karp_miller', name: 'Karp–Miller', role: 'exploration', status: 'stable', selectable: true, objectives: ['coverability'], supported_net_classes: ['PT_WEIGHTED'], description: 'Mock', when_to_use: '', when_not_to_use: '', output: 'árvore', preserves: [], does_not_preserve: [], reference_ids: [], implementation_version: 'mock', notes: '' },
        ],
      };

    case 'validate_analysis_config':
      return { status: 'valid', applied: [], ignored: [], issues: [], warnings: [], automatic_decisions: [{ id: 'reachability_bfs', reason: 'mock' }], pipeline: ['mock'], preserves: [], does_not_preserve: [], experimental: false };

    case 'get_net_structure':
      return {
        places: [
          { id: 0, name: "P1", tokens: finite(1) },
          { id: 1, name: "P2", tokens: finite(0) },
          { id: 2, name: "P3", tokens: finite(0) },
        ],
        transitions: [
          { id: 0, name: "T1" },
          { id: 1, name: "T2" },
        ],
        arcs: [
          { source: "P1", target: "T1", weight: 1 },
          { source: "T1", target: "P2", weight: 1 },
          { source: "P2", target: "T2", weight: 1 },
          { source: "T2", target: "P3", weight: 1 },
          { source: "P3", target: "T1", weight: 1 },
        ],
      };

    case 'get_reachability_graph':
      return {
        nodes: [
          { id: 0, marking: semanticMarking([1, 0, 0]), is_omega: false },
          { id: 1, marking: semanticMarking([0, 1, 0]), is_omega: false },
          { id: 2, marking: semanticMarking([0, 0, 1]), is_omega: false },
        ],
        edges: [
          { source: 0, target: 1, transition: 0, is_abstract: false },
          { source: 1, target: 2, transition: 1, is_abstract: false },
          { source: 2, target: 0, transition: 0, is_abstract: false },
        ],
      };

    case 'get_coverage_tree':
      return {
        nodes: [{ id: 0, marking: semanticMarking([1, 0, 0]), is_omega: false }],
        edges: [],
        is_bounded: true,
        max_states_exceeded: false,
      };

    case 'get_structural_properties':
      return {
        pure: true,
        ordinary: true,
        homogeneous: true,
        connected: true,
        strongly_connected: true,
        marked_graph: true,
        free_choice: true,
        extended_free_choice: true,
        state_machine: true,
        static_conflict_free: true,
        structurally_concurrent: true,
        extended_simple: true,
        non_blocking: true,
        state_machine_coverable: true,
        state_machine_decomposable: true,
        state_machine_allocatable: true,
        transitions_without_preset: true,
        transitions_without_postset: true,
        places_without_preset: true,
        places_without_postset: true,
      };

    case 'get_invariants':
      return {
        minimal_p_semiflows: [semanticVector([1, 0, 1]), semanticVector([0, 1, 0])],
        minimal_t_semiflows: [semanticVector([1, 1, 0])],
        structurally_bounded: true,
        conservative: true,
        p_invariant_coverage: true,
        t_invariant_coverage: true,
        p_sub_invariants: [{ coefficients: ['1', '1', '0'], residual: ['-1', '0', '0'], initial_value: '2' }],
        p_sur_invariants: [],
        t_sub_invariants: [{ coefficients: ['0', '1', '1'], residual: ['-1', '0', '1'], initial_value: null }],
        t_sur_invariants: [],
        p_basis: { vectors: [semanticVector([1, 1, 1])], rank: 2, dimension: 1 },
        t_basis: { vectors: [], rank: 2, dimension: 0 },
      };

    case 'compute_behavioral_properties':
      return {
        state_count: 3, deadlock_free: true, deadlock_nodes: [], dead_transitions: [], reversible: true,
        live: true, place_live: true, safe: true, live_and_safe: true, persistent: true, gfc: true,
        dynamic_conflict_free: true,
        liveness: [{ transition: 0, name: 't0', level: 'L4' }],
        place_liveness: [{ place: 0, name: 'p0', live: true }],
        scc: { count: 1, terminal_count: 1, trivial_count: 0, largest: 3, home_state_count: 3, components: [{ id: 0, size: 3, terminal: true, trivial: false, contains_initial: true, has_self_loop: false, states: ['M0', 'M1', 'M2'] }] },
      };

    case 'check_marking_coverability': {
      const total = args.marking.reduce((a, b) => a + b, 0);
      return total <= 1
        ? { covered: true, bounded: true, node_count: 3, path_has_omega: false, covering_marking: semanticMarking([0, 1, 0]), path: [
            { marking: semanticMarking([1, 0, 0]), transition: null, transition_name: null },
            { marking: semanticMarking([0, 1, 0]), transition: 0, transition_name: 'T1' } ] }
        : { covered: false, bounded: true, node_count: 3, path_has_omega: false, covering_marking: null, path: [] };
    }

    case 'build_predicate':
      return args.kind === 'enabled'
        ? { conjunctions: [{ label: args.transitions[0], bounds: [{ place: 'P1', low: 1, upp: null }] }], pdc: 'predicate for net:   0 BNA\nConjunction nr:  0\nm + p0.P1                low =  1  upp = oo\n@\n' }
        : { conjunctions: args.states.map((s) => ({ label: `M${s}`, bounds: [{ place: 'P1', low: 1, upp: 1 }, { place: 'P2', low: 0, upp: 0 }] })), pdc: 'predicate for net:   0 BNA\n@\n' };

    case 'compute_circuits':
      return { state_count: 3, total_found: 2, length_counts: args.request.mode === 'all' ? [0, 0, 1, 1] : [], shortest: 2, longest: 3, exhaustive: true, list_truncated: false,
        circuits: [
          { length: 2, states: ['M0', 'M1'], parikh: [{ name: 'T1', count: 1 }, { name: 'T2', count: 1 }], not_fired: [], fair: true, state_fair: true, steps: [
            { marking: semanticMarking([1, 0, 0]), transition: null, transition_name: null },
            { marking: semanticMarking([0, 1, 0]), transition: 0, transition_name: 'T1' },
            { marking: semanticMarking([1, 0, 0]), transition: 1, transition_name: 'T2' } ] } ] };

    case 'compute_distances':
      return { state_count: 3, histogram: [1, 1, 1], eccentricity: 2, unreachable: 0, diameter: 2, strongly_connected: false,
        target: args.to ? { distance: 2, path: [
          { marking: semanticMarking([1, 0, 0]), transition: null, transition_name: null },
          { marking: semanticMarking([0, 1, 0]), transition: 0, transition_name: 'T1' },
          { marking: semanticMarking([0, 0, 1]), transition: 1, transition_name: 'T2' } ] } : null };

    case 'test_marking_algebraic': {
      const total = args.marking.reduce((a, b) => a + b, 0);
      const names = { place_names: ['P1', 'P2', 'P3'], transition_names: ['T1', 'T2'] };
      return total !== 1
        ? { verdict: 'unreachable', method: 'p_invariant', invariant: ['1', '1', '1'], invariant_initial: '1', invariant_target: String(total), ...names }
        : { verdict: 'possibly_reachable', parikh: ['0', '0'], ...names };
    }

    case 'test_vector': {
      const zero = args.vector.every((v) => v === 0);
      return {
        kind: args.kind, residual: args.kind === 'place' ? ['0', '0'] : ['0', '0', '0'], semipositive: !zero && args.vector.every((v) => v >= 0),
        positive: args.vector.every((v) => v > 0), invariant: true, sub: true, sur: true, support: [], initial_value: args.kind === 'place' ? '1' : null,
        names: args.kind === 'place' ? ['P1', 'P2', 'P3'] : ['T1', 'T2'], residual_names: args.kind === 'place' ? ['T1', 'T2'] : ['P1', 'P2', 'P3'],
      };
    }

    case 'check_ctl_formula':
      return {
        satisfied: true,
        count: 2,
        total_states: 3,
        time: 1.2,
        formula: args.formula,
        exact_state_space: true,
        witness: { description: 'Mock witness', nodes: [0, 1], transitions: [0], is_infinite: false, cycle_start: null },
        counterexample: null,
      };

    case 'validate_ctl_formula':
      return {
        valid: true,
        formula: args.formula,
        preview: args.formula,
        error: null,
        suggestions: args.language?.startsWith('pt') ? ['sempre', 'eventualmente', 'próximo'] : ['always', 'eventually', 'next'],
      };

    case 'tg_get_enabled':
      return { marking: semanticMarking(args.marking), enabled: [0], deadlock: false };

    case 'tg_fire_transition':
      return { marking: semanticMarking(args.marking.map((v, i) => i === 1 ? 1 : 0)), enabled: [1], deadlock: false };

    case 'get_incidence_matrix':
      return {
        places: ["P1", "P2", "P3"],
        transitions: ["T1", "T2"],
        matrix: [
          ['-1', '1'],
          ['1', '-1'],
          ['0', '1'],
        ],
      };

    default:
      throw new Error(`[Tauri Mock] Unknown command: ${cmd}`);
  }
};
