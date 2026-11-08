//! Versioned catalogue of analysis/reduction algorithms and the compatibility
//! matrix that validates a user configuration **before** anything runs.
//!
//! Source of truth: *RACoN — Especificação de arquitetura para algoritmos
//! configuráveis de análise e redução* (2026-11-04), §3 (catalogue), §4.2
//! (combinations) and §5.2 (front-end behaviour). The catalogue only declares
//! what the project has actually implemented and tested; everything else is
//! listed as `proposed`/`under_study` and can never be selected.

use serde::{Deserialize, Serialize};

pub const CATALOG_VERSION: &str = "racon-algorithm-catalog-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmStatus {
    /// Implemented, covered by conformance tests against independent oracles.
    Stable,
    /// Implemented from the cited paper, awaiting the technical review the
    /// specification requires before `stable`. Results are marked experimental.
    Experimental,
    /// Historical internal implementation kept only for comparison (§8.2).
    LegacyExperimental,
    /// Publication identified, not implemented.
    Proposed,
    /// No specific publication validated yet.
    UnderStudy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Exploration,
    Reduction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    Reachability,
    Coverability,
    Boundedness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetClass {
    /// Ordinary and weighted P/T nets.
    PtWeighted,
    /// P/T nets with inhibitor arcs and/or immediate transitions.
    ExtendedPt,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    pub id: &'static str,
    pub citation: &'static str,
    pub year: u16,
    pub doi: Option<&'static str>,
    pub url: &'static str,
    /// What was verified about public access (spec §10).
    pub access: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlgorithmEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub role: Role,
    pub status: AlgorithmStatus,
    pub selectable: bool,
    pub objectives: &'static [Objective],
    pub supported_net_classes: &'static [NetClass],
    /// Short description of how the method works.
    pub description: &'static str,
    pub when_to_use: &'static str,
    pub when_not_to_use: &'static str,
    /// Kind of artefact produced (never interchangeable between kinds).
    pub output: &'static str,
    /// Properties that may be concluded from the produced artefact.
    pub preserves: &'static [&'static str],
    /// Properties that must NOT be concluded from the produced artefact.
    pub does_not_preserve: &'static [&'static str],
    pub reference_ids: &'static [&'static str],
    pub implementation_version: Option<&'static str>,
    /// Where the function ↔ paper step ↔ test mapping lives.
    pub proof_mapping: Option<&'static str>,
    pub notes: &'static str,
}

const PT: &[NetClass] = &[NetClass::PtWeighted];
const PT_EXT: &[NetClass] = &[NetClass::PtWeighted, NetClass::ExtendedPt];

pub const REFERENCES: &[Reference] = &[
    Reference { id: "R1", citation: "KARP, R. M.; MILLER, R. E. Parallel program schemata. J. Comput. Syst. Sci., 3(2):147-195", year: 1969, doi: Some("10.1016/S0022-0000(69)80011-5"), url: "https://www.sciencedirect.com/science/article/pii/S0022000069800115", access: "página pública; PDF aberto não confirmado" },
    Reference { id: "R2", citation: "FINKEL, A. The minimal coverability graph for Petri nets. Advances in Petri Nets 1993, LNCS 674, 210-243", year: 1993, doi: Some("10.1007/3-540-56689-9_45"), url: "https://www.informatik.uni-hamburg.de/TGI/pnbib/f/finkel_a2.html", access: "página pública; PDF aberto não confirmado" },
    Reference { id: "R3", citation: "GEERAERTS, G.; RASKIN, J.-F.; VAN BEGIN, L. On the efficient computation of the minimal coverability set for Petri nets. ATVA 2007, LNCS 4762, 98-113", year: 2007, doi: Some("10.1007/978-3-540-75596-8_9"), url: "https://verif.ulb.ac.be/ggeeraer/", access: "texto integral consultado (References/Bibliografia/covsetpn.pdf)" },
    Reference { id: "R4", citation: "REYNIER, P.-A.; SERVAIS, F. Minimal coverability set for Petri nets: Karp and Miller algorithm with pruning. Fundamenta Informaticae 122(1-2):1-30", year: 2013, doi: Some("10.3233/FI-2013-781"), url: "https://journals.sagepub.com/doi/10.3233/FI-2013-781", access: "texto integral consultado (References/Bibliografia/fi13.pdf)" },
    Reference { id: "R6", citation: "FINKEL, A.; HADDAD, S.; KHMELNITSKY, I. Minimal coverability tree construction made complete and efficient. FoSSaCS 2020, LNCS 12077, 237-256", year: 2020, doi: Some("10.1007/978-3-030-45231-5_13"), url: "https://link.springer.com/chapter/10.1007/978-3-030-45231-5_13", access: "texto integral aberto (consultado)" },
    Reference { id: "R7", citation: "HILAIRE, T.; ILCINKAS, D.; LEROUX, J. A state-of-the-art Karp-Miller algorithm certified in Coq. TACAS 2024, LNCS 14570, 370-389", year: 2024, doi: Some("10.1007/978-3-031-57246-3_21"), url: "https://link.springer.com/chapter/10.1007/978-3-031-57246-3_21", access: "texto integral aberto" },
    Reference { id: "R8", citation: "THIERRY-MIEG, Y. Structural reductions revisited. Petri Nets 2020, LNCS 12152, 303-323", year: 2020, doi: Some("10.1007/978-3-030-51831-8_15"), url: "https://pmc.ncbi.nlm.nih.gov/articles/PMC7324236/", access: "texto integral consultado (References/Bibliografia/978-3-030-51831-8_Chapter_15.pdf)" },
    Reference { id: "R9", citation: "SCHMIDT, K. Stubborn sets for standard properties. Petri Nets 1999, LNCS 1630, 46-65", year: 1999, doi: Some("10.1007/3-540-48745-X_4"), url: "https://www.informatik.uni-hamburg.de/TGI/pnbib/s/schmidt_k8.html", access: "registro público; PDF aberto não confirmado" },
    Reference { id: "R10", citation: "Stubborn versus structural reductions for Petri nets. J. Log. Algebr. Methods Program. 102:46-63", year: 2019, doi: Some("10.1016/j.jlamp.2018.09.002"), url: "https://www.sciencedirect.com/science/article/pii/S235222081830035X", access: "abstract público" },
    Reference { id: "R11", citation: "AMAT, N.; BERTHOMIEU, B.; DAL ZILIO, S. A polyhedral abstraction for Petri nets and its application to SMT-based model checking. Fundamenta Informaticae 187(2-4):103-138", year: 2022, doi: Some("10.3233/FI-222134"), url: "https://arxiv.org/abs/2104.09850", access: "texto integral aberto" },
    Reference { id: "R12", citation: "ESPARZA, J.; RÖMER, S.; VOGLER, W. An improvement of McMillan's unfolding algorithm. TACAS 1996, LNCS 1055, 87-106", year: 1996, doi: Some("10.1007/3-540-61042-1_40"), url: "https://archive.model.in.tum.de/um/bibdb/info/esparza.ERV96.shtml.html", access: "texto integral consultado (References/Bibliografia/ImpUnfolding.pdf, imagem)" },
    Reference { id: "R14", citation: "JUNTTILA, T. On the symmetry reduction method for Petri nets and similar formalisms. Helsinki University of Technology, Laboratory for Theoretical Computer Science, Research Report HUT-TCS-A80 (tese de doutorado)", year: 2003, doi: None, url: "", access: "texto integral local consultado (capítulos 2, 3 e 4)" },
    Reference { id: "R15", citation: "CORMEN, T. H.; LEISERSON, C. E.; RIVEST, R. L.; STEIN, C. Introduction to Algorithms, 4th ed., MIT Press, seções 20.2 (busca em largura) e 20.3 (busca em profundidade)", year: 2022, doi: None, url: "https://mitpress.mit.edu/9780262046305/introduction-to-algorithms/", access: "livro; seções 20.2 e 20.3 consultadas" },
    Reference { id: "R13", citation: "MODEL CHECKING CONTEST. Complete results for the 2026 edition", year: 2026, doi: None, url: "https://mcc.lip6.fr/2026/results.php", access: "resultados públicos (fonte empírica, não normativa)" },
];

pub const ALGORITHMS: &[AlgorithmEntry] = &[
    AlgorithmEntry {
        id: "reachability_bfs", name: "Alcançabilidade em largura (BFS)", family: "reachability",
        role: Role::Exploration, status: AlgorithmStatus::Stable, selectable: true,
        objectives: &[Objective::Reachability], supported_net_classes: PT_EXT,
        description: "Enumera todas as marcações alcançáveis por níveis: cada marcação é expandida disparando toda transição habilitada. Cada aresta guarda a transição disparada.",
        when_to_use: "Redes limitadas de tamanho tratável; quando se precisa do grafo exato (deadlock, vivacidade, reversibilidade, CTL) ou de caminhos mínimos em número de disparos.",
        when_not_to_use: "Redes ilimitadas ou com espaço de estados enorme: a exploração não termina; os limites de recurso produzem resultado inconclusivo, nunca 'falso'.",
        output: "grafo de alcançabilidade exato",
        preserves: &["alcançabilidade", "deadlock", "vivacidade", "reversibilidade", "limitação", "CTL", "caminho mínimo em disparos"],
        does_not_preserve: &[],
        reference_ids: &["R15"], implementation_version: Some("racon-reachability bfs-1"), proof_mapping: Some("crates/racon-reachability/tests"),
        notes: "Segue a busca em largura de Cormen et al. §20.2: fila FIFO, marcação descoberta uma única vez e profundidade igual à distância mínima em disparos (Teorema 20.5); custo O(V+E) sobre o grafo gerado. Suporta inibidores e imediatas (racon-extended-pt-v1).",
    },
    AlgorithmEntry {
        id: "reachability_dfs", name: "Alcançabilidade em profundidade (DFS)", family: "reachability",
        role: Role::Exploration, status: AlgorithmStatus::Stable, selectable: true,
        objectives: &[Objective::Reachability], supported_net_classes: PT_EXT,
        description: "Mesmo grafo que o BFS, mas descobrindo marcações por busca em profundidade com pilha explícita; a numeração dos nós segue a ordem de descoberta.",
        when_to_use: "Quando apenas o grafo completo importa e o consumo de fronteira do BFS é um problema.",
        when_not_to_use: "Quando caminhos mínimos importam: a profundidade registrada é a da árvore DFS, não a distância mínima.",
        output: "grafo de alcançabilidade exato",
        preserves: &["alcançabilidade", "deadlock", "vivacidade", "reversibilidade", "limitação", "CTL"],
        does_not_preserve: &["caminho mínimo em disparos"],
        reference_ids: &["R15"], implementation_version: Some("racon-reachability dfs-1"), proof_mapping: Some("crates/racon-reachability/tests/dfs_tests.rs"),
        notes: "Segue o DFS-VISIT de Cormen et al. §20.3, com pilha explícita no lugar da recursão: um nó é descoberto no primeiro acesso e concluído quando suas transições habilitadas se esgotam; custo O(V+E). Verificado por igualdade de marcações e arestas rotuladas contra o BFS em redes aleatórias, inclusive com inibidores e imediatas.",
    },
    AlgorithmEntry {
        id: "coverability_karp_miller", name: "Árvore de cobertura de Karp–Miller", family: "coverability",
        role: Role::Exploration, status: AlgorithmStatus::Stable, selectable: true,
        objectives: &[Objective::Coverability, Objective::Boundedness], supported_net_classes: PT,
        description: "Constrói a árvore de marcações em N∪{ω}: ao encontrar uma marcação que cobre estritamente uma marcação ancestral, os lugares que cresceram recebem ω (aceleração).",
        when_to_use: "Linha de base de cobertura e limitação, inclusive para redes ilimitadas.",
        when_not_to_use: "Não use para deadlock, vivacidade, reversibilidade ou CTL: com ω o grafo não é a alcançabilidade exata. Inválido com inibidores/imediatas (a monotonicidade falha).",
        output: "árvore de cobertura com ω",
        preserves: &["cobertura", "limitação"],
        does_not_preserve: &["alcançabilidade exata", "deadlock", "vivacidade", "reversibilidade", "CTL"],
        reference_ids: &["R1"], implementation_version: Some("racon-coverability km-1"), proof_mapping: None,
        notes: "Também é o oráculo usado nos testes do MinCov.",
    },
    AlgorithmEntry {
        id: "coverability_finkel_legacy", name: "Grafo mínimo de cobertura de Finkel (legado)", family: "coverability",
        role: Role::Exploration, status: AlgorithmStatus::LegacyExperimental, selectable: true,
        objectives: &[Objective::Coverability, Objective::Boundedness], supported_net_classes: PT,
        description: "Aceleração de Karp–Miller seguida de poda por dominância global de subárvores (formulação interna histórica).",
        when_to_use: "Apenas para comparação e regressão com o comportamento antigo.",
        when_not_to_use: "Não use como resultado de referência: a poda histórica do MCT é incompleta (R4, R6) e esta implementação não possui certificado de completude.",
        output: "grafo de cobertura podado (sem garantia de completude)",
        preserves: &[],
        does_not_preserve: &["cobertura (completude não certificada)", "alcançabilidade exata", "deadlock", "vivacidade", "CTL"],
        reference_ids: &["R2", "R4", "R6"], implementation_version: Some("racon-coverability finkel-legacy"), proof_mapping: None,
        notes: "Spec §8.2: manter só como legacy_experimental e remover do caminho padrão. Prefira MinCov.",
    },
    AlgorithmEntry {
        id: "coverability_mincov", name: "MinCov (Finkel–Haddad–Khmelnitsky)", family: "coverability",
        role: Role::Exploration, status: AlgorithmStatus::Experimental, selectable: true,
        objectives: &[Objective::Coverability, Objective::Boundedness], supported_net_classes: PT,
        description: "Constrói uma árvore de cobertura mínima memorizando as ACELERAÇÕES (ω-transições) em vez de marcações ω: dispara aceleração já conhecidas, descarta vértices cobertos (limpeza), cria nova aceleração ao achar ancestral estritamente menor e poda a subárvore. Termina com Clover(N,m0), o conjunto mínimo de cobertura.",
        when_to_use: "Cobertura e limitação com menor memória que Karp–Miller e, ao contrário da poda histórica de Finkel, com prova de completude (R6).",
        when_not_to_use: "Não é um grafo de transições: não use para alcançabilidade exata, deadlock, vivacidade, reversibilidade, caminhos ou CTL.",
        output: "conjunto mínimo de cobertura Clover (antichain de ω-marcações)",
        preserves: &["cobertura", "limitação", "lugares ilimitados"],
        does_not_preserve: &["alcançabilidade exata", "deadlock", "vivacidade", "reversibilidade", "CTL", "caminhos"],
        reference_ids: &["R6", "R7"], implementation_version: Some("racon-coverability mincov-1 (R6 Algoritmo 1)"),
        proof_mapping: Some("crates/racon-coverability/src/mincov.rs"),
        notes: "Experimental até a revisão técnica do mapeamento passo a passo do artigo. Validado contra Karp–Miller e contra exploração exaustiva em 600 redes aleatórias.",
    },
    AlgorithmEntry {
        id: "coverability_mp", name: "Monotone-Pruning (Reynier–Servais)", family: "coverability",
        role: Role::Exploration, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Coverability, Objective::Boundedness], supported_net_classes: PT,
        description: "Karp–Miller com poda horizontal entre ramos (Algoritmo 2 de R4): um nó x é desativado por n se algum ancestral y de x satisfaz Λ(y) ≤ Λ(n) e (y ativo ou y não é ancestral de n).",
        when_to_use: "Conjunto mínimo de cobertura com qualquer estratégia de exploração (largura ou profundidade) e prova de completude.",
        when_not_to_use: "Não é um grafo de transições: não use para alcançabilidade exata, deadlock, vivacidade, reversibilidade, caminhos ou CTL. Recusado em redes com inibidores ou precedência de imediatas.",
        output: "conjunto mínimo de cobertura (antichain de ω-marcações)",
        preserves: &["cobertura", "limitação", "lugares ilimitados"],
        does_not_preserve: &["alcançabilidade exata", "deadlock", "vivacidade", "reversibilidade", "CTL", "caminhos"],
        reference_ids: &["R4"], implementation_version: Some("racon-coverability monotone-pruning (R4 Algoritmo 2)"),
        proof_mapping: Some("crates/racon-coverability/src/mp.rs"),
        notes: "Implementado em racon_coverability::MonotonePruning, acessível por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis (aba Grafos, como conjunto de cobertura); não é job de exploração do motor (selectable=false). Validado contra Karp–Miller em 600 redes aleatórias (BFS e DFS) e no exemplo 2.1 do artigo. Condição de poda: Remark 3.1 (a condição simples perde terminação).",
    },
    AlgorithmEntry {
        id: "coverability_grvb", name: "Geeraerts–Raskin–Van Begin (CovProc)", family: "coverability",
        role: Role::Exploration, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Coverability, Objective::Boundedness], supported_net_classes: PT,
        description: "Sequência de cobertura sobre pares de ω-marcações (ordem ⊑) com oráculo recursivo (Algoritmo 3 de R3); alternativa correta ao MCT, que o artigo mostra incompleto.",
        when_to_use: "Conjunto mínimo de cobertura sem árvore de Karp–Miller; a recursão cresce em número de ω, limitada por |P|.",
        when_not_to_use: "Não é um grafo de transições: não use para alcançabilidade exata, deadlock, vivacidade, reversibilidade, caminhos ou CTL. Recusado em redes com inibidores ou precedência de imediatas.",
        output: "conjunto mínimo de cobertura (antichain de ω-marcações)",
        preserves: &["cobertura", "limitação", "lugares ilimitados"],
        does_not_preserve: &["alcançabilidade exata", "deadlock", "vivacidade", "reversibilidade", "CTL", "caminhos"],
        reference_ids: &["R3"], implementation_version: Some("racon-coverability covproc (R3 Algoritmo 3)"),
        proof_mapping: Some("crates/racon-coverability/src/covproc.rs"),
        notes: "Implementado em racon_coverability::CovProc, Acessível por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis (aba Grafos, como conjunto de cobertura); não é job de exploração do motor (selectable=false). Validado contra Karp–Miller em 600 redes aleatórias, com e sem oráculo. Condição de parada, Max⊑ e V/F/O seguem a Seção 5.",
    },
    AlgorithmEntry {
        id: "partial_order_stubborn", name: "Conjuntos teimosos (stubborn sets)", family: "partial_order",
        role: Role::Reduction, status: AlgorithmStatus::Experimental, selectable: true,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Em cada marcação explora apenas as transições habilitadas de um conjunto teimoso, fechado sob conflito e sob habilitação das transições desabilitadas, evitando intercalações independentes.",
        when_to_use: "Detectar deadlocks em redes com muita concorrência independente.",
        when_not_to_use: "A variante implementada NÃO tem proviso de 'ignoring': não leia vivacidade, transições mortas, reversibilidade, DCF, limitação ou CTL do grafo reduzido. Bloqueado com algoritmos de cobertura.",
        output: "grafo de alcançabilidade reduzido",
        preserves: &["deadlock (alcançabilidade de estado morto)"],
        does_not_preserve: &["vivacidade", "transições mortas", "reversibilidade", "DCF", "limitação", "CTL", "caminho mínimo"],
        reference_ids: &["R9", "R10"], implementation_version: Some("racon-reduction stubborn-1"), proof_mapping: None,
        notes: "Exploração de grafo (deadlock): variante interna de Valmari (1990). Em \"Alcançar uma marcação\" e Bad States a busca usa a variante orientada à propriedade de R9 (Teoremas 3-4; goal_search.rs), validada por equivalência aleatória contra a busca completa; sem proviso de ignoring, pois o conjunto sempre contém as transições interessantes. Limitação (R9, Teoremas 1-2): bounded_search.rs decide a limitação da rede ou de um lugar no grafo reduzido (atrator A = transições que produzem mais do que consomem; •p para um lugar); a variante com ω-aceleração (decide_boundedness_stubborn_omega) decide também um lugar isolado quando outro cresce sem limite; Caminho mais curto (Teorema 5): a busca em largura devolve um caminho de comprimento mínimo. Transições mortas (Teorema 6): dead_transitions.rs usa o atrator {t} e devolve a sequência mais curta que habilita t. A rede inteira está ligada ao job de análise e a limitação por lugar à rota GET /v1/sessions/{id}/places/{place}/boundedness.",
    },
    AlgorithmEntry {
        id: "structural_rules", name: "Reduções estruturais (Thierry-Mieg)", family: "structural",
        role: Role::Reduction, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Simplifica lugares e transições antes da exploração por regras com condições formais de preservação (R8, Seção 5): 1-11 e 14-17, com objetivo deadlock ou segurança com suporte.",
        when_to_use: "Reduzir a rede antes de verificar deadlock ou invariantes de segurança sobre um conjunto de lugares (o suporte), que nunca é removido.",
        when_not_to_use: "A rede reduzida é outra rede: não use para grafo de alcançabilidade, vivacidade, limitação de lugares removidos, reversibilidade ou CTL da rede original. Recusado em redes com inibidores ou precedência de imediatas. Regras 12, 13 (implícito/equivalente futuro), 18-20 (grafos) e 21-22 (SMT) não implementadas. As regras por consulta A-I de R10 (query_rules.rs) são distintas.",
        output: "rede reduzida (ou vazia) + contagem por regra + aglomerações", preserves: &[], does_not_preserve: &[],
        reference_ids: &["R8"], implementation_version: Some("racon-reduction thierry-mieg (R8 regras 1-11, 14-17)"),
        proof_mapping: Some("crates/racon-reduction/src/thierry_mieg.rs"),
        notes: "Implementado em racon_reduction::thierry_mieg::reduce, Acessível por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis (aba Métodos avançados); não é opção de exploração (selectable=false). Validado por oráculo de exploração exaustiva: projeção das marcações alcançáveis sobre o suporte (segurança) e existência de deadlock (deadlock) em milhares de redes aleatórias; todas as regras implementadas disparam nos testes. O StructuralReducer antigo (rules.rs) não é o mesmo e não é certificado.",
    },
    AlgorithmEntry {
        id: "polyhedral_abstraction", name: "Abstração poliédrica + SMT", family: "structural",
        role: Role::Reduction, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Reduz a rede a (N, E, N'), com E um sistema de equações lineares entre os lugares de N e de N' (R11, Seção 3), e verifica EF F em N testando a E-transformada de F em N' (Teorema 4.3).",
        when_to_use: "Alcançabilidade de uma fórmula linear sobre marcações quando a rede tem lugares redundantes, cadeias ou laços silenciosos.",
        when_not_to_use: "O solver SMT não está no workspace: a decisão é por exploração explícita da rede reduzida (precisa de rede reduzida limitada); o BMC em SMT-LIB é só gerado, não executado. Equivalência de marcações, sem rótulos/observações. Recusado em redes com inibidores ou precedência de imediatas. Não preserva deadlock, vivacidade, CTL nem o grafo.",
        output: "rede reduzida + sistema E + veredito (alcançável com marcação testemunha, inalcançável ou desconhecido)",
        preserves: &[], does_not_preserve: &[],
        reference_ids: &["R11"], implementation_version: Some("racon-reduction polyhedral (R11 DEADP/CONSTANT/CONCAT/AGG/RED)"),
        proof_mapping: Some("crates/racon-reduction/src/polyhedral.rs"),
        notes: "Implementado em racon_reduction::polyhedral (regras DEADP, CONSTANT, CONCAT, AGG, RED). Validado por oráculo em 1308 redes aleatórias: as marcações obtidas elevando todas as marcações alcançáveis de N' por E são exatamente as alcançáveis de N, e EF F coincide. Variantes com pesos/ciclos longos, REDT, SOURCE, PDR e BMC executado não implementados. Acessível por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis (aba Métodos avançados); não é opção de exploração (selectable=false).",
    },
    AlgorithmEntry {
        id: "unfolding_prefix", name: "Prefixo de unfolding (ERV)", family: "representation",
        role: Role::Reduction, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Prefixo finito completo do unfolding de uma rede 1-limitada (Algoritmo 3.6 de R12) com a ordem adequada total ≺r da Definição 4.1; as marcações alcançáveis são as dos cortes do prefixo.",
        when_to_use: "Redes 1-limitadas com muita concorrência: alcançabilidade de marcação, transições mortas (quasi-vivacidade) e deadlock sem enumerar o entrelaçamento. Pode ser exponencialmente menor que a ordem de McMillan.",
        when_not_to_use: "Só redes 1-limitadas com arcos de peso 1 e transições com pré e pós-conjuntos não vazios; recusa redes não seguras (detectado durante o unfolding). Não é método de cobertura ilimitada nem grafo de transições: não use para vivacidade, reversibilidade, caminhos ou CTL. Recusado com inibidores ou precedência de imediatas.",
        output: "prefixo (condições, eventos, eventos cut-off)", preserves: &[], does_not_preserve: &[],
        reference_ids: &["R12"], implementation_version: Some("racon-reduction unfolding (R12 Algoritmo 3.6, ordem ERV)"),
        proof_mapping: Some("crates/racon-reduction/src/unfolding.rs"),
        notes: "Implementado em racon_reduction::unfolding (ordens Erv e McMillan). Validado por oráculo em 3034 redes 1-limitadas aleatórias: marcações dos cortes = alcançáveis, transições sem evento = mortas, deadlock coincide, ERV nunca maior que McMillan; 966 redes não seguras são recusadas. A família da Fig. 5 dá prefixo linear (ERV) contra exponencial (McMillan). Não aplica o cut-off pela marcação inicial (a definição 3.5 usa só configurações locais). Acessível por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis (aba Métodos avançados); não é opção de exploração (selectable=false).",
    },
    AlgorithmEntry {
        id: "symmetry_reduction", name: "Redução por simetria", family: "symmetry",
        role: Role::Reduction, status: AlgorithmStatus::Experimental, selectable: true,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Explora um representante por classe de marcações equivalentes sob os automorfismos da rede: cada marcação sucessora é trocada pela menor marcação lexicográfica da sua órbita.",
        when_to_use: "Redes com partes intercambiáveis (processos idênticos, réplicas, anéis). Quanto maior o grupo de simetrias, menor o grafo.",
        when_not_to_use: "Redes sem simetria (nada é reduzido), com arcos inibidores ou transições imediatas, ou quando importam propriedades de um lugar ou de uma transição específicos.",
        output: "grafo de alcançabilidade quociente (um representante por órbita)",
        preserves: &["existência de deadlock", "alcançabilidade de uma marcação, a menos de equivalência", "limitação da rede", "CTL* apenas com proposições invariantes por simetria"],
        does_not_preserve: &["limitação de um lugar específico", "transição morta ou viva específica", "número de estados e de arestas", "caminho mínimo até uma marcação específica"],
        reference_ids: &["R14"], implementation_version: Some("racon-reduction symmetry-2 (Junttila 2003, Alg. 4.2)"), proof_mapping: Some("crates/racon-reduction/src/symmetry.rs"),
        notes: "O grupo de automorfismos é buscado por individualização e refinamento (§3.2) e cada gerador é conferido contra a rede. O representante é a menor marcação da órbita, por busca na representação de Schreier–Sims (Alg. 4.2). Por padrão o grupo fixa a marcação inicial (Cor. 2.13). Se a busca atinge o limite, usa-se um subgrupo: o resultado continua correto, mas pode ficar maior.",
    },
    AlgorithmEntry {
        id: "decision_diagrams", name: "Diagramas de decisão", family: "symbolic",
        role: Role::Exploration, status: AlgorithmStatus::Experimental, selectable: false,
        objectives: &[Objective::Reachability], supported_net_classes: PT,
        description: "Representação simbólica do conjunto de marcações alcançáveis: MDD com Saturation (Ciardo 2001) e BDD de Bryant 1986 para redes 1-safe.",
        when_to_use: "Contar e consultar o conjunto de marcações alcançáveis (pertinência, limites por lugar) em redes cujo espaço de estados explícito é grande demais.",
        when_not_to_use: "Não gera o grafo de transições: não use para deadlock com caminho, vivacidade, reversibilidade, caminhos ou CTL. O BDD exige rede 1-safe. Não é opção de exploração; roda na aba Métodos avançados.",
        output: "conjunto de marcações alcançáveis (contagem, pertinência, limites por lugar)", preserves: &[], does_not_preserve: &[],
        reference_ids: &["R13"], implementation_version: Some("racon-reduction decision-diagrams (MDD+Saturation, BDD)"), proof_mapping: Some("crates/racon-reduction/src/decision_diagrams.rs"), notes: "Implementados: MDD + Saturation (decision_diagrams.rs, Ciardo 2001) e BDD de Bryant 1986 (bdd.rs, redes 1-safe). Acessíveis como symbolic_mdd e symbolic_bdd por POST /v1/sessions/{id}/algorithms/{id} e pelo comando Tauri run_advanced_analysis. Geram só o conjunto de marcações (contagem, pertinência, limites por lugar), não o grafo de transições. R13 é só mapa de ferramentas.",
    },
];

pub fn algorithm(id: &str) -> Option<&'static AlgorithmEntry> {
    ALGORITHMS.iter().find(|a| a.id == id)
}

pub fn reference(id: &str) -> Option<&'static Reference> {
    REFERENCES.iter().find(|r| r.id == id)
}

// ---------------------------------------------------------------------------
// Configuration validation (POST /analysis/config/validate)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigRequest {
    pub objective: Objective,
    /// `none` | `manual` | `automatic`
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Algorithm id; ignored (chosen by the system) in `automatic` mode.
    #[serde(default)]
    pub exploration: Option<String>,
    #[serde(default)]
    pub reductions: Vec<String>,
    pub net_class: NetClass,
}

fn default_mode() -> String {
    "manual".to_owned()
}

#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub catalog_version: &'static str,
    /// `valid` | `unsupported_combination`
    pub status: &'static str,
    pub selected: Vec<String>,
    pub applied: Vec<Decision>,
    pub ignored: Vec<Decision>,
    pub issues: Vec<Issue>,
    pub warnings: Vec<String>,
    pub automatic_decisions: Vec<Decision>,
    pub pipeline: Vec<String>,
    pub preserves: Vec<&'static str>,
    pub does_not_preserve: Vec<&'static str>,
    pub reference_ids: Vec<&'static str>,
    pub experimental: bool,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.status == "valid"
    }
}

/// Validates a configuration against the capability matrix. Never executes
/// anything and never silently replaces a method the user chose (spec §2.3).
pub fn validate(request: &ConfigRequest) -> ValidationReport {
    let mut report = ValidationReport {
        catalog_version: CATALOG_VERSION,
        status: "valid",
        selected: Vec::new(),
        applied: Vec::new(),
        ignored: Vec::new(),
        issues: Vec::new(),
        warnings: Vec::new(),
        automatic_decisions: Vec::new(),
        pipeline: Vec::new(),
        preserves: Vec::new(),
        does_not_preserve: Vec::new(),
        reference_ids: Vec::new(),
        experimental: false,
    };
    let fail = |report: &mut ValidationReport, code: &'static str, message: String| {
        report.status = "unsupported_combination";
        report.issues.push(Issue { code, message });
    };

    let mode = request.mode.as_str();
    if !matches!(mode, "none" | "manual" | "automatic") {
        fail(&mut report, "unknown_mode", format!("Modo '{mode}' desconhecido; use none, manual ou automatic."));
        return report;
    }

    // --- choose the exploration backend -----------------------------------
    let exploration_id: Option<String> = if mode == "automatic" {
        // Only combinations certified `stable` may be chosen automatically.
        let choice = match (request.objective, request.net_class) {
            (Objective::Reachability, _) => Some(("reachability_bfs", "BFS é a única exploração exata estável para alcançabilidade com caminhos mínimos.")),
            (_, NetClass::PtWeighted) => Some(("coverability_karp_miller", "Karp–Miller é o único backend de cobertura 'stable'; MinCov permanece experimental.")),
            (_, NetClass::ExtendedPt) => None,
        };
        match choice {
            Some((id, reason)) => {
                report.automatic_decisions.push(Decision { id: id.to_owned(), reason: reason.to_owned() });
                if !request.reductions.is_empty() {
                    report.ignored.extend(request.reductions.iter().map(|r| Decision {
                        id: r.clone(),
                        reason: "O modo automático só aplica reduções certificadas; nenhuma está disponível.".to_owned(),
                    }));
                }
                Some(id.to_owned())
            }
            None => {
                fail(&mut report, "no_certified_method", "Nenhum método certificado para cobertura/limitação em redes com inibidores ou transições imediatas (a monotonicidade falha). Use alcançabilidade exata.".to_owned());
                None
            }
        }
    } else {
        request.exploration.clone()
    };

    let Some(exploration_id) = exploration_id else {
        if report.is_valid() {
            fail(&mut report, "missing_exploration", "Escolha um algoritmo de exploração.".to_owned());
        }
        return report;
    };
    report.selected.push(exploration_id.clone());

    let Some(exploration) = algorithm(&exploration_id) else {
        fail(&mut report, "unknown_algorithm", format!("Algoritmo '{exploration_id}' não existe no catálogo."));
        return report;
    };
    check_entry(&mut report, exploration, request, Role::Exploration);

    // --- reductions --------------------------------------------------------
    let reductions: Vec<String> = if mode == "automatic" { Vec::new() } else { request.reductions.clone() };
    if mode == "none" && !reductions.is_empty() {
        fail(&mut report, "reductions_with_none", "O modo 'none' não aplica nenhuma redução; remova as reduções ou use o modo manual.".to_owned());
    }
    let mut seen = Vec::new();
    for id in &reductions {
        report.selected.push(id.clone());
        if seen.contains(id) {
            fail(&mut report, "duplicate_reduction", format!("A redução '{id}' foi selecionada mais de uma vez."));
            continue;
        }
        seen.push(id.clone());
        let Some(entry) = algorithm(id) else {
            fail(&mut report, "unknown_algorithm", format!("Redução '{id}' não existe no catálogo."));
            continue;
        };
        check_entry(&mut report, entry, request, Role::Reduction);

        // §4.2: stubborn sets + any coverability backend are blocked.
        if entry.id == "partial_order_stubborn" && exploration.family == "coverability" {
            fail(&mut report, "blocked_combination", "Stubborn sets + algoritmos de cobertura: bloqueada — não há teorema que garanta a preservação de Clover por redução orientada a deadlock/alcançabilidade.".to_owned());
        }
        if entry.id == "unfolding_prefix" && exploration.family == "coverability" {
            fail(&mut report, "blocked_combination", "Unfolding + cobertura ilimitada: bloqueada.".to_owned());
        }
    }
    if reductions.iter().any(|r| r == "symmetry_reduction") && reductions.iter().any(|r| r == "partial_order_stubborn") {
        fail(&mut report, "blocked_combination", "Simetria + stubborn sets: bloqueada; exige provas de preservação específicas para a combinação.".to_owned());
    }

    if !report.is_valid() {
        return report;
    }

    // --- guarantees --------------------------------------------------------
    let mut chain: Vec<&AlgorithmEntry> = vec![exploration];
    chain.extend(reductions.iter().filter_map(|id| algorithm(id)));
    // A property is guaranteed only if EVERY step guarantees it (§4.3: the
    // naive union of property names is not a proof).
    let mut preserved: Vec<&'static str> = exploration.preserves.to_vec();
    for step in chain.iter().skip(1) {
        preserved.retain(|p| step.preserves.contains(p) || step.preserves.iter().any(|q| q.starts_with(*p)));
        if step.preserves.is_empty() {
            preserved.clear();
        }
    }
    // reductions preserving a property the exploration cannot give: nothing.
    report.preserves = preserved;
    let mut lost: Vec<&'static str> = Vec::new();
    for step in &chain {
        for property in step.does_not_preserve {
            if !lost.contains(property) {
                lost.push(property);
            }
        }
    }
    report.does_not_preserve = lost;
    for step in &chain {
        report.reference_ids.extend(step.reference_ids.iter().copied().filter(|r| !report.reference_ids.contains(r)).collect::<Vec<_>>());
        report.applied.push(Decision {
            id: step.id.to_owned(),
            reason: match step.role {
                Role::Exploration => "algoritmo de exploração".to_owned(),
                Role::Reduction => "redução aplicada durante a exploração".to_owned(),
            },
        });
        report.pipeline.push(format!("{} — {}", step.name, step.output));
    }
    report.experimental = chain.iter().any(|s| matches!(s.status, AlgorithmStatus::Experimental | AlgorithmStatus::LegacyExperimental));
    if report.experimental {
        report.warnings.push("Há algoritmos experimentais na configuração: o resultado será marcado como experimental, sem declaração de prova definitiva.".to_owned());
    }
    if matches!(request.net_class, NetClass::ExtendedPt) {
        report.warnings.push("Rede estendida (racon-extended-pt-v1): somente exploração exata; resultados truncados são 'unknown'.".to_owned());
    }
    report
}

fn check_entry(report: &mut ValidationReport, entry: &AlgorithmEntry, request: &ConfigRequest, expected_role: Role) {
    let mut bad = |code: &'static str, message: String| {
        report.status = "unsupported_combination";
        report.issues.push(Issue { code, message });
    };
    if !entry.selectable {
        let why = match entry.status {
            AlgorithmStatus::Experimental | AlgorithmStatus::Stable => "implementado, mas executado na aba Métodos avançados, não como opção de exploração ou redução",
            AlgorithmStatus::Proposed => "publicação identificada, mas o algoritmo ainda não foi implementado",
            AlgorithmStatus::UnderStudy => "em estudo: sem artigo específico validado",
            _ => "indisponível",
        };
        bad("not_selectable", format!("'{}' não pode ser selecionado: {why}.", entry.name));
        return;
    }
    if entry.role != expected_role {
        bad("wrong_role", format!("'{}' é uma {} e não pode ser usada como {}.",
            entry.name,
            if entry.role == Role::Reduction { "redução" } else { "exploração" },
            if expected_role == Role::Reduction { "redução" } else { "algoritmo de exploração" }));
    }
    if !entry.objectives.contains(&request.objective) {
        bad("objective_mismatch", format!("'{}' não atende ao objetivo selecionado; não confunda algoritmo de cobertura com alcançabilidade exata.", entry.name));
    }
    if !entry.supported_net_classes.contains(&request.net_class) {
        bad("net_class_unsupported", format!("'{}' não é válido para a classe de rede selecionada.{}", entry.name,
            if request.net_class == NetClass::ExtendedPt { " Inibidores e precedência de imediatas quebram a monotonicidade exigida." } else { "" }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(objective: Objective, exploration: &str, reductions: &[&str], net_class: NetClass) -> ConfigRequest {
        ConfigRequest {
            objective, mode: "manual".into(), exploration: Some(exploration.into()),
            reductions: reductions.iter().map(|s| s.to_string()).collect(), net_class,
        }
    }

    #[test]
    fn catalogue_ids_are_unique_and_references_resolve() {
        let mut ids: Vec<_> = ALGORITHMS.iter().map(|a| a.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ALGORITHMS.len());
        for a in ALGORITHMS {
            for r in a.reference_ids {
                assert!(reference(r).is_some(), "{} cites unknown {r}", a.id);
            }
            // spec §3: only implemented algorithms are selectable
            if a.selectable { assert!(a.implementation_version.is_some(), "{}", a.id); }
            if matches!(a.status, AlgorithmStatus::Proposed | AlgorithmStatus::UnderStudy) {
                assert!(!a.selectable, "{}", a.id);
            }
        }
    }

    #[test]
    fn baseline_is_valid_and_guarantees_everything_for_bfs() {
        let r = validate(&req(Objective::Reachability, "reachability_bfs", &[], NetClass::PtWeighted));
        assert!(r.is_valid());
        assert!(r.preserves.contains(&"CTL"));
        assert!(!r.experimental);
    }

    #[test]
    fn mincov_is_valid_for_cover_but_flagged_experimental_and_loses_exactness() {
        let r = validate(&req(Objective::Coverability, "coverability_mincov", &[], NetClass::PtWeighted));
        assert!(r.is_valid() && r.experimental);
        assert!(r.does_not_preserve.contains(&"deadlock"));
        assert!(!r.preserves.contains(&"deadlock"));
    }

    #[test]
    fn coverability_algorithm_is_rejected_for_a_reachability_objective() {
        let r = validate(&req(Objective::Reachability, "coverability_mincov", &[], NetClass::PtWeighted));
        assert_eq!(r.status, "unsupported_combination");
        assert!(r.issues.iter().any(|i| i.code == "objective_mismatch"));
    }

    #[test]
    fn symmetry_is_offered_for_plain_nets_only_and_never_with_stubborn_sets() {
        let ok = validate(&req(Objective::Reachability, "reachability_bfs", &["symmetry_reduction"], NetClass::PtWeighted));
        assert_eq!(ok.status, "valid", "{:?}", ok.issues);
        let extended = validate(&req(Objective::Reachability, "reachability_bfs", &["symmetry_reduction"], NetClass::ExtendedPt));
        assert_eq!(extended.status, "unsupported_combination");
        let both = validate(&req(Objective::Reachability, "reachability_bfs", &["symmetry_reduction", "partial_order_stubborn"], NetClass::PtWeighted));
        assert!(both.issues.iter().any(|i| i.code == "blocked_combination"));
    }

    #[test]
    fn every_selectable_method_cites_a_source() {
        for entry in ALGORITHMS.iter().filter(|a| a.selectable) {
            assert!(!entry.reference_ids.is_empty(), "{} has no reference", entry.id);
            for id in entry.reference_ids {
                assert!(REFERENCES.iter().any(|r| r.id == *id), "{} cites unknown {}", entry.id, id);
            }
        }
    }

    #[test]
    fn unimplemented_and_under_study_methods_are_never_selectable() {
        for id in ["coverability_mp", "coverability_grvb", "structural_rules", "decision_diagrams"] {
            let objective = if id.starts_with("coverability") { Objective::Coverability } else { Objective::Reachability };
            let (exploration, reductions): (&str, Vec<&str>) = if algorithm(id).unwrap().role == Role::Reduction {
                ("reachability_bfs", vec![id])
            } else {
                (id, vec![])
            };
            let r = validate(&req(objective, exploration, &reductions, NetClass::PtWeighted));
            assert_eq!(r.status, "unsupported_combination", "{id}");
        }
    }

    #[test]
    fn stubborn_with_coverability_is_blocked() {
        let r = validate(&req(Objective::Coverability, "coverability_karp_miller", &["partial_order_stubborn"], NetClass::PtWeighted));
        assert_eq!(r.status, "unsupported_combination");
    }

    #[test]
    fn stubborn_with_bfs_keeps_only_deadlock_and_drops_liveness() {
        let r = validate(&req(Objective::Reachability, "reachability_bfs", &["partial_order_stubborn"], NetClass::PtWeighted));
        assert!(r.is_valid() && r.experimental);
        assert!(!r.preserves.contains(&"vivacidade") && !r.preserves.contains(&"CTL"));
        assert!(r.does_not_preserve.contains(&"vivacidade"));
        assert!(r.preserves.iter().any(|p| p.starts_with("deadlock")));
    }

    #[test]
    fn extended_nets_allow_only_exact_exploration() {
        assert!(validate(&req(Objective::Reachability, "reachability_dfs", &[], NetClass::ExtendedPt)).is_valid());
        for (obj, id) in [(Objective::Coverability, "coverability_karp_miller"), (Objective::Coverability, "coverability_mincov")] {
            assert_eq!(validate(&req(obj, id, &[], NetClass::ExtendedPt)).status, "unsupported_combination");
        }
        assert_eq!(
            validate(&req(Objective::Reachability, "reachability_bfs", &["partial_order_stubborn"], NetClass::ExtendedPt)).status,
            "unsupported_combination"
        );
    }

    #[test]
    fn none_mode_applies_no_reduction_and_automatic_records_its_choice() {
        let mut none = req(Objective::Reachability, "reachability_bfs", &["partial_order_stubborn"], NetClass::PtWeighted);
        none.mode = "none".into();
        assert_eq!(validate(&none).status, "unsupported_combination");

        let auto = ConfigRequest { objective: Objective::Boundedness, mode: "automatic".into(), exploration: None, reductions: vec!["partial_order_stubborn".into()], net_class: NetClass::PtWeighted };
        let r = validate(&auto);
        assert!(r.is_valid());
        assert_eq!(r.automatic_decisions[0].id, "coverability_karp_miller");
        assert_eq!(r.ignored.len(), 1);
        assert!(!r.experimental);

        let auto_ext = ConfigRequest { net_class: NetClass::ExtendedPt, ..auto };
        assert_eq!(validate(&auto_ext).status, "unsupported_combination");
    }
}
