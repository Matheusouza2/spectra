import { invokeLogged as invoke } from './app-logger';

export const analyzePnt = (filePath) => invoke('analyze_pnt', { filePath: filePath });
export const getRequiredInitialProperties = (filePath) => invoke('get_required_initial_properties', { filePath });
export const getDeadlockTrapProperty = (filePath) => invoke('get_deadlock_trap_property', { filePath });
export const getAnalysisEvidence = (filePath) => invoke('get_analysis_evidence', { filePath });
export const getReachabilityGraph = (filePath, config) => invoke('get_reachability_graph', { filePath, config });
export const getReachabilityGraphPage = (filePath, offset = 0, limit = 500, config) =>
  invoke('get_reachability_graph_page', { filePath, offset, limit, config });
export const getCoverageTree = (filePath, config) => invoke('get_coverage_tree', { filePath, config });
export const getStructuralProperties = (filePath) => invoke('get_structural_properties', { filePath: filePath });
export const checkBadStates = (filePath, definitions, config) => invoke('check_bad_states', { filePath, definitions, config });
export const getInvariants = (filePath) => invoke('get_invariants', { filePath: filePath });
export const checkCtlFormula = (filePath, formula, config) => invoke('check_ctl_formula', { filePath, formula, config });
export const validateCtlFormula = (filePath, formula, language) => invoke('validate_ctl_formula', { filePath, formula, language });
export const getNetInfo = (filePath) => invoke('get_net_info', { filePath: filePath });
export const getNetStructure = (filePath) => invoke('get_net_structure', { filePath: filePath });
export const tgGetEnabled = (filePath, marking) => invoke('tg_get_enabled', { filePath: filePath, marking });
export const tgFireTransition = (filePath, marking, transition) =>
  invoke('tg_fire_transition', { filePath: filePath, marking, transition });
export const getIncidenceMatrix = (filePath) => invoke('get_incidence_matrix', { filePath: filePath });
export const computeBehavioralProperties = (filePath, config) =>
  invoke('compute_behavioral_properties', { filePath, config });
export const testMarkingAlgebraic = (filePath, marking) =>
  invoke('test_marking_algebraic', { filePath, marking });
export const checkMarkingCoverability = (filePath, marking, config) =>
  invoke('check_marking_coverability', { filePath, marking, config });
export const computeDistances = (filePath, from, to, config) =>
  invoke('compute_distances', { filePath, from, to, config });
export const computeCircuits = (filePath, request, config) =>
  invoke('compute_circuits', { filePath, request, config });
export const buildPredicate = (filePath, kind, transitions, states, config) =>
  invoke('build_predicate', { filePath, kind, transitions, states, config });
export const testVector = (filePath, kind, vector) =>
  invoke('test_vector', { filePath, kind, vector });
export const checkMarkingReachability = (filePath, marking, config) =>
  invoke('check_marking_reachability', { filePath, marking, config });
export const parseMercuryText = (script) =>
  invoke('parse_mercury_text', { script });
export const checkCycleFairness = (filePath, nodes, transitions, config) =>
  invoke('check_cycle_fairness', { filePath, nodes, transitions, config });
export const getAlgorithmCatalog = () => invoke('get_algorithm_catalog');
export const validateAnalysisConfig = (config) => invoke('validate_analysis_config', { config });
export const getMinimalCoverabilitySet = (filePath, config) =>
  invoke('get_minimal_coverability_set', { filePath, config });
export const checkPlaceBoundedness = (filePath, place, config) =>
  invoke('check_place_boundedness', { filePath, place, config });
export const checkTransitionDead = (filePath, transition, config) =>
  invoke('check_transition_dead', { filePath, transition, config });
