use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{ArcEndpoint, PetriNet, TransitionId};
use racon_core::StateSpace;
use racon_coverability::CoverabilityGraph;
use racon_invariants::types::InvariantAnalysisResult;
use racon_reachability::ReachabilityGraph;
use racon_structural::topology::StructuralProperties;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    Proven,
    Disproven,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PropertyId {
    B,
    Dst,
    Dtr,
    Rev,
    L,
    Ls,
    Dcf,
    Ord,
    Hom,
    Con,
    Sc,
    Pur,
    Scf,
    Mg,
    Fc,
    Efc,
    PInvariants,
    TInvariants,
    Cpi,
    Cti,
    Csv,
    Sb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathematicalDefinition {
    pub name: String,
    pub formula_latex: String,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceMethod {
    ReachabilityGraph,
    KarpMillerCoverability,
    StructuralTopology,
    IncidenceAlgebra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceMetadata {
    pub analyzer_version: String,
    pub algorithm: String,
    pub exact_state_space: bool,
    pub node_count: Option<usize>,
    pub edge_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathReference {
    pub author: String,
    pub title: String,
    pub year: Option<u16>,
    pub page: Option<String>,
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProofKind {
    Proof,
    Counterproof,
    AlgebraicCertificate,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofFact {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub formula_latex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofStep {
    pub text: String,
    #[serde(default)]
    pub formula_latex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Human-readable mathematical derivation, kept separate from the typed
/// technical certificate so the same result can feed the UI, Markdown and a
/// future verifier without making the UI interpret analyzer internals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathematicalCertificate {
    pub kind: ProofKind,
    pub proposition: String,
    pub assumptions: Vec<String>,
    pub facts: Vec<ProofFact>,
    pub derivation: Vec<ProofStep>,
    pub conclusion: String,
    pub completeness: String,
    #[serde(default)]
    pub tables: Vec<ProofTable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiringWitness {
    pub start: usize,
    pub transitions: Vec<usize>,
    pub markings: Vec<Vec<String>>,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedCertificate {
    pub state_count: usize,
    pub place_bounds: Vec<String>,
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmegaCertificate {
    pub node: usize,
    pub marking: Vec<String>,
    pub path: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadStateCertificate {
    pub node: usize,
    pub marking: Vec<String>,
    pub path: Vec<usize>,
    pub enabled_transitions: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadTransitionCertificate {
    pub transition: usize,
    pub enabled_nodes: Vec<usize>,
    pub state_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReversibilityCertificate {
    pub state_count: usize,
    pub initial_node: usize,
    pub non_reversible_node: Option<usize>,
    pub path: Option<FiringWitness>,
    #[serde(default)]
    pub return_paths: Vec<FiringWitness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LivenessCertificate {
    pub transition: usize,
    pub enabled_nodes: Vec<usize>,
    pub predecessor_closure_size: usize,
    pub state_space_size: usize,
    pub counterexample_node: Option<usize>,
    #[serde(default)]
    pub per_transition: Vec<LivenessTransitionCertificate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LivenessTransitionCertificate {
    pub transition: usize,
    pub enabled_nodes: Vec<usize>,
    pub predecessor_closure_size: usize,
    pub state_space_size: usize,
    pub holds: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictCertificate {
    pub state_count: usize,
    pub enabled_pairs: usize,
    pub conflicts: Vec<(usize, usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralCertificate {
    pub property: PropertyId,
    pub details: Vec<String>,
    pub counterexample: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantCertificate {
    pub coefficients: Vec<String>,
    pub product: Vec<String>,
    pub constant: Option<String>,
    pub support: Vec<usize>,
    #[serde(default)]
    pub realizability: Option<bool>,
    #[serde(default)]
    pub realization_witness: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageCertificate {
    pub covered: Vec<usize>,
    pub uncovered: Vec<usize>,
    pub strictly_positive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Certificate {
    Bounded(BoundedCertificate),
    Omega(OmegaCertificate),
    DeadState(DeadStateCertificate),
    DeadTransition(DeadTransitionCertificate),
    Reversibility(ReversibilityCertificate),
    Liveness(LivenessCertificate),
    DynamicConflictFree(ConflictCertificate),
    Structural(StructuralCertificate),
    Invariant(InvariantCertificate),
    Coverage(CoverageCertificate),
}

impl Certificate {
    pub fn summary(&self) -> String {
        match self {
            Self::Bounded(c) => format!(
                "{} estados; limites por lugar: {}",
                c.state_count,
                c.place_bounds.join(", ")
            ),
            Self::Omega(c) => format!(
                "nó {} contém ω; caminho com {} disparos",
                c.node,
                c.path.len()
            ),
            Self::DeadState(c) => format!(
                "nó {}; habilitadas: {}",
                c.node,
                c.enabled_transitions.len()
            ),
            Self::DeadTransition(c) => format!(
                "t{} habilitada em {} de {} estados",
                c.transition,
                c.enabled_nodes.len(),
                c.state_count
            ),
            Self::Reversibility(c) => {
                format!("{} estados; nó inicial {}", c.state_count, c.initial_node)
            }
            Self::Liveness(c) => format!(
                "t{}: closure {}/{}",
                c.transition, c.predecessor_closure_size, c.state_space_size
            ),
            Self::DynamicConflictFree(c) => format!(
                "{} pares habilitados; {} conflitos",
                c.enabled_pairs,
                c.conflicts.len()
            ),
            Self::Structural(c) => c.details.join("; "),
            Self::Invariant(c) => format!(
                "coeficientes [{}]; produto [{}]",
                c.coefficients.join(", "),
                c.product.join(", ")
            ),
            Self::Coverage(c) => format!(
                "{} cobertos; {} descobertos",
                c.covered.len(),
                c.uncovered.len()
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathematicalEvidence {
    pub property: PropertyId,
    #[serde(default)]
    pub sequence: Option<usize>,
    pub result: Option<bool>,
    pub status: EvidenceStatus,
    pub definition: MathematicalDefinition,
    pub proof: MathematicalCertificate,
    pub method: EvidenceMethod,
    pub certificate: Option<Certificate>,
    pub counterexample: Option<Certificate>,
    pub assumptions: Vec<String>,
    pub references: Vec<MathReference>,
    pub metadata: EvidenceMetadata,
}

fn definition(property: PropertyId) -> MathematicalDefinition {
    let (name, formula, explanation) = match property {
        PropertyId::B => (
            "Bounded",
            r"\forall p\in P,\exists k_p:\forall M\in R(M_0), M(p)\le k_p",
            "Cada lugar possui um limite finito em todas as marcações alcançáveis.",
        ),
        PropertyId::Dst => (
            "Dead State",
            r"\exists M\in R(M_0): Enabled(M)=\varnothing",
            "Existe uma marcação alcançável sem transições habilitadas.",
        ),
        PropertyId::Dtr => (
            "Dead Transition",
            r"\exists t\in T:\forall M\in R(M_0),\neg M[t\rangle",
            "Existe uma transição que nunca é habilitada.",
        ),
        PropertyId::Rev => (
            "Reversibility",
            r"\forall M\in R(M_0): M\rightarrow^*M_0",
            "Toda marcação alcançável retorna à marcação inicial.",
        ),
        PropertyId::L => (
            "Liveness",
            r"\forall t\in T,\forall M\in R(M_0),\exists M'\in R(M):M'[t\rangle",
            "Toda transição pode voltar a ser habilitada a partir de qualquer estado.",
        ),
        PropertyId::Ls => (
            "Live and Safe",
            r"L\land Safe",
            "A rede é viva e segura simultaneamente.",
        ),
        PropertyId::Dcf => (
            "Dynamic Conflict Free",
            r"\forall M\in R(M_0):\neg Conflict(M)",
            "Nenhuma marcação alcançável possui conflito dinâmico.",
        ),
        PropertyId::Ord => (
            "Ordinary",
            r"\forall f\in F: V(f)=1",
            "Todos os arcos têm peso unitário.",
        ),
        PropertyId::Hom => (
            "Homogeneous",
            r"\forall p,\forall t_1,t_2\in p^\bullet: V(p,t_1)=V(p,t_2)",
            "Arcos de saída de cada lugar têm o mesmo peso.",
        ),
        PropertyId::Con => (
            "Connected",
            r"\forall x,y\in P\cup T:x(F\cup F^{-1})^*y",
            "O grafo estrutural subjacente é conectado.",
        ),
        PropertyId::Sc => (
            "Strongly Connected",
            r"\forall x,y\in P\cup T:xF^*y",
            "Todo nó estrutural alcança todo outro nó.",
        ),
        PropertyId::Pur => (
            "Pure",
            r"\forall t:{}^\bullet t\cap t^\bullet=\varnothing",
            "Nenhum lugar é simultaneamente pré e pós-lugar da mesma transição.",
        ),
        PropertyId::Scf => (
            "Static Conflict Free",
            r"\forall p:|p^\bullet|\le 1",
            "Nenhum lugar possui mais de uma transição de saída.",
        ),
        PropertyId::Mg => (
            "Marked Graph",
            r"\forall p:|{}^\bullet p|=|p^\bullet|=1",
            "Cada lugar tem exatamente uma entrada e uma saída.",
        ),
        PropertyId::Fc => (
            "Free Choice",
            r"t,t'\in p^\bullet\Rightarrow{}^\bullet t={}^\bullet t'={p}",
            "Escolhas que compartilham lugar têm pré-conjunto unitário.",
        ),
        PropertyId::Efc => (
            "Extended Free Choice",
            r"{}^\bullet t_1\cap{}^\bullet t_2\ne\varnothing\Rightarrow{}^\bullet t_1={}^\bullet t_2",
            "Transições com pré-conjunto compartilhado têm pré-conjuntos iguais.",
        ),
        PropertyId::PInvariants => (
            "P-Invariants",
            r"y^TC=0",
            "Os vetores exibidos são P-semifluxos verificados pela matriz de incidência.",
        ),
        PropertyId::TInvariants => (
            "T-Invariants",
            r"Cx=0",
            "Os vetores exibidos são T-semifluxos verificados pela matriz de incidência.",
        ),
        PropertyId::Cpi => (
            "CPI",
            r"\bigcup_y supp(y)=P",
            "Todos os lugares são cobertos por P-semifluxos positivos.",
        ),
        PropertyId::Cti => (
            "CTI",
            r"\bigcup_x supp(x)=T",
            "Todas as transições são cobertas por T-semifluxos positivos.",
        ),
        PropertyId::Csv => (
            "CSV",
            r"\exists y>0:y^TC=0",
            "Existe um P-invariante estritamente positivo.",
        ),
        PropertyId::Sb => (
            "Structurally Bounded",
            r"\exists y\ge 0, y\ne0: C^Ty\le0",
            "A limitação estrutural é certificada por CPI/CSV ou pelo certificado de Murata.",
        ),
    };
    MathematicalDefinition {
        name: name.into(),
        formula_latex: formula.into(),
        explanation: explanation.into(),
    }
}

fn reference(method: EvidenceMethod) -> MathReference {
    MathReference {
        author: "Peter H. Starke".into(),
        title: "Analyse von Petri-Netz-Modellen".into(),
        year: None,
        page: None,
        definition: Some(format!("Método: {:?}", method)),
    }
}

fn proof_kind(result: Option<bool>, status: EvidenceStatus) -> ProofKind {
    match (result, status) {
        (Some(true), EvidenceStatus::Proven) => ProofKind::Proof,
        (Some(false), EvidenceStatus::Disproven) => ProofKind::Counterproof,
        (_, EvidenceStatus::Inconclusive) => ProofKind::Inconclusive,
        _ => ProofKind::AlgebraicCertificate,
    }
}

fn default_proof(
    property: PropertyId,
    result: Option<bool>,
    status: EvidenceStatus,
    certificate: Option<&Certificate>,
    counterexample: Option<&Certificate>,
) -> MathematicalCertificate {
    let selected = certificate.or(counterexample);
    let conclusion = match result {
        Some(true) => format!("{} = TRUE", definition(property).name),
        Some(false) => format!("{} = FALSE", definition(property).name),
        None => format!("{} permanece INCONCLUSIVO", definition(property).name),
    };
    MathematicalCertificate {
        kind: proof_kind(result, status),
        proposition: definition(property).formula_latex,
        assumptions: vec![
            "A conclusão é restrita ao espaço de estados informado pelo método.".into(),
        ],
        facts: selected
            .map(|item| {
                vec![ProofFact {
                    label: "Artefato verificado".into(),
                    value: item.summary(),
                    formula_latex: None,
                }]
            })
            .unwrap_or_default(),
        derivation: vec![ProofStep {
            text: match status {
                EvidenceStatus::Proven => {
                    "O certificado satisfaz a definição da propriedade.".into()
                }
                EvidenceStatus::Disproven => {
                    "O contraexemplo viola a definição da propriedade.".into()
                }
                EvidenceStatus::Inconclusive => {
                    "Os dados disponíveis não permitem concluir a propriedade.".into()
                }
            },
            formula_latex: None,
        }],
        conclusion,
        completeness: "A completude deve ser lida junto da proveniência do espaço analisado."
            .into(),
        tables: Vec::new(),
    }
}

fn evidence(
    property: PropertyId,
    _result: Option<bool>,
    method: EvidenceMethod,
    certificate: Option<Certificate>,
    counterexample: Option<Certificate>,
    metadata: EvidenceMetadata,
) -> MathematicalEvidence {
    let status = if certificate.is_some() {
        EvidenceStatus::Proven
    } else if counterexample.is_some() {
        EvidenceStatus::Disproven
    } else {
        EvidenceStatus::Inconclusive
    };
    MathematicalEvidence {
        property,
        sequence: None,
        result: _result,
        status,
        definition: definition(property),
        proof: default_proof(
            property,
            _result,
            status,
            certificate.as_ref(),
            counterexample.as_ref(),
        ),
        method: method.clone(),
        certificate,
        counterexample,
        assumptions: Vec::new(),
        references: vec![reference(method)],
        metadata,
    }
}

fn structural_meta() -> EvidenceMetadata {
    EvidenceMetadata {
        analyzer_version: "0.1.0".into(),
        algorithm: "Structural topology analysis".into(),
        exact_state_space: false,
        node_count: None,
        edge_count: None,
    }
}

fn structural_value(properties: &StructuralProperties, id: PropertyId) -> bool {
    match id {
        PropertyId::Ord => properties.is_ordinary,
        PropertyId::Hom => properties.is_homogeneous,
        PropertyId::Con => properties.is_connected,
        PropertyId::Pur => properties.is_pure,
        PropertyId::Scf => properties.is_static_conflict_free,
        PropertyId::Mg => properties.is_marked_graph,
        PropertyId::Fc => properties.is_free_choice,
        PropertyId::Efc => properties.is_extended_free_choice,
        _ => false,
    }
}

fn structural_details(net: &PetriNet, id: PropertyId, value: bool) -> StructuralCertificate {
    let mut details = Vec::new();
    let mut counterexample = None;
    match id {
        PropertyId::Ord => {
            let bad: Vec<_> = net
                .arcs()
                .iter()
                .filter(|a| a.weight != 1)
                .map(|a| format!("peso {}", a.weight))
                .collect();
            details.push(format!(
                "{} arcos; {} arcos com peso diferente de 1",
                net.arcs().len(),
                bad.len()
            ));
            counterexample = bad.first().cloned();
        }
        PropertyId::Pur => {
            for t in net.transitions() {
                let pre: HashSet<_> = net.preset(t.id).iter().map(|(p, _)| p.0).collect();
                let post: HashSet<_> = net.postset(t.id).iter().map(|(p, _)| p.0).collect();
                let overlap: Vec<_> = pre.intersection(&post).copied().collect();
                if !overlap.is_empty() {
                    let names = overlap
                        .iter()
                        .map(|id| place_name(net, *id as usize))
                        .collect::<Vec<_>>();
                    counterexample = Some(format!(
                        "{} intersection = {{{}}}",
                        t.name,
                        names.join(", ")
                    ));
                }
            }
            details.push(format!("{} transições verificadas", net.transition_count()));
        }
        PropertyId::Scf => {
            for p in net.places() {
                let outs = net
                    .transitions()
                    .iter()
                    .filter(|t| net.preset(t.id).iter().any(|(x, _)| *x == p.id))
                    .count();
                if outs > 1 {
                    counterexample = Some(format!("{} possui {} saídas", p.name, outs));
                }
            }
            details.push("grau de saída de todos os lugares verificado".into());
        }
        PropertyId::Mg => {
            for p in net.places() {
                let ins = net
                    .transitions()
                    .iter()
                    .filter(|t| net.postset(t.id).iter().any(|(x, _)| *x == p.id))
                    .count();
                let outs = net
                    .transitions()
                    .iter()
                    .filter(|t| net.preset(t.id).iter().any(|(x, _)| *x == p.id))
                    .count();
                details.push(format!("{}: in={} out={}", p.name, ins, outs));
                if ins != 1 || outs != 1 {
                    counterexample = Some(details.last().unwrap().clone());
                }
            }
        }
        _ => details.push(if value {
            "todos os elementos satisfazem o predicado".into()
        } else {
            "foi encontrado elemento que viola o predicado".into()
        }),
    }
    StructuralCertificate {
        property: id,
        details,
        counterexample,
    }
}

fn structural_strongly_connected(net: &PetriNet) -> bool {
    let n = net.place_count() + net.transition_count();
    if n == 0 {
        return true;
    }
    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for arc in net.arcs() {
        let (a, b) = match (arc.source, arc.target) {
            (ArcEndpoint::Place(p), ArcEndpoint::Transition(t)) => {
                (p.0 as usize, net.place_count() + t.0 as usize)
            }
            (ArcEndpoint::Transition(t), ArcEndpoint::Place(p)) => {
                (net.place_count() + t.0 as usize, p.0 as usize)
            }
            _ => continue,
        };
        adj[a].push(b);
        rev[b].push(a);
    }
    fn reach(start: usize, graph: &[Vec<usize>]) -> usize {
        let mut seen = vec![false; graph.len()];
        let mut q = VecDeque::from([start]);
        seen[start] = true;
        while let Some(v) = q.pop_front() {
            for &u in &graph[v] {
                if !seen[u] {
                    seen[u] = true;
                    q.push_back(u)
                }
            }
        }
        seen.into_iter().filter(|x| *x).count()
    }
    reach(0, &adj) == n && reach(0, &rev) == n
}

pub fn build_structural_evidence(
    net: &PetriNet,
    properties: &StructuralProperties,
) -> Vec<MathematicalEvidence> {
    let ids = [
        PropertyId::Ord,
        PropertyId::Hom,
        PropertyId::Con,
        PropertyId::Sc,
        PropertyId::Pur,
        PropertyId::Scf,
        PropertyId::Mg,
        PropertyId::Fc,
        PropertyId::Efc,
    ];
    ids.into_iter()
        .map(|id| {
            let value = if id == PropertyId::Sc {
                structural_strongly_connected(net)
            } else {
                structural_value(properties, id)
            };
            let cert = Certificate::Structural(structural_details(net, id, value));
            let (c, x) = if value {
                (Some(cert), None)
            } else {
                (None, Some(cert))
            };
            let mut item = evidence(
                id,
                Some(value),
                EvidenceMethod::StructuralTopology,
                c,
                x,
                structural_meta(),
            );
            let details = match item.certificate.as_ref().or(item.counterexample.as_ref()) {
                Some(Certificate::Structural(cert)) => cert.details.clone(),
                _ => Vec::new(),
            };
            let counterexample = item.counterexample.as_ref().and_then(|cert| match cert {
                Certificate::Structural(cert) => cert.counterexample.clone(),
                _ => None,
            });
            item.proof = MathematicalCertificate {
                kind: if value { ProofKind::Proof } else { ProofKind::Counterproof },
                proposition: item.definition.formula_latex.clone(),
                assumptions: vec!["A propriedade é estrutural e foi verificada diretamente na rede, sem usar o grafo de cobertura.".into()],
                facts: details.into_iter().map(|detail| ProofFact { label: "Verificação estrutural".into(), value: detail, formula_latex: None }).collect(),
                derivation: vec![ProofStep {
                    text: if value { "Todos os elementos relevantes satisfazem o predicado estrutural.".into() } else { "Foi encontrado um elemento que viola o predicado estrutural.".into() },
                    formula_latex: None,
                }],
                conclusion: format!("{} = {}", item.definition.name, if value { "TRUE" } else { "FALSE" }),
                completeness: "Todos os elementos da rede relevantes para esta definição foram verificados.".into(),
                tables: counterexample.map(|value| vec![ProofTable { title: "Contraexemplo estrutural".into(), columns: vec!["Fato".into()], rows: vec![vec![value]] }]).unwrap_or_default(),
            };
            item
        })
        .collect()
}

fn token_text(t: &TokenValue) -> String {
    match t {
        TokenValue::Finite(n) => n.to_string(),
        TokenValue::Exact(n) => n.to_string(),
        TokenValue::Omega => "ω".into(),
    }
}

fn place_name(net: &PetriNet, id: usize) -> String {
    net.places()
        .get(id)
        .map(|place| place.name.clone())
        .unwrap_or_else(|| format!("p{}", id))
}

fn transition_name(net: &PetriNet, id: usize) -> String {
    net.transitions()
        .get(id)
        .map(|transition| transition.name.clone())
        .unwrap_or_else(|| format!("t{}", id))
}

fn marking_text(m: &Marking) -> Vec<String> {
    m.tokens.iter().map(token_text).collect()
}
fn path_witness_between(graph: &ReachabilityGraph, start: usize, end: usize) -> FiringWitness {
    let nodes = graph.bfs_path(start, end).unwrap_or_else(|| vec![start]);
    let mut transitions = Vec::new();
    let mut markings = Vec::new();
    for &n in &nodes {
        markings.push(marking_text(&graph.nodes[n].marking));
    }
    for pair in nodes.windows(2) {
        transitions.push(
            graph.edge_adjacency[pair[0]]
                .iter()
                .find(|(target, _)| target.0 as usize == pair[1])
                .map(|(_, t)| t.0 as usize)
                .unwrap_or(usize::MAX),
        );
    }
    FiringWitness {
        start: nodes[0],
        transitions,
        markings,
        end: *nodes.last().unwrap(),
    }
}

fn path_witness(graph: &ReachabilityGraph, node: usize) -> FiringWitness {
    path_witness_between(graph, graph.initial_node.0 as usize, node)
}
fn graph_meta(graph: &ReachabilityGraph, algorithm: &str) -> EvidenceMetadata {
    EvidenceMetadata {
        analyzer_version: "0.1.0".into(),
        algorithm: algorithm.into(),
        exact_state_space: graph.is_exact(),
        node_count: Some(graph.node_count()),
        edge_count: Some(graph.edge_count()),
    }
}

pub fn bounded_evidence(
    graph: Option<&ReachabilityGraph>,
    coverability: Option<&CoverabilityGraph>,
) -> MathematicalEvidence {
    if let Some(g) = graph.filter(|g| g.is_exact()) {
        let bounds: Vec<String> = (0..g.net.place_count())
            .map(|p| {
                g.nodes
                    .iter()
                    .filter_map(|n| n.marking.tokens.get(p).and_then(|t| t.as_biguint()))
                    .max()
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "0".into())
            })
            .collect();
        let cert = Certificate::Bounded(BoundedCertificate {
            state_count: g.node_count(),
            place_bounds: bounds.clone(),
            exact: true,
        });
        let mut item = evidence(
            PropertyId::B,
            Some(true),
            EvidenceMethod::ReachabilityGraph,
            Some(cert),
            None,
            graph_meta(g, "Reachability BFS"),
        );
        item.proof = MathematicalCertificate {
            kind: ProofKind::Proof,
            proposition: item.definition.formula_latex.clone(),
            assumptions: vec!["O grafo de alcançabilidade é completo e exato.".into()],
            facts: vec![
                ProofFact { label: "Espaço analisado".into(), value: format!("{} estados", g.node_count()), formula_latex: Some(r"|R(M_0)|".into()) },
                ProofFact { label: "Limites observados por lugar".into(), value: bounds.join(", "), formula_latex: Some(r"k=(k_{p_1},\ldots,k_{p_n})".into()) },
            ],
            derivation: vec![
                ProofStep { text: "Em cada lugar foi tomado o maior número de tokens observado no grafo completo.".into(), formula_latex: Some(r"k_p=\max_{M\in R(M_0)}M(p)".into()) },
                ProofStep { text: "Nenhuma marcação enumerada excede o vetor de limites.".into(), formula_latex: Some(r"\forall M\in R(M_0),\ M\le k".into()) },
            ],
            conclusion: "B(N,M₀) = TRUE".into(),
            completeness: format!("Os {} estados alcançáveis foram enumerados.", g.node_count()),
            tables: vec![ProofTable {
                title: "Máximos observados por lugar".into(),
                columns: vec!["Lugar".into(), "Máximo observado".into(), "Limite kₚ".into()],
                rows: bounds.iter().enumerate().map(|(p, bound)| vec![place_name(&g.net, p), bound.clone(), bound.clone()]).collect(),
            }],
        };
        return item;
    }
    if let Some(c) = coverability {
        if let Some(node) = c.nodes.iter().find(|n| n.has_omega && n.is_active) {
            let cert = Certificate::Omega(OmegaCertificate {
                node: node.id.0 as usize,
                marking: marking_text(&node.marking),
                path: c
                    .get_path_to_root(node.id)
                    .into_iter()
                    .map(|n| n.0 as usize)
                    .collect(),
            });
            return evidence(
                PropertyId::B,
                Some(false),
                EvidenceMethod::KarpMillerCoverability,
                None,
                Some(cert.clone()),
                EvidenceMetadata {
                    analyzer_version: "0.1.0".into(),
                    algorithm: "Karp-Miller".into(),
                    exact_state_space: false,
                    node_count: Some(c.node_count()),
                    edge_count: Some(c.edge_count()),
                },
            );
        }
    }
    evidence(
        PropertyId::B,
        None,
        EvidenceMethod::ReachabilityGraph,
        None,
        None,
        EvidenceMetadata {
            analyzer_version: "0.1.0".into(),
            algorithm: "Espaço de estados não exato ou incompleto".into(),
            exact_state_space: false,
            node_count: None,
            edge_count: None,
        },
    )
}

pub fn build_behavioral_evidence(
    graph: Option<&ReachabilityGraph>,
    coverability: Option<&CoverabilityGraph>,
) -> Vec<MathematicalEvidence> {
    let mut out = vec![bounded_evidence(graph, coverability)];
    let Some(g) = graph.filter(|x| x.is_exact()) else {
        for id in [
            PropertyId::Dst,
            PropertyId::Dtr,
            PropertyId::Rev,
            PropertyId::L,
            PropertyId::Ls,
            PropertyId::Dcf,
        ] {
            out.push(evidence(
                id,
                None,
                EvidenceMethod::ReachabilityGraph,
                None,
                None,
                EvidenceMetadata {
                    analyzer_version: "0.1.0".into(),
                    algorithm: "Reachability Graph completo necessário".into(),
                    exact_state_space: false,
                    node_count: None,
                    edge_count: None,
                },
            ));
        }
        return out;
    };
    let terminal: Vec<_> = g
        .nodes
        .iter()
        .filter(|n| g.successors(n.id).is_empty())
        .collect();
    let dst_cert = terminal.first().map(|n| {
        Certificate::DeadState(DeadStateCertificate {
            node: n.id.0 as usize,
            marking: marking_text(&n.marking),
            path: path_witness(g, n.id.0 as usize).transitions,
            enabled_transitions: Vec::new(),
        })
    });
    let dst = dst_cert.is_some();
    let dst_certificate = if dst {
        dst_cert.clone()
    } else {
        Some(Certificate::Structural(StructuralCertificate {
            property: PropertyId::Dst,
            details: vec![format!(
                "RG completo: {}; estados terminais: 0",
                g.node_count()
            )],
            counterexample: None,
        }))
    };
    let mut dst_evidence = evidence(
        PropertyId::Dst,
        Some(dst),
        EvidenceMethod::ReachabilityGraph,
        dst_certificate,
        None,
        graph_meta(g, "Reachability BFS"),
    );
    let dst_rows = g
        .nodes
        .iter()
        .map(|node| {
            let enabled: Vec<_> = (0..g.net.transition_count())
                .filter(|t| g.net.is_enabled(TransitionId(*t as u32), &node.marking))
                .map(|t| transition_name(&g.net, t))
                .collect();
            vec![
                format!("M{}", node.id.0),
                marking_text(&node.marking).join(", "),
                if enabled.is_empty() {
                    "∅".into()
                } else {
                    enabled.join(", ")
                },
            ]
        })
        .collect();
    dst_evidence.proof = MathematicalCertificate {
        kind: if dst {
            ProofKind::Proof
        } else {
            ProofKind::Counterproof
        },
        proposition: dst_evidence.definition.formula_latex.clone(),
        assumptions: vec!["O grafo de alcançabilidade é completo e exato.".into()],
        facts: vec![ProofFact {
            label: "Estados terminais".into(),
            value: terminal.len().to_string(),
            formula_latex: Some(r"|\{M\in R(M_0)\mid Enabled(M)=\varnothing\}|".into()),
        }],
        derivation: if dst {
            vec![ProofStep {
                text: "Foi encontrada uma marcação alcançável sem transições habilitadas.".into(),
                formula_latex: Some(r"\exists M\in R(M_0):Enabled(M)=\varnothing".into()),
            }]
        } else {
            vec![ProofStep {
                text: "Cada marcação enumerada possui pelo menos uma transição habilitada.".into(),
                formula_latex: Some(r"\forall M\in R(M_0):Enabled(M)\ne\varnothing".into()),
            }]
        },
        conclusion: format!("DSt = {}", if dst { "TRUE" } else { "FALSE" }),
        completeness: format!(
            "{} marcações foram enumeradas e verificadas.",
            g.node_count()
        ),
        tables: vec![ProofTable {
            title: "Marcações e transições habilitadas".into(),
            columns: vec![
                "Marcação".into(),
                "Vetor".into(),
                "Transições habilitadas".into(),
            ],
            rows: dst_rows,
        }],
    };
    out.push(dst_evidence);
    let mut dead = Vec::new();
    for t in 0..g.net.transition_count() {
        let enabled: Vec<_> = g
            .nodes
            .iter()
            .filter(|n| g.net.is_enabled(TransitionId(t as u32), &n.marking))
            .map(|n| n.id.0 as usize)
            .collect();
        if enabled.is_empty() {
            dead.push((t, enabled));
        }
    }
    let dtr = !dead.is_empty();
    let dc = dead.first().map(|(t, e)| {
        Certificate::DeadTransition(DeadTransitionCertificate {
            transition: *t,
            enabled_nodes: e.clone(),
            state_count: g.node_count(),
        })
    });
    let dtr_certificate = if dtr {
        None
    } else {
        Some(Certificate::Structural(StructuralCertificate {
            property: PropertyId::Dtr,
            details: vec![format!(
                "RG completo: cada transição possui marcação habilitadora; {} estados",
                g.node_count()
            )],
            counterexample: None,
        }))
    };
    let mut dtr_evidence = evidence(
        PropertyId::Dtr,
        Some(dtr),
        EvidenceMethod::ReachabilityGraph,
        dtr_certificate,
        dc,
        graph_meta(g, "Reachability BFS"),
    );
    let dtr_rows: Vec<_> = (0..g.net.transition_count())
        .map(|t| {
            let enabled_nodes: Vec<_> = g
                .nodes
                .iter()
                .filter(|n| g.net.is_enabled(TransitionId(t as u32), &n.marking))
                .map(|n| format!("M{}", n.id.0))
                .collect();
            vec![
                transition_name(&g.net, t),
                if enabled_nodes.is_empty() {
                    "—".into()
                } else {
                    enabled_nodes.join(", ")
                },
                if enabled_nodes.is_empty() {
                    "✗".into()
                } else {
                    "✓".into()
                },
            ]
        })
        .collect();
    dtr_evidence.proof = MathematicalCertificate {
        kind: if dtr {
            ProofKind::Proof
        } else {
            ProofKind::Counterproof
        },
        proposition: dtr_evidence.definition.formula_latex.clone(),
        assumptions: vec!["O grafo de alcançabilidade é completo e exato.".into()],
        facts: vec![ProofFact {
            label: "Transições verificadas".into(),
            value: g.net.transition_count().to_string(),
            formula_latex: Some(r"|T|".into()),
        }],
        derivation: vec![ProofStep {
            text: if dtr {
                "Existe uma transição sem qualquer marcação habilitadora.".into()
            } else {
                "Cada transição possui ao menos uma marcação alcançável que a habilita.".into()
            },
            formula_latex: Some(if dtr {
                r"\exists t\in T,\ \forall M\in R(M_0):\neg M[t\rangle".into()
            } else {
                r"\forall t\in T,\ \exists M\in R(M_0):M[t\rangle".into()
            }),
        }],
        conclusion: format!("DTr = {}", if dtr { "TRUE" } else { "FALSE" }),
        completeness: format!(
            "Todas as {} transições foram verificadas contra as {} marcações.",
            g.net.transition_count(),
            g.node_count()
        ),
        tables: vec![ProofTable {
            title: "Witness por transição".into(),
            columns: vec![
                "Transição".into(),
                "Marcações habilitadoras".into(),
                "Habilitada".into(),
            ],
            rows: dtr_rows,
        }],
    };
    out.push(dtr_evidence);
    let initial = g.initial_node;
    let mut nonrev = None;
    for n in &g.nodes {
        if g.bfs_path(n.id.0 as usize, initial.0 as usize).is_none() {
            nonrev = Some(n.id.0 as usize);
            break;
        }
    }
    let rev = nonrev.is_none();
    let rc = Certificate::Reversibility(ReversibilityCertificate {
        state_count: g.node_count(),
        initial_node: initial.0 as usize,
        non_reversible_node: nonrev,
        path: nonrev.map(|n| path_witness(g, n)),
        return_paths: if rev {
            g.nodes
                .iter()
                .map(|node| path_witness_between(g, node.id.0 as usize, initial.0 as usize))
                .collect()
        } else {
            Vec::new()
        },
    });
    let mut reversibility = evidence(
        PropertyId::Rev,
        Some(rev),
        EvidenceMethod::ReachabilityGraph,
        if rev { Some(rc.clone()) } else { None },
        if rev { None } else { Some(rc) },
        graph_meta(g, "Reachability BFS"),
    );
    let returning = g
        .nodes
        .iter()
        .filter(|node| g.bfs_path(node.id.0 as usize, initial.0 as usize).is_some())
        .count();
    reversibility.proof = MathematicalCertificate {
        kind: if rev {
            ProofKind::Proof
        } else {
            ProofKind::Counterproof
        },
        proposition: reversibility.definition.formula_latex.clone(),
        assumptions: vec!["O grafo de alcançabilidade é completo e exato.".into()],
        facts: vec![
            ProofFact {
                label: "Estados alcançáveis".into(),
                value: g.node_count().to_string(),
                formula_latex: Some(r"|R(M_0)|".into()),
            },
            ProofFact {
                label: "Estados que alcançam M₀".into(),
                value: returning.to_string(),
                formula_latex: Some(r"|\{M\in R(M_0)\mid M\rightarrow^*M_0\}|".into()),
            },
            ProofFact {
                label: "Estados sem caminho de retorno".into(),
                value: (g.node_count() - returning).to_string(),
                formula_latex: None,
            },
        ],
        derivation: if rev {
            vec![ProofStep {
                text: "Todos os estados alcançáveis possuem caminho de retorno para M₀.".into(),
                formula_latex: Some(r"\forall M\in R(M_0):M\rightarrow^*M_0".into()),
            }]
        } else {
            vec![ProofStep {
                text: format!(
                    "O estado M{} é alcançável, mas não possui caminho de retorno para M₀.",
                    nonrev.unwrap()
                ),
                formula_latex: Some(r"\exists M\in R(M_0):M\not\rightarrow^*M_0".into()),
            }]
        },
        conclusion: format!("REV = {}", if rev { "TRUE" } else { "FALSE" }),
        completeness: format!(
            "{}/{} estados foram verificados no grafo completo.",
            g.node_count(),
            g.node_count()
        ),
        tables: if rev {
            vec![ProofTable {
                title: "Caminhos de retorno para M₀".into(),
                columns: vec!["Estado".into(), "Sequência de disparos".into()],
                rows: g
                    .nodes
                    .iter()
                    .map(|node| {
                        let witness =
                            path_witness_between(g, node.id.0 as usize, initial.0 as usize);
                        vec![
                            format!("M{}", node.id.0),
                            if witness.transitions.is_empty() {
                                "ε".into()
                            } else {
                                witness
                                    .transitions
                                    .iter()
                                    .map(|t| transition_name(&g.net, *t))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            },
                        ]
                    })
                    .collect(),
            }]
        } else {
            Vec::new()
        },
    };
    out.push(reversibility);
    let mut liveness = Vec::new();
    for t in 0..g.net.transition_count() {
        let enabled: HashSet<_> = g
            .nodes
            .iter()
            .filter(|n| g.net.is_enabled(TransitionId(t as u32), &n.marking))
            .map(|n| n.id)
            .collect();
        let mut closure = enabled.clone();
        let mut q: VecDeque<_> = enabled.iter().copied().collect();
        while let Some(n) = q.pop_front() {
            for &p in g.predecessors(n) {
                if closure.insert(p) {
                    q.push_back(p)
                }
            }
        }
        liveness.push((t, enabled, closure));
    }
    let live = liveness.iter().all(|(_, _, c)| c.contains(&initial));
    let bad = liveness.iter().find(|(_, _, c)| !c.contains(&initial));
    let per_transition: Vec<_> = liveness
        .iter()
        .map(|(t, enabled, closure)| LivenessTransitionCertificate {
            transition: *t,
            enabled_nodes: enabled.iter().map(|n| n.0 as usize).collect(),
            predecessor_closure_size: closure.len(),
            state_space_size: g.node_count(),
            holds: closure.contains(&initial),
        })
        .collect();
    let lc = bad.map(|(t, e, c)| {
        Certificate::Liveness(LivenessCertificate {
            transition: *t,
            enabled_nodes: e.iter().map(|n| n.0 as usize).collect(),
            predecessor_closure_size: c.len(),
            state_space_size: g.node_count(),
            counterexample_node: None,
            per_transition: per_transition.clone(),
        })
    });
    let good = if live {
        Some(Certificate::Liveness(LivenessCertificate {
            transition: 0,
            enabled_nodes: liveness
                .first()
                .map(|x| x.1.iter().map(|n| n.0 as usize).collect())
                .unwrap_or_default(),
            predecessor_closure_size: g.node_count(),
            state_space_size: g.node_count(),
            counterexample_node: None,
            per_transition: per_transition.clone(),
        }))
    } else {
        None
    };
    let mut liveness_evidence = evidence(
        PropertyId::L,
        Some(live),
        EvidenceMethod::ReachabilityGraph,
        good,
        lc,
        graph_meta(g, "Reachability BFS"),
    );
    liveness_evidence.proof = MathematicalCertificate {
        kind: if live {
            ProofKind::Proof
        } else {
            ProofKind::Counterproof
        },
        proposition: liveness_evidence.definition.formula_latex.clone(),
        assumptions: vec!["O grafo de alcançabilidade é completo e exato.".into()],
        facts: vec![ProofFact {
            label: "Estados alcançáveis".into(),
            value: g.node_count().to_string(),
            formula_latex: Some(r"|R(M_0)|".into()),
        }],
        derivation: vec![ProofStep {
            text: if live {
                "Para cada transição, o fecho de predecessores dos estados habilitadores cobre todos os estados alcançáveis.".into()
            } else {
                "Foi encontrada uma transição cujo fecho de predecessores não cobre todos os estados alcançáveis.".into()
            },
            formula_latex: Some(r"\forall t\in T:Pred^*(E_t)=R(M_0)".into()),
        }],
        conclusion: format!("L = {}", if live { "TRUE" } else { "FALSE" }),
        completeness: format!(
            "{} transições e {} estados foram verificados.",
            g.net.transition_count(),
            g.node_count()
        ),
        tables: vec![ProofTable {
            title: "Fechos de predecessores por transição".into(),
            columns: vec![
                "Transição".into(),
                "Estados habilitadores".into(),
                "|Pred*(Eₜ)|".into(),
                "|R|".into(),
                "Resultado".into(),
            ],
            rows: per_transition
                .iter()
                .map(|entry| {
                    vec![
                        transition_name(&g.net, entry.transition),
                        entry
                            .enabled_nodes
                            .iter()
                            .map(|n| format!("M{}", n))
                            .collect::<Vec<_>>()
                            .join(", "),
                        entry.predecessor_closure_size.to_string(),
                        entry.state_space_size.to_string(),
                        if entry.holds {
                            "✓".into()
                        } else {
                            "✗".into()
                        },
                    ]
                })
                .collect(),
        }],
    };
    out.push(liveness_evidence);
    let safe = g.nodes.iter().all(|n| {
        n.marking
            .tokens
            .iter()
            .all(|t| t.as_biguint().map(|x| x <= 1u32.into()).unwrap_or(false))
    });
    let ls = live && safe;
    let lsc = Certificate::Structural(StructuralCertificate {
        property: PropertyId::Ls,
        details: vec![format!("Liveness: {}", live), format!("Safe: {}", safe)],
        counterexample: None,
    });
    out.push(evidence(
        PropertyId::Ls,
        Some(ls),
        EvidenceMethod::ReachabilityGraph,
        if ls { Some(lsc.clone()) } else { None },
        if ls { None } else { Some(lsc) },
        graph_meta(g, "Reachability BFS"),
    ));
    let mut conflicts = Vec::new();
    let mut pairs = 0;
    for n in &g.nodes {
        let enabled: Vec<_> = (0..g.net.transition_count())
            .filter(|t| g.net.is_enabled(TransitionId(*t as u32), &n.marking))
            .collect();
        for (i, &a) in enabled.iter().enumerate() {
            for &b in enabled.iter().skip(i + 1) {
                pairs += 1;
                let shared = g.net.preset(TransitionId(a as u32)).iter().any(|(p, _)| {
                    g.net
                        .preset(TransitionId(b as u32))
                        .iter()
                        .any(|(q, _)| p == q)
                });
                if shared {
                    let after = g.net.fire(TransitionId(a as u32), &n.marking);
                    if !g.net.is_enabled(TransitionId(b as u32), &after) {
                        conflicts.push((n.id.0 as usize, a, b));
                    }
                }
            }
        }
    }
    let cc = Certificate::DynamicConflictFree(ConflictCertificate {
        state_count: g.node_count(),
        enabled_pairs: pairs,
        conflicts: conflicts.clone(),
    });
    out.push(evidence(
        PropertyId::Dcf,
        Some(conflicts.is_empty()),
        EvidenceMethod::ReachabilityGraph,
        if conflicts.is_empty() {
            Some(cc.clone())
        } else {
            None
        },
        if conflicts.is_empty() { None } else { Some(cc) },
        graph_meta(g, "Reachability BFS"),
    ));
    out
}

fn invariant_meta() -> EvidenceMetadata {
    EvidenceMetadata {
        analyzer_version: "0.1.0".into(),
        algorithm: "Incidence matrix exact integer arithmetic".into(),
        exact_state_space: false,
        node_count: None,
        edge_count: None,
    }
}
pub fn build_invariant_evidence(
    net: &PetriNet,
    result: &InvariantAnalysisResult,
) -> Vec<MathematicalEvidence> {
    let mut out = Vec::new();
    let pcerts: Vec<_> = result
        .p_semiflows
        .iter()
        .map(|inv| {
            let product = net.incidence_matrix().left_multiply(&inv.coefficients);
            Certificate::Invariant(InvariantCertificate {
                coefficients: inv.coefficients.iter().map(ToString::to_string).collect(),
                product: product.iter().map(ToString::to_string).collect(),
                constant: inv.constant_value.as_ref().map(ToString::to_string),
                support: inv.support.iter().map(|x| x.0 as usize).collect(),
                realizability: None,
                realization_witness: None,
            })
        })
        .collect();
    let p_ok = !pcerts.is_empty() && result.p_semiflows.iter().all(|i| i.is_semiflow);
    if p_ok {
        for (index, certificate) in pcerts.iter().cloned().enumerate() {
            let mut item = evidence(
                PropertyId::PInvariants,
                Some(true),
                EvidenceMethod::IncidenceAlgebra,
                Some(certificate.clone()),
                None,
                invariant_meta(),
            );
            if let Certificate::Invariant(cert) = &certificate {
                item.proof = MathematicalCertificate {
                    kind: ProofKind::AlgebraicCertificate,
                    proposition: item.definition.formula_latex.clone(),
                    assumptions: vec!["A matriz de incidência C foi calculada com aritmética inteira exata.".into()],
                    facts: vec![
                        ProofFact { label: "Vetor y".into(), value: format!("[{}]", cert.coefficients.join(", ")), formula_latex: Some("y".into()) },
                        ProofFact { label: "Produto yᵀC".into(), value: format!("[{}]", cert.product.join(", ")), formula_latex: Some(r"y^TC".into()) },
                    ],
                    derivation: vec![ProofStep { text: "A multiplicação do vetor pelo lado esquerdo da matriz de incidência produz o vetor nulo.".into(), formula_latex: Some(r"y^TC=[0,\ldots,0]".into()) }],
                    conclusion: "yᵀC = 0; o vetor é um P-semifluxo verificado".into(),
                    completeness: "O produto foi verificado em todas as colunas da matriz de incidência.".into(),
                    tables: Vec::new(),
                };
            }
            item.sequence = Some(index);
            out.push(item);
        }
    } else {
        out.push(evidence(
            PropertyId::PInvariants,
            None,
            EvidenceMethod::IncidenceAlgebra,
            None,
            None,
            invariant_meta(),
        ));
    }
    let tcerts: Vec<_> = result
        .t_semiflows
        .iter()
        .map(|inv| {
            let product = net.incidence_matrix().right_multiply(&inv.coefficients);
            Certificate::Invariant(InvariantCertificate {
                coefficients: inv.coefficients.iter().map(ToString::to_string).collect(),
                product: product.iter().map(ToString::to_string).collect(),
                constant: None,
                support: inv.support.iter().map(|x| x.0 as usize).collect(),
                realizability: inv.is_realizable,
                realization_witness: inv
                    .realization_witness
                    .as_ref()
                    .map(|sequence| sequence.iter().map(|x| x.0 as usize).collect()),
            })
        })
        .collect();
    let t_ok = !tcerts.is_empty() && result.t_semiflows.iter().all(|i| i.is_semiflow);
    if t_ok {
        for (index, certificate) in tcerts.iter().cloned().enumerate() {
            let mut item = evidence(
                PropertyId::TInvariants,
                Some(true),
                EvidenceMethod::IncidenceAlgebra,
                Some(certificate.clone()),
                None,
                invariant_meta(),
            );
            if let Certificate::Invariant(cert) = &certificate {
                let realization = match cert.realizability {
                    Some(true) => format!(
                        "SIM; sequência: {}",
                        cert.realization_witness
                            .as_ref()
                            .map(|w| w
                                .iter()
                                .map(|t| transition_name(net, *t))
                                .collect::<Vec<_>>()
                                .join(" "))
                            .unwrap_or_else(|| "não informada".into())
                    ),
                    Some(false) => "NÃO".into(),
                    None => "INCONCLUSIVA".into(),
                };
                item.proof = MathematicalCertificate {
                    kind: ProofKind::AlgebraicCertificate,
                    proposition: item.definition.formula_latex.clone(),
                    assumptions: vec!["A matriz de incidência C foi calculada com aritmética inteira exata.".into()],
                    facts: vec![
                        ProofFact { label: "Vetor x".into(), value: format!("[{}]", cert.coefficients.join(", ")), formula_latex: Some("x".into()) },
                        ProofFact { label: "Produto Cx".into(), value: format!("[{}]", cert.product.join(", ")), formula_latex: Some("Cx".into()) },
                        ProofFact { label: "Realizabilidade".into(), value: realization, formula_latex: None },
                    ],
                    derivation: vec![ProofStep { text: "A multiplicação da matriz pelo vetor de disparos produz o vetor nulo.".into(), formula_latex: Some(r"Cx=[0,\ldots,0]".into()) }],
                    conclusion: "Cx = 0; o vetor é um T-semifluxo verificado".into(),
                    completeness: "O produto foi verificado em todas as linhas da matriz de incidência; a realizabilidade é reportada separadamente.".into(),
                    tables: Vec::new(),
                };
            }
            item.sequence = Some(index);
            out.push(item);
        }
    } else {
        out.push(evidence(
            PropertyId::TInvariants,
            None,
            EvidenceMethod::IncidenceAlgebra,
            None,
            None,
            invariant_meta(),
        ));
    }
    let pcovered: Vec<_> = result
        .p_semiflows
        .iter()
        .flat_map(|i| i.support.iter().map(|p| p.0 as usize))
        .collect();
    let puniq: HashSet<_> = pcovered.iter().copied().collect();
    let pu: Vec<_> = (0..net.place_count())
        .filter(|p| !puniq.contains(p))
        .collect();
    let cpi = pu.is_empty() && net.place_count() > 0;
    let cov = Certificate::Coverage(CoverageCertificate {
        covered: puniq.iter().copied().collect(),
        uncovered: pu,
        strictly_positive: false,
    });
    out.push(evidence(
        PropertyId::Cpi,
        Some(cpi),
        EvidenceMethod::IncidenceAlgebra,
        if cpi { Some(cov.clone()) } else { None },
        if cpi { None } else { Some(cov) },
        invariant_meta(),
    ));
    let tcovered: HashSet<_> = result
        .t_semiflows
        .iter()
        .flat_map(|i| i.support.iter().map(|t| t.0 as usize))
        .collect();
    let tu: Vec<_> = (0..net.transition_count())
        .filter(|t| !tcovered.contains(t))
        .collect();
    let cti = tu.is_empty() && net.transition_count() > 0;
    let tcov = Certificate::Coverage(CoverageCertificate {
        covered: tcovered.iter().copied().collect(),
        uncovered: tu,
        strictly_positive: false,
    });
    out.push(evidence(
        PropertyId::Cti,
        Some(cti),
        EvidenceMethod::IncidenceAlgebra,
        if cti { Some(tcov.clone()) } else { None },
        if cti { None } else { Some(tcov) },
        invariant_meta(),
    ));
    let csv = result.is_conservative;
    let csv_cert=pcerts.iter().find(|c|matches!(c,Certificate::Invariant(InvariantCertificate{coefficients,..}) if coefficients.iter().all(|x|x!="0"))).cloned();
    let csv_result = if csv { Some(true) } else { None };
    out.push(evidence(
        PropertyId::Csv,
        csv_result,
        EvidenceMethod::IncidenceAlgebra,
        csv_cert.clone(),
        if csv { None } else { csv_cert },
        invariant_meta(),
    ));
    let sb = result.is_cpi
        || result.is_conservative
        || result.structural_bounded_by_inequality.is_some();
    let sbcert = if result.structural_bounded_by_inequality.is_some() {
        Some(Certificate::Invariant(InvariantCertificate {
            coefficients: result
                .structural_bounded_by_inequality
                .as_ref()
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect(),
            product: Vec::new(),
            constant: None,
            support: Vec::new(),
            realizability: None,
            realization_witness: None,
        }))
    } else if result.is_cpi {
        Some(Certificate::Coverage(CoverageCertificate {
            covered: (0..net.place_count()).collect(),
            uncovered: Vec::new(),
            strictly_positive: false,
        }))
    } else {
        None
    };
    let sb_result = if sb { Some(true) } else { None };
    out.push(evidence(
        PropertyId::Sb,
        sb_result,
        EvidenceMethod::IncidenceAlgebra,
        sbcert.clone(),
        if sb { None } else { None },
        invariant_meta(),
    ));
    out
}

pub fn build_all_evidence(
    net: &PetriNet,
    structural: &StructuralProperties,
    invariants: &InvariantAnalysisResult,
    graph: Option<&ReachabilityGraph>,
    coverability: Option<&CoverabilityGraph>,
) -> Vec<MathematicalEvidence> {
    let mut e = build_behavioral_evidence(graph, coverability);
    e.extend(build_structural_evidence(net, structural));
    e.extend(build_invariant_evidence(net, invariants));
    e
}
