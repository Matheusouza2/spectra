use racon_core::{PetriNet, RaconError};
use std::collections::HashSet;
use std::path::Path;

/// Configuration for PNT parsing.
#[derive(Debug, Clone)]
pub struct PntParserConfig {
    pub validate_math: bool,
    pub allow_comments: bool,
    pub trim_whitespace: bool,
    pub max_arc_weight: u32,
    pub max_initial_marking: u64,
}

impl Default for PntParserConfig {
    fn default() -> Self {
        Self {
            validate_math: true,
            allow_comments: true,
            trim_whitespace: true,
            max_arc_weight: 1_000_000,
            max_initial_marking: 1_000_000_000,
        }
    }
}

/// Trait for PNT format parsers.
pub trait PntParser {
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError>;
    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError>;
    fn parse_reader<R: std::io::Read>(&self, reader: R) -> Result<PetriNet, RaconError>;
}

/// Default PNT parser implementation.
pub struct PntParserImpl {
    config: PntParserConfig,
}

impl PntParserImpl {
    pub fn new(config: PntParserConfig) -> Self {
        Self { config }
    }
}

use racon_core::{ArcEndpoint, PetriNetBuilder, TokenValue};
use std::collections::HashMap;

struct PntSection {
    places: Vec<(String, u64)>,       // (name, initial_tokens)
    transitions: Vec<String>,         // names
    arcs: Vec<(String, String, u32)>, // (source, target, weight)
}

impl PntParser for PntParserImpl {
    fn parse_string(&self, input: &str) -> Result<PetriNet, RaconError> {
        // Auto-detect INA format (legacy .pnt from Integrated Net Analyzer)
        if Self::is_ina_format(input) {
            self.parse_ina(input)
        } else {
            self.parse_reader(input.as_bytes())
        }
    }

    fn parse_file(&self, path: &Path) -> Result<PetriNet, RaconError> {
        let content = std::fs::read_to_string(path).map_err(RaconError::Io)?;
        self.parse_string(&content)
    }

    fn parse_reader<R: std::io::Read>(&self, reader: R) -> Result<PetriNet, RaconError> {
        use std::io::BufRead;
        let buf = std::io::BufReader::new(reader);
        let lines: Vec<String> = buf
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .map_err(RaconError::Io)?;

        let section = self.parse_sections(&lines)?;
        self.build_petri_net(section)
    }
}

impl PntParserImpl {
    fn parse_sections(&self, lines: &[String]) -> Result<PntSection, RaconError> {
        let mut section = PntSection {
            places: Vec::new(),
            transitions: Vec::new(),
            arcs: Vec::new(),
        };

        enum Section {
            None,
            Places,
            Transitions,
            Arcs,
        }
        let mut current = Section::None;
        let mut line_num = 0usize;

        for raw in lines {
            line_num += 1;
            let line = if self.config.trim_whitespace {
                raw.trim()
            } else {
                raw
            };

            // Skip empty lines and comments
            if line.is_empty() {
                continue;
            }
            if self.config.allow_comments && line.starts_with('#') {
                continue;
            }

            // Section headers
            if line == "PLACES" {
                current = Section::Places;
                continue;
            }
            if line == "TRANSITIONS" {
                current = Section::Transitions;
                continue;
            }
            if line == "ARCS" {
                current = Section::Arcs;
                continue;
            }

            match current {
                Section::Places => {
                    // Format: <id>: tokens=<number>
                    let err = || RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!(
                            "Invalid place definition at line {}: '{}'",
                            line_num, line
                        ),
                        line: Some(line_num),
                        col: None,
                    };
                    let (id_part, tokens_part) = line.split_once(':').ok_or_else(err)?;
                    let name = id_part.trim().to_string();
                    // Validate: must start with P or p (per spec)
                    if !name.starts_with('P') && !name.starts_with('p') {
                        return Err(RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!(
                                "Place ID '{}' must start with P or p (line {})",
                                name, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    let tokens_str = tokens_part.trim();
                    let tokens_val = tokens_str
                        .strip_prefix("tokens=")
                        .or_else(|| tokens_str.strip_prefix("TOKENS="))
                        .ok_or_else(|| RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!("Missing 'tokens=' at line {}: '{}'", line_num, line),
                            line: Some(line_num),
                            col: None,
                        })?;
                    let tokens: u64 = tokens_val.trim().parse().map_err(|_| err())?;
                    if tokens > self.config.max_initial_marking {
                        return Err(RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!(
                                "Initial marking {} exceeds maximum {} at line {}",
                                tokens, self.config.max_initial_marking, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    section.places.push((name, tokens));
                }
                Section::Transitions => {
                    // Format: <id>
                    if !line.starts_with('T') && !line.starts_with('t') {
                        return Err(RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!(
                                "Transition ID '{}' must start with T or t (line {})",
                                line, line_num
                            ),
                            line: Some(line_num),
                            col: None,
                        });
                    }
                    section.transitions.push(line.to_string());
                }
                Section::Arcs => {
                    // Format: <source> -> <target>[: weight=<number>]
                    let err = || RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!("Invalid arc definition at line {}: '{}'", line_num, line),
                        line: Some(line_num),
                        col: None,
                    };
                    let (source_part, rest) = line.split_once("->").ok_or_else(err)?;
                    let source = source_part.trim().to_string();
                    let rest = rest.trim();

                    let (target, weight) = if let Some((t, w)) = rest.split_once(':') {
                        let target = t.trim().to_string();
                        let w_str = w.trim();
                        let w_val = w_str
                            .strip_prefix("weight=")
                            .or_else(|| w_str.strip_prefix("WEIGHT="))
                            .ok_or_else(err)?;
                        let w: u32 = w_val.trim().parse().map_err(|_| err())?;
                        if w == 0 || w > self.config.max_arc_weight {
                            return Err(RaconError::ParseError {
                                format: "pnt".into(),
                                reason: format!(
                                    "Arc weight {} out of range (1..{}) at line {}",
                                    w, self.config.max_arc_weight, line_num
                                ),
                                line: Some(line_num),
                                col: None,
                            });
                        }
                        (target, w)
                    } else {
                        (rest.to_string(), 1u32)
                    };

                    section.arcs.push((source, target, weight));
                }
                Section::None => {
                    return Err(RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!(
                            "Content before any section header at line {}: '{}'",
                            line_num, line
                        ),
                        line: Some(line_num),
                        col: None,
                    });
                }
            }
        }

        Ok(section)
    }

    fn build_petri_net(&self, section: PntSection) -> Result<PetriNet, RaconError> {
        let mut builder = PetriNetBuilder::default();
        let mut place_map: HashMap<String, racon_core::PlaceId> = HashMap::new();
        let mut trans_map: HashMap<String, racon_core::TransitionId> = HashMap::new();
        let mut initial_tokens: Vec<TokenValue> = Vec::new();

        // Build places
        for (name, tokens) in &section.places {
            let pid = builder.add_place(name.clone(), None);
            place_map.insert(name.clone(), pid);
            initial_tokens.push(TokenValue::Finite(*tokens));
        }

        // Build transitions
        for name in &section.transitions {
            let tid = builder.add_transition(name.clone(), None);
            trans_map.insert(name.clone(), tid);
        }

        // Build arcs
        for (src, tgt, weight) in &section.arcs {
            match (
                place_map.get(src),
                trans_map.get(tgt),
                place_map.get(tgt),
                trans_map.get(src),
            ) {
                (Some(&p), Some(&t), _, _) => {
                    builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), *weight);
                }
                (_, _, Some(&p), Some(&t)) => {
                    builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), *weight);
                }
                _ => {
                    return Err(RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!("Invalid arc endpoints: {} -> {}", src, tgt),
                        line: None,
                        col: None,
                    });
                }
            }
        }

        builder.build(initial_tokens)
    }

    // ── INA format parser (legacy Integrated Net Analyzer .pnt) ──────────────

    /// Detect whether the input uses the INA format.
    ///
    /// INA format starts with a header line like:
    /// `P   M   PRE,POST  NETZ <netnr>[:<netname>]`
    fn is_ina_format(input: &str) -> bool {
        input
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|first| {
                let t = first.trim();
                t.contains("PRE,POST") && t.contains("NETZ")
            })
            .unwrap_or(false)
    }

    /// Parse a legacy INA-format `.pnt` file.
    ///
    /// ## Format (from INA Manual §2.3.4)
    ///
    /// ```text
    /// P   M   PRE,POST  NETZ <nr>[:<name>]
    /// <placenr> <tokens> [<prelist>] [, <postlist>]
    /// ...
    /// @
    /// place nr.             name capacity time
    ///     <nr>: <name>              <cap> <time>
    /// ...
    /// @
    /// trans nr.             name priority time
    ///     <nr>: <name>              <pri> <time>
    /// ...
    /// @
    /// ```
    ///
    /// Each placedef line may have a prelist and/or a postlist separated by `,`.
    /// Pre/post entries are space-separated transition references:
    ///   - `<transnr>`         → weight 1
    ///   - `<transnr>: <wgt>`  → explicit weight
    fn parse_ina(&self, input: &str) -> Result<PetriNet, RaconError> {
        let lines: Vec<&str> = input.lines().collect();
        let len = lines.len();
        let mut pos: usize = 0;

        // ---------- helper: skip empty/comment lines ----------
        let advance = |p: &mut usize| {
            while *p < len {
                let t = lines[*p].trim();
                if t.is_empty() || (self.config.allow_comments && t.starts_with('#')) {
                    *p += 1;
                } else {
                    break;
                }
            }
        };

        // ---------- 1. Header ----------
        advance(&mut pos);
        if pos >= len {
            return Err(RaconError::ParseError {
                format: "pnt".into(),
                reason: "Empty file, expected INA header".into(),
                line: None,
                col: None,
            });
        }
        // Header is consumed; move past it
        pos += 1;

        // ---------- 2. Structure section (placedef lines until '@') ----------
        let mut place_data: Vec<(u32, u64)> = Vec::new(); // (place_nr, tokens)
                                                          // (place_nr, trans_nr, weight) — pre arcs: Place → Transition
        let mut pre_arcs: Vec<(u32, u32, u32)> = Vec::new();
        // (place_nr, trans_nr, weight) — post arcs: Transition → Place
        let mut post_arcs: Vec<(u32, u32, u32)> = Vec::new();
        let mut seen_places: HashSet<u32> = HashSet::new();
        let mut seen_trans: HashSet<u32> = HashSet::new();

        // Helper: parse comma-separated entry list
        let parse_entries = |tokens: &[&str],
                             place_nr: u32,
                             arcs: &mut Vec<(u32, u32, u32)>,
                             trans_set: &mut HashSet<u32>,
                             ln: usize|
         -> Result<(), RaconError> {
            let mut i = 0;
            while i < tokens.len() {
                let tok = tokens[i];
                if tok.is_empty() {
                    i += 1;
                    continue;
                }
                if tok.contains(':') {
                    // <transnr>: <weight>, also written glued as <transnr>:<weight>
                    let (left, right) = tok.split_once(':').unwrap_or((tok, ""));
                    let trans_nr: u32 =
                        left
                            .parse()
                            .map_err(|_| RaconError::ParseError {
                                format: "pnt".into(),
                                reason: format!(
                                    "Invalid transition number in INA entry: '{}'",
                                    tok
                                ),
                                line: Some(ln),
                                col: None,
                            })?;
                    i += 1;
                    let weight_text = if !right.is_empty() {
                        right
                    } else if i < tokens.len() {
                        let next = tokens[i];
                        i += 1;
                        next
                    } else {
                        return Err(RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!("Missing weight after '{}' in INA entry", tok),
                            line: Some(ln),
                            col: None,
                        });
                    };
                    let weight: u32 = weight_text.parse().map_err(|_| RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!("Expected weight after '{}' in INA entry", tok),
                        line: Some(ln),
                        col: None,
                    })?;
                    if weight == 0 || weight > self.config.max_arc_weight {
                        return Err(RaconError::ParseError {
                            format: "pnt".into(),
                            reason: format!(
                                "Arc weight {} out of range (1..{})",
                                weight, self.config.max_arc_weight
                            ),
                            line: Some(ln),
                            col: None,
                        });
                    }
                    arcs.push((place_nr, trans_nr, weight));
                    trans_set.insert(trans_nr);
                } else {
                    // Plain <transnr> → weight 1
                    let trans_nr: u32 = tok.parse().map_err(|_| RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!("Invalid transition number in INA entry: '{}'", tok),
                        line: Some(ln),
                        col: None,
                    })?;
                    arcs.push((place_nr, trans_nr, 1));
                    trans_set.insert(trans_nr);
                    i += 1;
                }
            }
            Ok(())
        };

        advance(&mut pos);
        while pos < len {
            let trimmed = lines[pos].trim();
            pos += 1;
            if trimmed.is_empty() || (self.config.allow_comments && trimmed.starts_with('#')) {
                continue;
            }
            if trimmed == "@" {
                break; // End of structure section
            }

            // Split on comma to separate prelist and postlist
            let (pre_str, post_str) = match trimmed.split_once(',') {
                Some((p, s)) => (p.trim(), s.trim()),
                None => (trimmed, ""),
            };

            let pre_tokens: Vec<&str> = pre_str.split_whitespace().collect();
            if pre_tokens.len() < 2 {
                return Err(RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!(
                        "Invalid INA place definition (need nr + tokens): '{}'",
                        trimmed
                    ),
                    line: Some(pos),
                    col: None,
                });
            }

            let place_nr: u32 = pre_tokens[0].parse().map_err(|_| RaconError::ParseError {
                format: "pnt".into(),
                reason: format!("Invalid place number in INA file: '{}'", pre_tokens[0]),
                line: Some(pos),
                col: None,
            })?;
            let tokens: u64 = pre_tokens[1].parse().map_err(|_| RaconError::ParseError {
                format: "pnt".into(),
                reason: format!("Invalid token count in INA file: '{}'", pre_tokens[1]),
                line: Some(pos),
                col: None,
            })?;
            if tokens > self.config.max_initial_marking {
                return Err(RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!(
                        "Initial marking {} exceeds maximum {}",
                        tokens, self.config.max_initial_marking
                    ),
                    line: Some(pos),
                    col: None,
                });
            }

            if !seen_places.insert(place_nr) {
                return Err(RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!("Duplicate place number {} in INA file", place_nr),
                    line: Some(pos),
                    col: None,
                });
            }
            place_data.push((place_nr, tokens));

            // INA manual §2.3.4: <prelist> holds the pre-transitions of the
            // place (•p, arcs Transition → Place) and <postlist> its
            // post-transitions (p•, arcs Place → Transition). In the example
            // net, `terminal_free` has prelist `4 5 6` (the logouts return the
            // terminal) and postlist `1 2 3` (the logins take it).
            if pre_tokens.len() > 2 {
                parse_entries(
                    &pre_tokens[2..],
                    place_nr,
                    &mut post_arcs,
                    &mut seen_trans,
                    pos,
                )?;
            }

            // Parse postlist — Place → Transition arcs
            if !post_str.is_empty() {
                let post_tokens: Vec<&str> = post_str.split_whitespace().collect();
                parse_entries(&post_tokens, place_nr, &mut pre_arcs, &mut seen_trans, pos)?;
            }
        }

        // ---------- 3. Place names section ----------
        // Line "place nr.             name capacity time" then data lines, then '@'
        // Each data line: "<nr>: <name> ..." — we extract only nr and name.
        let mut place_names: HashMap<u32, String> = HashMap::new();
        advance(&mut pos);
        while pos < len {
            let trimmed = lines[pos].trim();
            pos += 1;
            if trimmed == "@" {
                break;
            }
            if let Some((id_str, rest)) = trimmed.split_once(':') {
                if let Ok(nr) = id_str.trim().parse::<u32>() {
                    let name = rest
                        .trim()
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        place_names.insert(nr, name);
                    }
                }
            }
        }

        // ---------- 4. Transition names section ----------
        // Line "trans nr.             name priority time" then data lines, then '@'
        let mut trans_names: HashMap<u32, String> = HashMap::new();
        // Priority column of the transition table (INA: higher fires first).
        let mut trans_priority: HashMap<u32, u64> = HashMap::new();
        advance(&mut pos);
        while pos < len {
            let trimmed = lines[pos].trim();
            pos += 1;
            if trimmed == "@" {
                break;
            }
            if let Some((id_str, rest)) = trimmed.split_once(':') {
                if let Ok(nr) = id_str.trim().parse::<u32>() {
                    let mut fields = rest.trim().split_whitespace();
                    let name = fields.next().unwrap_or("").to_string();
                    if !name.is_empty() {
                        trans_names.insert(nr, name);
                        if let Some(priority) = fields.next().and_then(|v| v.parse::<u64>().ok()) {
                            trans_priority.insert(nr, priority);
                        }
                    }
                }
            }
        }

        // ---------- 4b. RACoN extension block (optional) ----------
        // racon-extended-pt-v1
        // immediate <trans_nr>
        // inhibitor <place_nr> <trans_nr> <threshold>
        // @
        // Plain INA files have no such block and keep their exact old meaning.
        let mut immediates: Vec<u32> = Vec::new();
        let mut inhibitors: Vec<(u32, u32, u64)> = Vec::new();
        advance(&mut pos);
        if pos < len && lines[pos].trim() == racon_core::SEMANTICS_VERSION {
            pos += 1;
            let ext_err = |reason: String, line: usize| RaconError::ParseError {
                format: "pnt".into(),
                reason,
                line: Some(line + 1),
                col: None,
            };
            while pos < len {
                let trimmed = lines[pos].trim();
                let line_no = pos;
                pos += 1;
                if trimmed.is_empty() || (self.config.allow_comments && trimmed.starts_with('#')) {
                    continue;
                }
                if trimmed == "@" {
                    break;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let number = |text: &str| -> Result<u64, RaconError> {
                    text.parse::<u64>().map_err(|_| {
                        ext_err(format!("Invalid number '{text}' in extension block"), line_no)
                    })
                };
                match parts.as_slice() {
                    ["immediate", t] => {
                        let t = number(t)? as u32;
                        seen_trans.insert(t);
                        immediates.push(t);
                    }
                    ["inhibitor", p, t, k] => {
                        let (p, t, k) = (number(p)? as u32, number(t)? as u32, number(k)?);
                        if k == 0 || k > self.config.max_initial_marking {
                            return Err(ext_err(
                                format!("Inhibitor threshold {k} out of range (1..{})", self.config.max_initial_marking),
                                line_no,
                            ));
                        }
                        seen_trans.insert(t);
                        inhibitors.push((p, t, k));
                    }
                    _ => {
                        return Err(ext_err(
                            format!("Unknown directive in extension block: '{trimmed}'"),
                            line_no,
                        ))
                    }
                }
            }
        }

        // ---------- 4c. Priorities → immediate transitions ----------
        // RACoN models exactly two precedence levels: immediate transitions pre-empt
        // all others. An INA priority column with two distinct levels maps onto that
        // (the higher level is immediate); one level means no relative precedence.
        // More levels cannot be represented faithfully, so refuse instead of guessing.
        {
            let levels: std::collections::BTreeSet<u64> = seen_trans
                .iter()
                .filter_map(|t| trans_priority.get(t).copied())
                .collect();
            match levels.len() {
                0 | 1 => {}
                2 => {
                    let top = *levels.iter().next_back().unwrap();
                    for t in &seen_trans {
                        if trans_priority.get(t) == Some(&top) && !immediates.contains(t) {
                            immediates.push(*t);
                        }
                    }
                }
                n => {
                    return Err(RaconError::ParseError {
                        format: "pnt".into(),
                        reason: format!(
                            "The transition priority column has {n} distinct levels; RACoN supports only two (immediate and normal)"
                        ),
                        line: None,
                        col: None,
                    })
                }
            }
        }

        // ---------- 5. Build PetriNet ----------
        use racon_core::TokenValue;

        // Sort by number for deterministic order
        place_data.sort_by_key(|k| k.0);
        let mut sorted_trans: Vec<u32> = seen_trans.into_iter().collect();
        sorted_trans.sort();

        let mut builder = PetriNetBuilder::default();
        let mut place_map: HashMap<u32, racon_core::PlaceId> = HashMap::new();
        let mut trans_map: HashMap<u32, racon_core::TransitionId> = HashMap::new();
        let mut initial_tokens: Vec<TokenValue> = Vec::new();

        // Create places in sorted order — use REAL names from names section
        for (nr, tokens) in &place_data {
            let name = place_names
                .get(nr)
                .cloned()
                .unwrap_or_else(|| format!("P{}", nr));
            let pid = builder.add_place(name, None);
            place_map.insert(*nr, pid);
            initial_tokens.push(TokenValue::Finite(*tokens));
        }

        // Create transitions in sorted order — use REAL names from names section
        for nr in &sorted_trans {
            let name = trans_names
                .get(nr)
                .cloned()
                .unwrap_or_else(|| format!("T{}", nr));
            let tid = builder.add_transition(name, None);
            trans_map.insert(*nr, tid);
        }

        // Create arcs: pre = Place → Transition
        for (place_nr, trans_nr, weight) in &pre_arcs {
            let p = place_map
                .get(place_nr)
                .ok_or_else(|| RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!("INA pre arc references unknown place {}", place_nr),
                    line: None,
                    col: None,
                })?;
            let t = trans_map
                .get(trans_nr)
                .ok_or_else(|| RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!("INA pre arc references unknown transition {}", trans_nr),
                    line: None,
                    col: None,
                })?;
            builder.add_arc(ArcEndpoint::Place(*p), ArcEndpoint::Transition(*t), *weight);
        }

        // Create arcs: post = Transition → Place
        for (place_nr, trans_nr, weight) in &post_arcs {
            let p = place_map
                .get(place_nr)
                .ok_or_else(|| RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!("INA post arc references unknown place {}", place_nr),
                    line: None,
                    col: None,
                })?;
            let t = trans_map
                .get(trans_nr)
                .ok_or_else(|| RaconError::ParseError {
                    format: "pnt".into(),
                    reason: format!("INA post arc references unknown transition {}", trans_nr),
                    line: None,
                    col: None,
                })?;
            builder.add_arc(ArcEndpoint::Transition(*t), ArcEndpoint::Place(*p), *weight);
        }

        for (place_nr, trans_nr, threshold) in &inhibitors {
            let p = place_map.get(place_nr).ok_or_else(|| RaconError::ParseError {
                format: "pnt".into(),
                reason: format!("Inhibitor arc references unknown place {}", place_nr),
                line: None,
                col: None,
            })?;
            let t = trans_map.get(trans_nr).ok_or_else(|| RaconError::ParseError {
                format: "pnt".into(),
                reason: format!("Inhibitor arc references unknown transition {}", trans_nr),
                line: None,
                col: None,
            })?;
            builder.add_inhibitor_arc(*p, *t, *threshold);
        }
        for trans_nr in &immediates {
            let t = trans_map.get(trans_nr).ok_or_else(|| RaconError::ParseError {
                format: "pnt".into(),
                reason: format!("Immediate marker references unknown transition {}", trans_nr),
                line: None,
                col: None,
            })?;
            builder.set_transition_kind(*t, racon_core::TransitionKind::Immediate);
        }

        builder.build(initial_tokens)
    }
}

/// Serializes a net as an INA-style `.pnt` text readable by [`PntParserImpl`].
///
/// Plain nets produce exactly the legacy layout. Nets with inhibitor arcs or
/// immediate transitions append a `racon-extended-pt-v1` block, so the
/// extended semantics survive every Mercury → PNT → analysis round trip.
pub fn petri_net_to_ina_string(net: &PetriNet) -> String {
    use racon_core::{ArcEndpoint as E, TokenValue as V};
    let mut out = String::new();
    out.push_str("P   M   PRE,POST  NETZ 0:t\n");

    for p in 0..net.place_count() {
        let m = match net.initial_marking().tokens.get(p) {
            Some(V::Finite(n)) => n.to_string(),
            Some(V::Exact(n)) => n.to_string(),
            _ => "0".to_string(),
        };
        out.push_str(&format!("{} {}", p, m));

        // INA .pnt: <prelist> = pre-transitions of the place (T -> P arcs),
        // <postlist> = post-transitions (P -> T arcs), each as `t` or `t: w`.
        let entry = |tid: u32, weight: u32| {
            if weight == 1 {
                format!(" {}", tid)
            } else {
                format!(" {}: {}", tid, weight)
            }
        };
        let mut produced_by: Vec<String> = Vec::new();
        let mut consumed_by: Vec<String> = Vec::new();
        for arc in net.arcs() {
            match (arc.source, arc.target) {
                (E::Place(pid), E::Transition(tid)) if pid.0 as usize == p => {
                    consumed_by.push(entry(tid.0, arc.weight));
                }
                (E::Transition(tid), E::Place(pid)) if pid.0 as usize == p => {
                    produced_by.push(entry(tid.0, arc.weight));
                }
                _ => {}
            }
        }
        for item in &produced_by {
            out.push_str(item);
        }
        // INA misreads a trailing "," with an empty post-list: it assigns the
        // pre-list to the post side (a sink place then looks consumed). Emit
        // the comma only when the post-list is non-empty.
        if !consumed_by.is_empty() {
            out.push_str(" ,");
            for item in &consumed_by {
                out.push_str(item);
            }
        }
        out.push('\n');
    }

    out.push_str("@\n");
    out.push_str("place nr.             name capacity time\n");
    for (i, place) in net.places().iter().enumerate() {
        out.push_str(&format!("     {:>3}: {:<22}65535    0\n", i, place.name));
    }
    out.push_str("@\n");
    out.push_str("trans nr.             name priority time\n");
    // Immediate transitions get priority 1, so tools that honour the priority
    // column (INA) reproduce the same precedence as the extension block below.
    for (i, t) in net.transitions().iter().enumerate() {
        let priority = u8::from(net.immediate_transitions().iter().any(|id| id.0 as usize == i));
        out.push_str(&format!("     {:>3}: {:<22}{}    0\n", i, t.name, priority));
    }
    out.push_str("@\n");

    if net.is_extended() {
        out.push_str(racon_core::SEMANTICS_VERSION);
        out.push('\n');
        for t in net.immediate_transitions() {
            out.push_str(&format!("immediate {}\n", t.0));
        }
        for inh in net.inhibitor_arcs() {
            out.push_str(&format!(
                "inhibitor {} {} {}\n",
                inh.place.0, inh.transition.0, inh.threshold
            ));
        }
        out.push_str("@\n");
    }
    out
}
