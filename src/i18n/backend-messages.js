// The Rust backend emits its user-facing messages in Portuguese. When the UI
// language is not Portuguese, these tables translate them at display time.
// Entries are [portuguese, english]; "{1}", "{2}" mark dynamic parts (translated
// recursively, so nested backend text is handled too). Unknown text passes through.

const ENTRIES = [
  // ── commands (src-tauri) ──
  ['O limite de estados deve estar entre 1 e 10.000.000.', 'The state limit must be between 1 and 10,000,000.'],
  ['O limite de arestas deve estar entre 1 e 50.000.000.', 'The edge limit must be between 1 and 50,000,000.'],
  ['O orçamento de memória deve estar entre 64 e 8.192 MiB.', 'The memory budget must be between 64 and 8,192 MiB.'],
  ['Falha na tarefa de verificação de marcação: {1}', 'Marking check task failed: {1}'],
  ['Falha na tarefa de análise: {1}', 'Analysis task failed: {1}'],
  ['Falha na tarefa de propriedades iniciais: {1}', 'Initial properties task failed: {1}'],
  ['Falha na tarefa de análise do ciclo: {1}', 'Cycle analysis task failed: {1}'],
  ['Falha na tarefa de alcançabilidade: {1}', 'Reachability task failed: {1}'],
  ['Falha na tarefa de cobertura: {1}', 'Coverability task failed: {1}'],
  ['Falha na tarefa de propriedades estruturais: {1}', 'Structural properties task failed: {1}'],
  ['Falha na tarefa de Bad States: {1}', 'Bad states task failed: {1}'],
  ['Falha na tarefa de invariantes: {1}', 'Invariants task failed: {1}'],
  ['Falha na tarefa de evidências: {1}', 'Evidence task failed: {1}'],
  ['Análise estrutural concluída. As propriedades que dependem do espaço de estados não foram calculadas nesta etapa.', 'Structural analysis complete. Properties that depend on the state space were not computed at this stage.'],
  ['a rede é ilimitada; o grafo de alcançabilidade é infinito', 'the net is unbounded; the reachability graph is infinite'],
  ['O espaço de estados não é exato; L e L&S continuam sem resultado.', 'The state space is not exact; L and L&S remain without a result.'],
  ['Não foi possível concluir o espaço de estados para L e L&S: {1}', 'Could not complete the state space for L and L&S: {1}'],
  ['A rede é ilimitada: o grafo de alcançabilidade é infinito, então L não é calculado.', 'The net is unbounded: its reachability graph is infinite, so L (liveness) is not computed.'],
  ['Não foi possível concluir CSV com os P-invariantes.', 'Could not conclude CSV from the P-invariants.'],
  ['Não foi possível concluir SB.', 'Could not conclude SB.'],
  ['Rede com arcos inibidores/transições imediatas: B só é decidido por certificado estrutural ou por exploração exata completa (UNKNOWN).', 'Net with inhibitor arcs/immediate transitions: B is only decided by a structural certificate or a complete exact exploration (UNKNOWN).'],
  ['Não foi possível concluir B pelo certificado estrutural nem pela árvore de cobertura.', 'Could not conclude B from the structural certificate or the coverage tree.'],
  ['Geração solicitada pelo usuário', 'Generation requested by the user'],
  ['Geração do grafo reduzido interrompida: {1}', 'Reduced graph generation interrupted: {1}'],
  ['Geração do grafo de alcançabilidade interrompida: {1}', 'Reachability graph generation interrupted: {1}'],
  ['Geração da árvore de cobertura interrompida: {1}', 'Coverage tree generation interrupted: {1}'],
  ['UNSUPPORTED: a árvore de cobertura (Karp–Miller/Finkel) não é válida para redes com arcos inibidores ou transições imediatas, pois a monotonicidade falha. Use o grafo de alcançabilidade exato (limitado por recursos).', 'UNSUPPORTED: the coverage tree (Karp–Miller/Finkel) is not valid for nets with inhibitor arcs or immediate transitions, because monotonicity fails. Use the exact reachability graph (resource-limited).'],
  ['O MinCov produz o conjunto mínimo de cobertura, não uma árvore; use o comando do conjunto de cobertura.', 'MinCov produces the minimal coverability set, not a tree; use the coverability set command.'],
  ['Nenhuma marcação alcançável satisfaz o Bad State.', 'No reachable marking satisfies the bad state.'],
  ['Lugar desconhecido: {1}', 'Unknown place: {1}'],
  ['UNKNOWN: o MinCov não concluiu ({1}); nenhuma conclusão sobre limitação é válida.', 'UNKNOWN: MinCov did not finish ({1}); no conclusion about boundedness is valid.'],
  ['O script Mercury contém semântica GSPN temporal; a conversão para PNT perderia essa informação.', 'The Mercury script has temporal GSPN semantics; converting it to PNT would lose that information.'],
  ['A fórmula do Bad State é inconsistente com as equações dos P-invariantes.', 'The bad-state formula is inconsistent with the P-invariant equations.'],

  ['{1} permanece INCONCLUSIVO', '{1} remains INCONCLUSIVE'],

  // ── algorithm catalogue ──
  ['página pública; PDF aberto não confirmado', 'public page; open PDF not confirmed'],
  ['página acadêmica pública', 'public academic page'],
  ['preprint público; texto não obtido nesta implementação', 'public preprint; text not obtained in this implementation'],
  ['resumo e slides públicos', 'public abstract and slides'],
  ['registro público; PDF aberto não confirmado', 'public record; open PDF not confirmed'],
  ['abstract público', 'public abstract'],
  ['registro público com PDF indicado', 'public record with PDF listed'],
  ['resultados públicos (fonte empírica, não normativa)', 'public results (empirical, non-normative source)'],
  ['Alcançabilidade em largura (BFS)', 'Breadth-first reachability (BFS)'],
  ['Enumera todas as marcações alcançáveis por níveis: cada marcação é expandida disparando toda transição habilitada. Cada aresta guarda a transição disparada.', 'Enumerates every reachable marking level by level: each marking is expanded by firing every enabled transition. Each edge stores the fired transition.'],
  ['Redes limitadas de tamanho tratável; quando se precisa do grafo exato (deadlock, vivacidade, reversibilidade, CTL) ou de caminhos mínimos em número de disparos.', 'Bounded nets of tractable size; when the exact graph is needed (deadlock, liveness, reversibility, CTL) or shortest paths in number of firings.'],
  ["Redes ilimitadas ou com espaço de estados enorme: a exploração não termina; os limites de recurso produzem resultado inconclusivo, nunca 'falso'.", "Unbounded nets or huge state spaces: exploration does not terminate; resource limits yield an inconclusive result, never 'false'."],
  ['grafo de alcançabilidade exato', 'exact reachability graph'],
  ['Algoritmo clássico de grafos, sem artigo específico de Petri (spec §3). Suporta inibidores e imediatas (racon-extended-pt-v1).', 'Classic graph algorithm, with no Petri-specific paper (spec §3). Supports inhibitors and immediate transitions (racon-extended-pt-v1).'],
  ['Alcançabilidade em profundidade (DFS)', 'Depth-first reachability (DFS)'],
  ['Mesmo grafo que o BFS, mas descobrindo marcações por busca em profundidade com pilha explícita; a numeração dos nós segue a ordem de descoberta.', 'Same graph as BFS, but markings are discovered by depth-first search with an explicit stack; node numbering follows discovery order.'],
  ['Quando apenas o grafo completo importa e o consumo de fronteira do BFS é um problema.', "When only the complete graph matters and BFS's frontier memory is a problem."],
  ['Quando caminhos mínimos importam: a profundidade registrada é a da árvore DFS, não a distância mínima.', 'When shortest paths matter: the recorded depth is that of the DFS tree, not the minimum distance.'],
  ['Verificado por igualdade de marcações e arestas rotuladas contra o BFS em redes aleatórias, inclusive com inibidores e imediatas.', 'Checked for equal markings and labelled edges against BFS on random nets, including inhibitors and immediate transitions.'],
  ['Árvore de cobertura de Karp–Miller', 'Karp–Miller coverage tree'],
  ['Constrói a árvore de marcações em N∪{ω}: ao encontrar uma marcação que cobre estritamente uma marcação ancestral, os lugares que cresceram recebem ω (aceleração).', 'Builds the tree of markings over N∪{ω}: when a marking strictly covers an ancestor marking, the places that grew receive ω (acceleration).'],
  ['Linha de base de cobertura e limitação, inclusive para redes ilimitadas.', 'Baseline for coverability and boundedness, including unbounded nets.'],
  ['Não use para deadlock, vivacidade, reversibilidade ou CTL: com ω o grafo não é a alcançabilidade exata. Inválido com inibidores/imediatas (a monotonicidade falha).', 'Do not use for deadlock, liveness, reversibility or CTL: with ω the graph is not exact reachability. Invalid with inhibitors/immediate transitions (monotonicity fails).'],
  ['árvore de cobertura com ω', 'coverage tree with ω'],
  ['Também é o oráculo usado nos testes do MinCov.', 'It is also the oracle used in the MinCov tests.'],
  ['Grafo mínimo de cobertura de Finkel (legado)', "Finkel's minimal coverability graph (legacy)"],
  ['Aceleração de Karp–Miller seguida de poda por dominância global de subárvores (formulação interna histórica).', 'Karp–Miller acceleration followed by pruning by global subtree dominance (historical internal formulation).'],
  ['Apenas para comparação e regressão com o comportamento antigo.', 'For comparison and regression against the old behaviour only.'],
  ['Não use como resultado de referência: a poda histórica do MCT é incompleta (R4, R6) e esta implementação não possui certificado de completude.', 'Do not use as a reference result: the historical MCT pruning is incomplete (R4, R6) and this implementation has no completeness certificate.'],
  ['grafo de cobertura podado (sem garantia de completude)', 'pruned coverability graph (no completeness guarantee)'],
  ['Spec §8.2: manter só como legacy_experimental e remover do caminho padrão. Prefira MinCov.', 'Spec §8.2: keep only as legacy_experimental and remove from the default path. Prefer MinCov.'],
  ['Constrói uma árvore de cobertura mínima memorizando as ACELERAÇÕES (ω-transições) em vez de marcações ω: dispara aceleração já conhecidas, descarta vértices cobertos (limpeza), cria nova aceleração ao achar ancestral estritamente menor e poda a subárvore. Termina com Clover(N,m0), o conjunto mínimo de cobertura.', 'Builds a minimal coverability tree by remembering ACCELERATIONS (ω-transitions) instead of ω-markings: fires already known accelerations, discards covered vertices (cleanup), creates a new acceleration on finding a strictly smaller ancestor and prunes the subtree. Ends with Clover(N,m0), the minimal coverability set.'],
  ['Cobertura e limitação com menor memória que Karp–Miller e, ao contrário da poda histórica de Finkel, com prova de completude (R6).', "Coverability and boundedness using less memory than Karp–Miller and, unlike Finkel's historical pruning, with a completeness proof (R6)."],
  ['Não é um grafo de transições: não use para alcançabilidade exata, deadlock, vivacidade, reversibilidade, caminhos ou CTL.', 'It is not a transition graph: do not use it for exact reachability, deadlock, liveness, reversibility, paths or CTL.'],
  ['conjunto mínimo de cobertura Clover (antichain de ω-marcações)', 'Clover minimal coverability set (antichain of ω-markings)'],
  ['Experimental até a revisão técnica exigida pela spec §8.1.10. Validado contra Karp–Miller e contra exploração exaustiva em 600 redes aleatórias.', 'Experimental until the technical review required by spec §8.1.10. Validated against Karp–Miller and exhaustive exploration on 600 random nets.'],
  ['Karp–Miller com poda entre ramos segundo as condições demonstradas em R4.', 'Karp–Miller with pruning across branches under the conditions proved in R4.'],
  ['Indisponível: o texto do artigo não foi obtido para transcrever as condições exatas de poda.', 'Unavailable: the paper text was not obtained to transcribe the exact pruning conditions.'],
  ['conjunto mínimo de cobertura', 'minimal coverability set'],
  ['Spec §9: implementar as condições exatas, não uma heurística global.', 'Spec §9: implement the exact conditions, not a global heuristic.'],
  ['Cálculo eficiente do conjunto mínimo de cobertura (backend alternativo).', 'Efficient computation of the minimal coverability set (alternative backend).'],
  ['Indisponível: não implementado.', 'Unavailable: not implemented.'],
  ['Algoritmo sem poda de futuros e comparação de estratégias de exploração.', 'Algorithm without future pruning and comparison of exploration strategies.'],
  ['Em cada marcação explora apenas as transições habilitadas de um conjunto teimoso, fechado sob conflito e sob habilitação das transições desabilitadas, evitando intercalações independentes.', 'At each marking, explores only the enabled transitions of a stubborn set, closed under conflict and under enabling of disabled transitions, avoiding independent interleavings.'],
  ['Detectar deadlocks em redes com muita concorrência independente.', 'Detecting deadlocks in nets with a lot of independent concurrency.'],
  ["A variante implementada NÃO tem proviso de 'ignoring': não leia vivacidade, transições mortas, reversibilidade, DCF, limitação ou CTL do grafo reduzido. Bloqueado com algoritmos de cobertura.", "The implemented variant has NO 'ignoring' proviso: do not read liveness, dead transitions, reversibility, DCF, boundedness or CTL from the reduced graph. Blocked with coverability algorithms."],
  ['grafo de alcançabilidade reduzido', 'reduced reachability graph'],
  ['Variante interna baseada em Valmari (1990); a conformidade com a variante orientada à propriedade de R9 ainda não foi auditada.', "Internal variant based on Valmari (1990); conformance with R9's property-oriented variant has not been audited yet."],
  ['Reduções estruturais (Thierry-Mieg)', 'Structural reductions (Thierry-Mieg)'],
  ['Simplifica lugares e transições antes da exploração por regras com condições formais de preservação.', 'Simplifies places and transitions before exploration using rules with formal preservation conditions.'],
  ['Indisponível: as regras de R8 ainda não foram mapeadas uma a uma; o redutor estrutural interno não é certificado.', 'Unavailable: the R8 rules have not been mapped one by one yet; the internal structural reducer is not certified.'],
  ['rede reduzida + correspondência', 'reduced net + correspondence'],
  ['Abstração poliédrica + SMT', 'Polyhedral abstraction + SMT'],
  ['Relaciona a rede a uma versão reduzida por restrições lineares para verificação via SMT.', 'Relates the net to a reduced version through linear constraints for verification via SMT.'],
  ['Fase posterior da spec; não implementado.', 'Later phase of the spec; not implemented.'],
  ['Prefixo de unfolding (ERV)', 'Unfolding prefix (ERV)'],
  ['Prefixo completo de ordem parcial para classes finitas apropriadas.', 'Complete partial-order prefix for suitable finite classes.'],
  ['Não é método genérico de cobertura ilimitada; não implementado.', 'Not a generic method for unbounded coverability; not implemented.'],
  ['Redução por simetria', 'Symmetry reduction'],
  ['Quociente por classes de marcações equivalentes sob automorfismos.', 'Quotient by classes of markings that are equivalent under automorphisms.'],
  ['Em estudo: nenhum artigo específico para P/T e propriedades suportadas foi validado (spec §3, §11).', 'Under study: no specific paper for P/T nets and the supported properties has been validated (spec §3, §11).'],
  ['Existe uma implementação interna acessível apenas pela API legada; ela não é oferecida aqui.', 'An internal implementation exists, reachable only through the legacy API; it is not offered here.'],
  ['Diagramas de decisão', 'Decision diagrams'],
  ['Representação simbólica do espaço de estados.', 'Symbolic representation of the state space.'],
  ['Em estudo: algoritmo e representação concretos ainda não selecionados.', 'Under study: concrete algorithm and representation not selected yet.'],
  ['R13 é só mapa de ferramentas.', 'R13 is only a tool map.'],

  // ── configuration validation ──
  ["Modo '{1}' desconhecido; use none, manual ou automatic.", "Unknown mode '{1}'; use none, manual or automatic."],
  ['BFS é a única exploração exata estável para alcançabilidade com caminhos mínimos.', 'BFS is the only stable exact exploration for reachability with shortest paths.'],
  ["Karp–Miller é o único backend de cobertura 'stable'; MinCov permanece experimental.", "Karp–Miller is the only 'stable' coverability backend; MinCov remains experimental."],
  ['O modo automático só aplica reduções certificadas; nenhuma está disponível.', 'Automatic mode only applies certified reductions; none is available.'],
  ['Nenhum método certificado para cobertura/limitação em redes com inibidores ou transições imediatas (a monotonicidade falha). Use alcançabilidade exata.', 'No certified method for coverability/boundedness on nets with inhibitors or immediate transitions (monotonicity fails). Use exact reachability.'],
  ['Escolha um algoritmo de exploração.', 'Choose an exploration algorithm.'],
  ["Algoritmo '{1}' não existe no catálogo.", "Algorithm '{1}' does not exist in the catalogue."],
  ["O modo 'none' não aplica nenhuma redução; remova as reduções ou use o modo manual.", "The 'none' mode applies no reduction; remove the reductions or use manual mode."],
  ["A redução '{1}' foi selecionada mais de uma vez.", "Reduction '{1}' was selected more than once."],
  ["Redução '{1}' não existe no catálogo.", "Reduction '{1}' does not exist in the catalogue."],
  ['Stubborn sets + algoritmos de cobertura: bloqueada — não há teorema que garanta a preservação de Clover por redução orientada a deadlock/alcançabilidade (spec §4.2).', 'Stubborn sets + coverability algorithms: blocked — no theorem guarantees preservation of Clover by a deadlock/reachability-oriented reduction (spec §4.2).'],
  ['Symmetry + stubborn + estrutural: em pesquisa; exige provas de preservação específicas (spec §4.2).', 'Symmetry + stubborn + structural: under research; requires specific preservation proofs (spec §4.2).'],
  ['algoritmo de exploração', 'exploration algorithm'],
  ['redução aplicada durante a exploração', 'reduction applied during exploration'],
  ['Há algoritmos experimentais na configuração: o resultado será marcado como experimental, sem declaração de prova definitiva.', 'The configuration includes experimental algorithms: the result will be marked experimental, with no claim of definitive proof.'],
  ["Rede estendida (racon-extended-pt-v1): somente exploração exata; resultados truncados são 'unknown'.", "Extended net (racon-extended-pt-v1): exact exploration only; truncated results are 'unknown'."],
  ['publicação identificada, mas o algoritmo ainda não foi implementado', 'publication identified, but the algorithm is not implemented yet'],
  ['em estudo: sem artigo específico validado', 'under study: no specific paper validated'],
  ['indisponível', 'unavailable'],
  ["'{1}' não pode ser selecionado: {2}.", "'{1}' cannot be selected: {2}."],
  ["'{1}' é uma {2} e não pode ser usada como {3}.", "'{1}' is a {2} and cannot be used as {3}."],
  ['redução', 'reduction'],
  ['exploração', 'exploration'],
  ["'{1}' não atende ao objetivo selecionado; não confunda algoritmo de cobertura com alcançabilidade exata.", "'{1}' does not meet the selected objective; do not confuse a coverability algorithm with exact reachability."],
  ["'{1}' não é válido para a classe de rede selecionada. Inibidores e precedência de imediatas quebram a monotonicidade exigida.", "'{1}' is not valid for the selected net class. Inhibitors and immediate-transition precedence break the required monotonicity."],
  ["'{1}' não é válido para a classe de rede selecionada.", "'{1}' is not valid for the selected net class."],

  // ── properties preserved / not preserved ──
  ['alcançabilidade', 'reachability'], ['alcançabilidade exata', 'exact reachability'], ['vivacidade', 'liveness'],
  ['reversibilidade', 'reversibility'], ['limitação', 'boundedness'], ['cobertura', 'coverability'],
  ['caminho mínimo em disparos', 'shortest path in firings'], ['caminho mínimo', 'shortest path'], ['caminhos', 'paths'],
  ['lugares ilimitados', 'unbounded places'], ['transições mortas', 'dead transitions'],
  ['cobertura (completude não certificada)', 'coverability (completeness not certified)'],
  ['deadlock (alcançabilidade de estado morto)', 'deadlock (reachability of a dead state)'],
  ['para todos', 'for all'], ['próximo', 'next'],

  // ── evidence ──
  ['{1} estados; limites por lugar: {2}', '{1} states; per-place bounds: {2}'],
  ['nó {1} contém ω; caminho com {2} disparos', 'node {1} contains ω; path with {2} firings'],
  ['nó {1}; habilitadas: {2}', 'node {1}; enabled: {2}'],
  ['t{1} habilitada em {2} de {3} estados', 't{1} enabled in {2} of {3} states'],
  ['{1} estados; nó inicial {2}', '{1} states; initial node {2}'],
  ['Cada lugar possui um limite finito em todas as marcações alcançáveis.', 'Every place has a finite bound over all reachable markings.'],
  ['Existe uma marcação alcançável sem transições habilitadas.', 'There is a reachable marking with no enabled transitions.'],
  ['Existe uma transição que nunca é habilitada.', 'There is a transition that is never enabled.'],
  ['Toda marcação alcançável retorna à marcação inicial.', 'Every reachable marking can return to the initial marking.'],
  ['Toda transição pode voltar a ser habilitada a partir de qualquer estado.', 'Every transition can become enabled again from any state.'],
  ['A rede é viva e segura simultaneamente.', 'The net is live and safe at the same time.'],
  ['Nenhuma marcação alcançável possui conflito dinâmico.', 'No reachable marking has a dynamic conflict.'],
  ['Todos os arcos têm peso unitário.', 'All arcs have unit weight.'],
  ['Arcos de saída de cada lugar têm o mesmo peso.', 'Output arcs of each place have the same weight.'],
  ['O grafo estrutural subjacente é conectado.', 'The underlying structural graph is connected.'],
  ['Todo nó estrutural alcança todo outro nó.', 'Every structural node reaches every other node.'],
  ['Nenhum lugar é simultaneamente pré e pós-lugar da mesma transição.', 'No place is both an input and an output place of the same transition.'],
  ['Nenhum lugar possui mais de uma transição de saída.', 'No place has more than one output transition.'],
  ['Cada lugar tem exatamente uma entrada e uma saída.', 'Each place has exactly one input and one output.'],
  ['Escolhas que compartilham lugar têm pré-conjunto unitário.', 'Choices that share a place have a single-element preset.'],
  ['Transições com pré-conjunto compartilhado têm pré-conjuntos iguais.', 'Transitions with a shared preset have equal presets.'],
  ['Os vetores exibidos são P-semifluxos verificados pela matriz de incidência.', 'The displayed vectors are P-semiflows verified against the incidence matrix.'],
  ['Os vetores exibidos são T-semifluxos verificados pela matriz de incidência.', 'The displayed vectors are T-semiflows verified against the incidence matrix.'],
  ['Todos os lugares são cobertos por P-semifluxos positivos.', 'All places are covered by positive P-semiflows.'],
  ['Todas as transições são cobertas por T-semifluxos positivos.', 'All transitions are covered by positive T-semiflows.'],
  ['A limitação estrutural é certificada por CPI/CSV ou pelo certificado de Murata.', "Structural boundedness is certified by CPI/CSV or by Murata's certificate."],
  ['Método: {1}', 'Method: {1}'],
  ['A conclusão é restrita ao espaço de estados informado pelo método.', 'The conclusion is restricted to the state space reported by the method.'],
  ['O certificado satisfaz a definição da propriedade.', 'The certificate satisfies the property definition.'],
  ['O contraexemplo viola a definição da propriedade.', 'The counterexample violates the property definition.'],
  ['Os dados disponíveis não permitem concluir a propriedade.', 'The available data does not allow concluding the property.'],
  ['A completude deve ser lida junto da proveniência do espaço analisado.', 'Completeness must be read together with the provenance of the analysed space.'],
  ['{1} arcos; {2} arcos com peso diferente de 1', '{1} arcs; {2} arcs with weight other than 1'],
  ['{1} transições verificadas', '{1} transitions checked'],
  ['{1} possui {2} saídas', '{1} has {2} outputs'],
  ['grau de saída de todos os lugares verificado', 'out-degree of all places checked'],
  ['todos os elementos satisfazem o predicado', 'all elements satisfy the predicate'],
  ['foi encontrado elemento que viola o predicado', 'an element violating the predicate was found'],
  ['A propriedade é estrutural e foi verificada diretamente na rede, sem usar o grafo de cobertura.', 'The property is structural and was checked directly on the net, without using the coverage graph.'],
  ['Verificação estrutural', 'Structural check'],
  ['Todos os elementos relevantes satisfazem o predicado estrutural.', 'All relevant elements satisfy the structural predicate.'],
  ['Foi encontrado um elemento que viola o predicado estrutural.', 'An element violating the structural predicate was found.'],
  ['Todos os elementos da rede relevantes para esta definição foram verificados.', 'All net elements relevant to this definition were checked.'],
  ['O grafo de alcançabilidade é completo e exato.', 'The reachability graph is complete and exact.'],
  ['Espaço analisado', 'Analysed space'],
  ['{1} estados', '{1} states'],
  ['Limites observados por lugar', 'Observed bounds per place'],
  ['Em cada lugar foi tomado o maior número de tokens observado no grafo completo.', 'For each place, the largest number of tokens observed in the complete graph was taken.'],
  ['Nenhuma marcação enumerada excede o vetor de limites.', 'No enumerated marking exceeds the bound vector.'],
  ['Os {1} estados alcançáveis foram enumerados.', 'All {1} reachable states were enumerated.'],
  ['Máximos observados por lugar', 'Observed maxima per place'],
  ['Lugar', 'Place'], ['Máximo observado', 'Observed maximum'], ['Limite kₚ', 'Bound kₚ'],
  ['Espaço de estados não exato ou incompleto', 'State space not exact or incomplete'],
  ['Reachability Graph completo necessário', 'Complete reachability graph required'],
  ['Foi encontrada uma marcação alcançável sem transições habilitadas.', 'A reachable marking with no enabled transitions was found.'],
  ['Cada marcação enumerada possui pelo menos uma transição habilitada.', 'Every enumerated marking has at least one enabled transition.'],
  ['{1} marcações foram enumeradas e verificadas.', '{1} markings were enumerated and checked.'],
  ['Marcações e transições habilitadas', 'Markings and enabled transitions'],
  ['Marcação', 'Marking'], ['Transições habilitadas', 'Enabled transitions'],
  ['RG completo: cada transição possui marcação habilitadora; {1} estados', 'Complete RG: every transition has an enabling marking; {1} states'],
  ['Transições verificadas', 'Transitions checked'],
  ['Existe uma transição sem qualquer marcação habilitadora.', 'There is a transition with no enabling marking.'],
  ['Cada transição possui ao menos uma marcação alcançável que a habilita.', 'Every transition has at least one reachable marking that enables it.'],
  ['Todas as {1} transições foram verificadas contra as {2} marcações.', 'All {1} transitions were checked against the {2} markings.'],
  ['Witness por transição', 'Witness per transition'],
  ['Transição', 'Transition'], ['Marcações habilitadoras', 'Enabling markings'],
  ['Estados alcançáveis', 'Reachable states'], ['Estados que alcançam M₀', 'States that reach M₀'],
  ['Estados sem caminho de retorno', 'States without a return path'],
  ['Todos os estados alcançáveis possuem caminho de retorno para M₀.', 'Every reachable state has a return path to M₀.'],
  ['O estado M{1} é alcançável, mas não possui caminho de retorno para M₀.', 'State M{1} is reachable but has no return path to M₀.'],
  ['Caminhos de retorno para M₀', 'Return paths to M₀'],
  ['Estado', 'State'], ['Sequência de disparos', 'Firing sequence'],
  ['Para cada transição, o fecho de predecessores dos estados habilitadores cobre todos os estados alcançáveis.', 'For every transition, the predecessor closure of its enabling states covers all reachable states.'],
  ['Foi encontrada uma transição cujo fecho de predecessores não cobre todos os estados alcançáveis.', 'A transition whose predecessor closure does not cover all reachable states was found.'],
  ['{1} transições e {2} estados foram verificados.', '{1} transitions and {2} states were checked.'],
  ['Fechos de predecessores por transição', 'Predecessor closures per transition'],
  ['A matriz de incidência C foi calculada com aritmética inteira exata.', 'The incidence matrix C was computed with exact integer arithmetic.'],
  ['A multiplicação do vetor pelo lado esquerdo da matriz de incidência produz o vetor nulo.', 'Multiplying the vector on the left of the incidence matrix yields the zero vector.'],
  ['yᵀC = 0; o vetor é um P-semifluxo verificado', 'yᵀC = 0; the vector is a verified P-semiflow'],
  ['O produto foi verificado em todas as colunas da matriz de incidência.', 'The product was checked on every column of the incidence matrix.'],
  ['SIM; sequência: {1}', 'YES; sequence: {1}'],
  ['não informada', 'not provided'],
  ['A multiplicação da matriz pelo vetor de disparos produz o vetor nulo.', 'Multiplying the matrix by the firing vector yields the zero vector.'],
  ['Cx = 0; o vetor é um T-semifluxo verificado', 'Cx = 0; the vector is a verified T-semiflow'],
  ['O produto foi verificado em todas as linhas da matriz de incidência; a realizabilidade é reportada separadamente.', 'The product was checked on every row of the incidence matrix; realizability is reported separately.'],
];

const escapeRe = (s) => s.replace(/[.*+?^$()|[\]\\]/g, '\\$&');

const exact = new Map();
const patterns = [];
for (const [pt, en] of ENTRIES) {
  if (!/\{\d\}/.test(pt)) { exact.set(pt, en); continue; }
  const source = pt.split(/(\{\d\})/).map((part) => (/^\{\d\}$/.test(part) ? '([\\s\\S]+?)' : escapeRe(part))).join('');
  const order = [...pt.matchAll(/\{(\d)\}/g)].map((m) => Number(m[1]));
  patterns.push({ re: new RegExp(`^${source}$`), order, en });
}
// Longer (more specific) patterns first.
patterns.sort((a, b) => b.re.source.length - a.re.source.length);

export function translateBackend(text, language, depth = 0) {
  if (typeof text !== 'string' || !text || String(language).toLowerCase().startsWith('pt') || depth > 4) return text;
  const trimmed = text.trim();
  if (exact.has(trimmed)) return exact.get(trimmed);
  for (const { re, order, en } of patterns) {
    const match = re.exec(trimmed);
    if (!match) continue;
    const args = {};
    order.forEach((n, i) => { args[n] = translateBackend(match[i + 1], language, depth + 1); });
    return en.replace(/\{(\d)\}/g, (_, n) => args[n] ?? '');
  }
  // "name — output" pairs (e.g. the pipeline steps) are translated part by part.
  if (trimmed.includes(' — ')) {
    const pieces = trimmed.split(' — ');
    const translated = pieces.map((piece) => translateBackend(piece, language, depth + 1));
    if (translated.some((piece, i) => piece !== pieces[i])) return translated.join(' — ');
  }
  // Several messages may be joined with a space (e.g. warnings): translate sentence by sentence.
  const parts = trimmed.split(/(?<=\.)\s+(?=[A-ZÁÉÍÓÚ])/);
  if (parts.length > 1) {
    const translated = parts.map((part) => translateBackend(part, language, depth + 1));
    if (translated.some((part, i) => part !== parts[i])) return translated.join(' ');
  }
  return text;
}
