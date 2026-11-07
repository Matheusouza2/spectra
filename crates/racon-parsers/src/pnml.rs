use quick_xml::events::Event;
use quick_xml::Reader;
use racon_core::{ArcEndpoint, PetriNet, PetriNetBuilder, RaconError, TokenValue};
use std::collections::HashMap;
use std::path::Path;

/// Configuration for PNML parsing.
#[derive(Debug, Clone)]
pub struct PnmlParserConfig {
    /// Validate mathematical consistency of the resulting net.
    pub validate_math: bool,
    /// Ignore graphical information in PNML (V1: always true).
    pub ignore_graphics: bool,
    /// Generate numeric IDs for unnamed elements.
    pub generate_ids: bool,
    /// Maximum allowed arc weight.
    pub max_arc_weight: u32,
    /// Maximum allowed initial marking on any place.
    pub max_initial_marking: u64,
}

impl Default for PnmlParserConfig {
    fn default() -> Self {
        Self {
            validate_math: true,
            ignore_graphics: true,
            generate_ids: true,
            max_arc_weight: 1_000_000,
            max_initial_marking: 1_000_000_000,
        }
    }
}

/// Trait for PNML format parsers.
pub trait PnmlParser {
    /// Parse PNML XML from a string.
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError>;

    /// Parse PNML XML from a file path.
    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError>;

    /// Parse PNML XML from any std::io::BufRead source.
    fn parse_reader<R: std::io::BufRead>(&self, reader: R) -> Result<PetriNet, RaconError>;
}

/// Default PNML parser implementation using quick-xml.
pub struct PnmlParserImpl {
    config: PnmlParserConfig,
}

impl PnmlParserImpl {
    pub fn new(config: PnmlParserConfig) -> Self {
        Self { config }
    }
}

/// Intermediate PNML document representation.
struct PnmlDocument {
    places: Vec<PnmlPlace>,
    transitions: Vec<PnmlTransition>,
    arcs: Vec<PnmlArc>,
}

struct PnmlPlace {
    id: String,
    name: Option<String>,
    initial_marking: u64,
}

struct PnmlTransition {
    id: String,
    name: Option<String>,
}

struct PnmlArc {
    source: String,
    target: String,
    weight: u32,
}

impl PnmlParser for PnmlParserImpl {
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError> {
        self.parse_reader(input.as_bytes())
    }

    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError> {
        let content = std::fs::read_to_string(path).map_err(|e| RaconError::Io(e))?;
        self.parse_string(&content)
    }

    fn parse_reader<R: std::io::BufRead>(&self, reader: R) -> Result<PetriNet, RaconError> {
        let mut xml_reader = Reader::from_reader(reader);

        let mut doc = PnmlDocument {
            places: Vec::new(),
            transitions: Vec::new(),
            arcs: Vec::new(),
        };

        let mut buf = Vec::new();
        let mut current_place: Option<PnmlPlace> = None;
        let mut current_transition: Option<PnmlTransition> = None;
        let mut current_arc: Option<PnmlArc> = None;
        let mut in_name = false;
        let mut in_marking = false;
        let mut in_inscription = false;

        loop {
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"place" => {
                        let id = self.read_attr(e, b"id")?;
                        current_place = Some(PnmlPlace {
                            id,
                            name: None,
                            initial_marking: 0,
                        });
                    }
                    b"transition" => {
                        let id = self.read_attr(e, b"id")?;
                        current_transition = Some(PnmlTransition { id, name: None });
                    }
                    b"arc" => {
                        let source = self.read_attr(e, b"source")?;
                        let target = self.read_attr(e, b"target")?;
                        current_arc = Some(PnmlArc {
                            source,
                            target,
                            weight: 1,
                        });
                    }
                    b"name" => {
                        in_name = true;
                    }
                    b"initialMarking" => {
                        in_marking = true;
                    }
                    b"inscription" => {
                        in_inscription = true;
                    }
                    _ => {}
                },
                Ok(Event::Text(ref e)) => {
                    let text = e.unescape().unwrap_or_default();
                    if in_name {
                        if let Some(ref mut p) = current_place {
                            p.name = Some(text.to_string());
                        }
                        if let Some(ref mut t) = current_transition {
                            t.name = Some(text.to_string());
                        }
                    } else if in_marking {
                        if let Some(ref mut p) = current_place {
                            let v: u64 = text.parse().map_err(|_| RaconError::ParseError {
                                format: "pnml".into(),
                                reason: format!("Invalid initial marking: {}", text),
                                line: None,
                                col: None,
                            })?;
                            if v > self.config.max_initial_marking {
                                return Err(RaconError::ParseError {
                                    format: "pnml".into(),
                                    reason: format!(
                                        "Initial marking {} exceeds max {}",
                                        v, self.config.max_initial_marking
                                    ),
                                    line: None,
                                    col: None,
                                });
                            }
                            p.initial_marking = v;
                        }
                    } else if in_inscription {
                        if let Some(ref mut a) = current_arc {
                            let w: u32 = text.parse().map_err(|_| RaconError::ParseError {
                                format: "pnml".into(),
                                reason: format!("Invalid arc weight: {}", text),
                                line: None,
                                col: None,
                            })?;
                            if w == 0 || w > self.config.max_arc_weight {
                                return Err(RaconError::ParseError {
                                    format: "pnml".into(),
                                    reason: format!(
                                        "Arc weight {} out of range (1..{})",
                                        w, self.config.max_arc_weight
                                    ),
                                    line: None,
                                    col: None,
                                });
                            }
                            a.weight = w;
                        }
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"place" => {
                        if let Some(p) = current_place.take() {
                            doc.places.push(p);
                        }
                    }
                    b"transition" => {
                        if let Some(t) = current_transition.take() {
                            doc.transitions.push(t);
                        }
                    }
                    b"arc" => {
                        if let Some(a) = current_arc.take() {
                            doc.arcs.push(a);
                        }
                    }
                    b"name" => {
                        in_name = false;
                    }
                    b"initialMarking" => {
                        in_marking = false;
                    }
                    b"inscription" => {
                        in_inscription = false;
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(RaconError::ParseError {
                        format: "pnml".into(),
                        reason: format!("XML error: {}", e),
                        line: None,
                        col: None,
                    });
                }
                _ => {}
            }
            buf.clear();
        }

        self.build_petri_net(doc)
    }
}

impl PnmlParserImpl {
    fn read_attr<'a>(
        &self,
        e: &quick_xml::events::BytesStart<'a>,
        attr_name: &[u8],
    ) -> Result<String, RaconError> {
        e.attributes()
            .filter_map(|a| a.ok())
            .find(|a| a.key.as_ref() == attr_name)
            .map(|a| String::from_utf8_lossy(&a.value).to_string())
            .ok_or_else(|| RaconError::ParseError {
                format: "pnml".into(),
                reason: format!(
                    "Missing required attribute: {}",
                    String::from_utf8_lossy(attr_name)
                ),
                line: None,
                col: None,
            })
    }

    fn build_petri_net(&self, doc: PnmlDocument) -> Result<PetriNet, RaconError> {
        let mut builder = PetriNetBuilder::default();
        let mut place_id_map: HashMap<String, racon_core::PlaceId> = HashMap::new();
        let mut trans_id_map: HashMap<String, racon_core::TransitionId> = HashMap::new();

        // Build places
        for p in &doc.places {
            let name = p.name.clone().unwrap_or_else(|| p.id.clone());
            let pid = builder.add_place(name, None);
            place_id_map.insert(p.id.clone(), pid);
        }

        // Build transitions
        for t in &doc.transitions {
            let name = t.name.clone().unwrap_or_else(|| t.id.clone());
            let tid = builder.add_transition(name, None);
            trans_id_map.insert(t.id.clone(), tid);
        }

        // Build initial marking (order must match place ID order)
        let place_count = doc.places.len();
        let mut initial_tokens = vec![TokenValue::Finite(0); place_count];
        for p in &doc.places {
            if let Some(&pid) = place_id_map.get(&p.id) {
                initial_tokens[pid.0 as usize] = TokenValue::Finite(p.initial_marking);
            }
        }

        // Build arcs
        for a in &doc.arcs {
            let weight = a.weight;
            match (
                place_id_map.get(&a.source),
                trans_id_map.get(&a.target),
                place_id_map.get(&a.target),
                trans_id_map.get(&a.source),
            ) {
                // Place -> Transition
                (Some(&p), Some(&t), _, _) => {
                    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), weight);
                }
                // Transition -> Place
                (_, _, Some(&p), Some(&t)) => {
                    builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), weight);
                }
                _ => {
                    return Err(RaconError::ParseError {
                        format: "pnml".into(),
                        reason: format!("Invalid arc endpoints: {} -> {}", a.source, a.target),
                        line: None,
                        col: None,
                    });
                }
            }
        }

        builder.build(initial_tokens)
    }
}
