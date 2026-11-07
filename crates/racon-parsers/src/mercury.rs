use racon_core::{
    ArcEndpoint, PetriNet, PetriNetBuilder, RaconError, TokenValue, TransitionKind,
};
use racon_gspn::{GspnSpec, TransitionType};
use std::collections::HashMap;
use std::path::Path;

/// Result from parsing MercuryScript, optionally including GSPN data.
#[derive(Debug, Clone)]
pub struct MercuryParseResult {
    pub net: PetriNet,
    pub gspn_spec: Option<GspnSpec>,
}

/// Configuration for MercuryScript parsing.
#[derive(Debug, Clone)]
pub struct MercuryParserConfig {
    pub validate_math: bool,
    pub max_arc_weight: u32,
    pub max_initial_marking: u64,
}

impl Default for MercuryParserConfig {
    fn default() -> Self {
        Self {
            validate_math: true,
            max_arc_weight: 1_000_000,
            max_initial_marking: 1_000_000_000,
        }
    }
}

/// Trait for MercuryScript format parsers.
pub trait MercuryParser {
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError>;
    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError>;
}

/// Default MercuryScript parser implementation.
/// V1: extracts places, transitions, and arcs. Ignores timing/delay info.
pub struct MercuryParserImpl {
    config: MercuryParserConfig,
}

impl MercuryParserImpl {
    pub fn new(config: MercuryParserConfig) -> Self {
        Self { config }
    }
}

impl MercuryParser for MercuryParserImpl {
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError> {
        let mut builder = PetriNetBuilder::default();
        let mut place_map: HashMap<String, racon_core::PlaceId> = HashMap::new();
        let mut trans_map: HashMap<String, racon_core::TransitionId> = HashMap::new();
        let mut initial_tokens: Vec<(String, u64)> = Vec::new();

        let mut inside_model = false;
        let mut line_num = 0usize;

        for raw_line in logical_statements(input) {
            line_num += 1;
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            // Detect model block
            if line.starts_with("SPN Model") || line.starts_with("spn Model") {
                inside_model = true;
                continue;
            }
            if line == "main {" {
                inside_model = false;
                continue;
            }
            if !inside_model {
                continue;
            }

            // Place definition: place P0; or place P0( tokens = 1 );
            if line.starts_with("place ") || line.starts_with("Place ") {
                let rest = &line["place ".len()..].trim();
                let (name, tokens) = if let Some((id_part, paren_part)) = rest.split_once('(') {
                    let name = id_part.trim().trim_end_matches(';').trim().to_string();
                    let tokens_str = paren_part
                        .trim_start()
                        .trim_end_matches([')', ';', ' '].as_ref())
                        .trim();
                    let tokens: u64 = tokens_str
                        .split('=')
                        .nth(1)
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0);
                    if tokens > self.config.max_initial_marking {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Initial marking {} exceeds max {}",
                                tokens, self.config.max_initial_marking
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    (name, tokens)
                } else {
                    let name = rest.trim_end_matches(';').trim().to_string();
                    (name, 0u64)
                };
                let pid = builder.add_place(name.clone(), None);
                place_map.insert(name.clone(), pid);
                initial_tokens.push((name, tokens));
            }

            // Transition definition:
            //   timedTransition     T0( inputs = [P0], outputs = [P1], inhibitors = [P2(3)], delay = 0.0 );
            //   immediateTransition T1( inputs = [P0], outputs = [P1], inhibitors = [P2] );
            // Inhibitors default to threshold 1 (`M(P2) = 0`); `P2(k)` requires `M(P2) < k`.
            let (keyword_len, kind) = if line.starts_with("timedTransition ")
                || line.starts_with("TimedTransition ")
            {
                ("timedTransition ".len(), TransitionKind::Normal)
            } else if line.starts_with("immediateTransition ")
                || line.starts_with("ImmediateTransition ")
            {
                ("immediateTransition ".len(), TransitionKind::Immediate)
            } else {
                continue;
            };
            {
                let rest = &line[keyword_len..].trim();
                let (tname, paren) =
                    rest.split_once('(').ok_or_else(|| RaconError::ParseError {
                        format: "mercury".into(),
                        reason: format!("Invalid transition at line {}", line_num),
                        line: Some(line_num),
                        col: None,
                    })?;
                let tname = tname.trim().to_string();
                let tid = builder.add_transition(tname.clone(), None);
                trans_map.insert(tname.clone(), tid);
                if kind == TransitionKind::Immediate {
                    builder.set_transition_kind(tid, kind);
                    reject_numeric_priority(paren, line_num)?;
                }

                // Extract inputs and outputs
                let paren = paren.trim_end_matches(')').trim_end_matches(';');
                let inputs = self.extract_list(paren, "inputs");
                let outputs = self.extract_list(paren, "outputs");

                // Validate and build input arcs: place -> transition
                for input_spec in inputs {
                    let (place_name, weight) = self.parse_list_entry(input_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range (1..{}) at line {}",
                                weight, self.config.max_arc_weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Place(pid),
                            ArcEndpoint::Transition(tid),
                            weight,
                        );
                    }
                }

                // Validate and build output arcs: transition -> place
                for output_spec in outputs {
                    let (place_name, weight) = self.parse_list_entry(output_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range (1..{}) at line {}",
                                weight, self.config.max_arc_weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Transition(tid),
                            ArcEndpoint::Place(pid),
                            weight,
                        );
                    }
                }

                self.add_inhibitors(&mut builder, &place_map, tid, paren, line_num)?;
            }
        }

        // Build initial marking in correct place order
        let tokens: Vec<TokenValue> = initial_tokens
            .iter()
            .map(|(_, t)| TokenValue::Finite(*t))
            .collect();

        builder.build(tokens)
    }

    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError> {
        let content = std::fs::read_to_string(path).map_err(RaconError::Io)?;
        self.parse_string(&content)
    }
}

impl MercuryParserImpl {
    /// Extract a named list from a parameter string.
    /// e.g., inputs = [P0(2), P1] → ["P0(2)", "P1"]
    fn extract_list<'a>(&self, params: &'a str, list_name: &str) -> Vec<&'a str> {
        if let Some(start) = params.find(&format!("{} = [", list_name)) {
            let start = start + list_name.len() + 4; // skip "name = ["
            if let Some(end) = params[start..].find(']') {
                let list_str = &params[start..start + end];
                return list_str.split(',').map(|s| s.trim()).collect();
            }
        }
        Vec::new()
    }

    /// Parse the optional `inhibitors = [P(k), ...]` list of a transition.
    ///
    /// Unlike ordinary arcs, malformed entries and unknown places are hard
    /// errors: silently dropping an inhibitor would *enable* transitions that
    /// the model forbids. A missing `(k)` means threshold 1 (`M(P) = 0`).
    fn add_inhibitors(
        &self,
        builder: &mut PetriNetBuilder,
        place_map: &HashMap<String, racon_core::PlaceId>,
        tid: racon_core::TransitionId,
        params: &str,
        line_num: usize,
    ) -> Result<(), RaconError> {
        let err = |reason: String| RaconError::ParseError {
            format: "mercury".into(),
            reason,
            line: Some(line_num),
            col: None,
        };
        for entry in self.extract_list(params, "inhibitors") {
            if entry.is_empty() {
                continue;
            }
            let (name, threshold) = match entry.split_once('(') {
                Some((name, rest)) => {
                    let digits = rest.trim().strip_suffix(')').ok_or_else(|| {
                        err(format!("Malformed inhibitor entry '{entry}' at line {line_num}"))
                    })?;
                    let threshold: u64 = digits.trim().parse().map_err(|_| {
                        err(format!(
                            "Inhibitor threshold '{}' is not a positive integer at line {line_num}",
                            digits.trim()
                        ))
                    })?;
                    (name.trim(), threshold)
                }
                None => (entry.trim(), 1),
            };
            if threshold == 0 || threshold > self.config.max_initial_marking {
                return Err(err(format!(
                    "Inhibitor threshold {threshold} out of range (1..{}) at line {line_num}",
                    self.config.max_initial_marking
                )));
            }
            let pid = place_map
                .get(name)
                .ok_or_else(|| err(format!("Inhibitor place '{name}' is not declared (line {line_num})")))?;
            builder.add_inhibitor_arc(*pid, tid, threshold);
        }
        Ok(())
    }

    /// Parse a list entry: "P0" → ("P0", 1), "P0(2)" → ("P0", 2)
    fn parse_list_entry<'a>(&self, entry: &'a str) -> (&'a str, u32) {
        if let Some((name, paren)) = entry.split_once('(') {
            let weight: u32 = paren.trim_end_matches(')').parse().unwrap_or(1);
            (name.trim(), weight.max(1))
        } else {
            (entry.trim(), 1)
        }
    }
}

impl MercuryParserImpl {
    /// Parse MercuryScript with GSPN support.
    /// Returns both the PetriNet and optional GspnSpec.
    pub fn parse_gspn(&self, input: &str) -> Result<MercuryParseResult, RaconError> {
        let mut builder = PetriNetBuilder::default();
        let mut place_map: HashMap<String, racon_core::PlaceId> = HashMap::new();
        let mut trans_map: HashMap<String, racon_core::TransitionId> = HashMap::new();
        let mut initial_tokens: Vec<(String, u64)> = Vec::new();
        let mut gspn_types: HashMap<racon_core::TransitionId, TransitionType> = HashMap::new();
        let mut has_gspn_data = false;

        let mut inside_model = false;
        let mut line_num = 0usize;

        for raw_line in logical_statements(input) {
            line_num += 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            // Detect model block
            if line.starts_with("SPN Model") || line.starts_with("spn Model") {
                inside_model = true;
                continue;
            }
            if line == "main {" {
                inside_model = false;
                continue;
            }
            if !inside_model {
                continue;
            }

            // Place definition (same as parse_string but without max initial marking check for simplicity)
            if line.starts_with("place ") || line.starts_with("Place ") {
                let rest = &line["place ".len()..].trim();
                let (name, tokens) = if let Some((id_part, paren_part)) = rest.split_once('(') {
                    let name = id_part.trim().trim_end_matches(';').trim().to_string();
                    let tokens_str = paren_part
                        .trim_start()
                        .trim_end_matches([')', ';', ' '].as_ref())
                        .trim();
                    let tokens: u64 = tokens_str
                        .split('=')
                        .nth(1)
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0);
                    (name, tokens)
                } else {
                    let name = rest.trim_end_matches(';').trim().to_string();
                    (name, 0u64)
                };
                let pid = builder.add_place(name.clone(), None);
                place_map.insert(name.clone(), pid);
                initial_tokens.push((name, tokens));
                continue;
            }

            // Timed transition
            if line.starts_with("timedTransition ") || line.starts_with("TimedTransition ") {
                let rest = &line["timedTransition ".len()..].trim();
                let (tname, paren) =
                    rest.split_once('(').ok_or_else(|| RaconError::ParseError {
                        format: "mercury".into(),
                        reason: format!("Invalid transition at line {}", line_num),
                        line: Some(line_num),
                        col: None,
                    })?;
                let tname = tname.trim().to_string();
                let tid = builder.add_transition(tname.clone(), None);
                trans_map.insert(tname.clone(), tid);

                let paren = paren.trim_end_matches(')').trim_end_matches(';');

                // Extract inputs and outputs (reusing helper methods)
                let inputs = self.extract_list(paren, "inputs");
                let outputs = self.extract_list(paren, "outputs");

                for input_spec in inputs {
                    let (place_name, weight) = self.parse_list_entry(input_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range at line {}",
                                weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Place(pid),
                            ArcEndpoint::Transition(tid),
                            weight,
                        );
                    }
                }
                for output_spec in outputs {
                    let (place_name, weight) = self.parse_list_entry(output_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range at line {}",
                                weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Transition(tid),
                            ArcEndpoint::Place(pid),
                            weight,
                        );
                    }
                }

                self.add_inhibitors(&mut builder, &place_map, tid, paren, line_num)?;

                // Extract rate or delay
                if let Some(delay_str) = extract_value(paren, "delay") {
                    let delay: f64 = delay_str.parse().map_err(|_| RaconError::ParseError {
                        format: "mercury".into(),
                        reason: format!("Invalid delay value at line {}", line_num),
                        line: Some(line_num),
                        col: None,
                    })?;
                    if delay <= 0.0 {
                        // delay=0 or negative: treat as P/T (no timing)
                        continue;
                    }
                    let rate = 1.0 / delay;
                    gspn_types.insert(tid, TransitionType::Timed { rate });
                    has_gspn_data = true;
                } else if let Some(rate_str) = extract_value(paren, "rate") {
                    let rate: f64 = rate_str.parse().map_err(|_| RaconError::ParseError {
                        format: "mercury".into(),
                        reason: format!("Invalid rate value at line {}", line_num),
                        line: Some(line_num),
                        col: None,
                    })?;
                    if rate <= 0.0 {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!("Rate must be positive at line {}", line_num),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    gspn_types.insert(tid, TransitionType::Timed { rate });
                    has_gspn_data = true;
                }
                continue;
            }

            // Immediate transition (NEW)
            if line.starts_with("immediateTransition ") || line.starts_with("ImmediateTransition ")
            {
                let rest = &line["immediateTransition ".len()..].trim();
                let (tname, paren) =
                    rest.split_once('(').ok_or_else(|| RaconError::ParseError {
                        format: "mercury".into(),
                        reason: format!("Invalid immediate transition at line {}", line_num),
                        line: Some(line_num),
                        col: None,
                    })?;
                let tname = tname.trim().to_string();
                let tid = builder.add_transition(tname.clone(), None);
                trans_map.insert(tname.clone(), tid);

                let paren = paren.trim_end_matches(')').trim_end_matches(';');

                let inputs = self.extract_list(paren, "inputs");
                let outputs = self.extract_list(paren, "outputs");

                for input_spec in inputs {
                    let (place_name, weight) = self.parse_list_entry(input_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range at line {}",
                                weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Place(pid),
                            ArcEndpoint::Transition(tid),
                            weight,
                        );
                    }
                }
                for output_spec in outputs {
                    let (place_name, weight) = self.parse_list_entry(output_spec);
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "mercury".into(),
                            reason: format!(
                                "Arc weight {} out of range at line {}",
                                weight, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    if let Some(&pid) = place_map.get(place_name) {
                        builder.add_arc(
                            ArcEndpoint::Transition(tid),
                            ArcEndpoint::Place(pid),
                            weight,
                        );
                    }
                }

                self.add_inhibitors(&mut builder, &place_map, tid, paren, line_num)?;

                // Extract weight (default 1) and priority (default 0)
                let weight = extract_value(paren, "weight")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(1.0);
                let priority = extract_value(paren, "priority")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);

                gspn_types.insert(tid, TransitionType::Immediate { weight, priority });
                has_gspn_data = true;
            }
        }

        let tokens: Vec<TokenValue> = initial_tokens
            .iter()
            .map(|(_, t)| TokenValue::Finite(*t))
            .collect();
        let net = builder.build(tokens)?;

        let gspn_spec = if has_gspn_data {
            Some(GspnSpec {
                transition_types: gspn_types,
            })
        } else {
            None
        };

        Ok(MercuryParseResult { net, gspn_spec })
    }
}

/// Joins multiline Mercury declarations before the line-oriented parsers
/// inspect their parameters. Mercury examples commonly format transition
/// arguments over several lines, but the grammar still terminates each
/// declaration with `;`.
fn logical_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut pending = None::<String>;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        let is_declaration = line.starts_with("place ")
            || line.starts_with("Place ")
            || line.starts_with("timedTransition ")
            || line.starts_with("TimedTransition ")
            || line.starts_with("immediateTransition ")
            || line.starts_with("ImmediateTransition ");

        if pending.is_none() && !is_declaration {
            statements.push(line.to_string());
            continue;
        }

        let statement = pending.get_or_insert_with(String::new);
        if !statement.is_empty() {
            statement.push(' ');
        }
        statement.push_str(line);

        if line.contains(';') {
            statements.push(pending.take().expect("pending Mercury declaration"));
        }
    }

    if let Some(statement) = pending {
        statements.push(statement);
    }

    statements
}

/// The qualitative extended semantics has a single precedence level for
/// immediates (spec §1.7): numeric priorities cannot be represented, so a
/// non-zero `priority` is rejected instead of being silently flattened.
/// `weight` is ignored by design (conflicts are non-deterministic).
fn reject_numeric_priority(params: &str, line_num: usize) -> Result<(), RaconError> {
    if let Some(value) = extract_value(params, "priority") {
        if value.parse::<u32>().ok() != Some(0) {
            return Err(RaconError::ParseError {
                format: "mercury".into(),
                reason: format!(
                    "Numeric immediate priority '{value}' is not supported by racon-extended-pt-v1 (line {line_num})"
                ),
                line: Some(line_num),
                col: None,
            });
        }
    }
    Ok(())
}

/// Extract a named value from a parameter string.
/// e.g., extract_value("inputs = [P0], outputs = [P1], delay = 2.0", "delay") → Some("2.0")
fn extract_value<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("{} = ", key);
    params.find(&pattern).and_then(|start| {
        let start = start + pattern.len();
        let rest = &params[start..];
        let end = rest
            .find(|c: char| c == ',' || c == ')' || c == ';')
            .unwrap_or(rest.len());
        Some(rest[..end].trim())
    })
}
