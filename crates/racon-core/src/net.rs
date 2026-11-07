use crate::error::RaconError;
use crate::marking::{Marking, TokenValue};
use crate::matrix::IncidenceMatrix;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// ID types
// ---------------------------------------------------------------------------

/// Identifies a place within a Petri net.
/// IDs are assigned sequentially by `PetriNetBuilder` starting from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlaceId(pub u32);

/// Identifies a transition within a Petri net.
/// IDs are assigned sequentially by `PetriNetBuilder` starting from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransitionId(pub u32);

// ---------------------------------------------------------------------------
// Arc endpoint
// ---------------------------------------------------------------------------

/// One end of an arc: either a place or a transition.
/// Arcs must always connect Place ↔ Transition (never Place ↔ Place).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArcEndpoint {
    Place(PlaceId),
    Transition(TransitionId),
}

// ---------------------------------------------------------------------------
// Core elements
// ---------------------------------------------------------------------------

/// A place (P-element) in a Petri net.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: PlaceId,
    pub name: String,
    pub label: Option<String>,
}

/// A transition (T-element) in a Petri net.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub id: TransitionId,
    pub name: String,
    pub label: Option<String>,
}

/// A directed arc connecting a place and a transition with a weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arc {
    pub source: ArcEndpoint,
    pub target: ArcEndpoint,
    pub weight: u32,
}

/// Kind of a transition under the RACoN extended P/T semantics.
///
/// `Immediate` transitions have precedence over `Normal` ones whenever any
/// immediate transition is basically enabled (Chiola et al. 1993, §2; spec
/// `racon-extended-pt-v1`). No time, rate, weight or numeric priority is
/// attached: conflicts between immediates are non-deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    #[default]
    Normal,
    Immediate,
}

/// Inhibitor arc `place --o transition` with a positive integer threshold `k`.
///
/// The arc only *tests* the marking: the transition requires `M(place) < k`.
/// It never consumes or produces tokens and is therefore absent from `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InhibitorArc {
    pub place: PlaceId,
    pub transition: TransitionId,
    pub threshold: u64,
}

/// Identifier of the enabling/firing semantics implemented by the core.
pub const SEMANTICS_VERSION: &str = "racon-extended-pt-v1";
/// Identifier of the precedence policy between transition kinds.
pub const PRIORITY_POLICY: &str = "immediate_over_normal";

// ---------------------------------------------------------------------------
// PetriNet — immutable after construction
// ---------------------------------------------------------------------------

/// A structurally-validated Petri net.
///
/// Constructed via `PetriNetBuilder`. After construction, the net is
/// immutable. All analysis operates on references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetriNet {
    places: Vec<Place>,
    transitions: Vec<Transition>,
    arcs: Vec<Arc>,
    /// Pre-computed preset for each transition: (place, weight) pairs.
    transition_preset: Vec<Vec<(PlaceId, u32)>>,
    /// Pre-computed postset for each transition: (place, weight) pairs.
    transition_postset: Vec<Vec<(PlaceId, u32)>>,
    incidence_matrix: IncidenceMatrix,
    initial_marking: Marking,
    /// Kind of each transition (indexed by transition id).
    #[serde(default)]
    transition_kinds: Vec<TransitionKind>,
    /// All inhibitor arcs, sorted by (transition, place).
    #[serde(default)]
    inhibitor_arcs: Vec<InhibitorArc>,
    /// Inhibitor arcs indexed by transition: (place, threshold).
    #[serde(default)]
    transition_inhibitors: Vec<Vec<(PlaceId, u64)>>,
    /// Cached list of immediate transitions.
    #[serde(default)]
    immediate_transitions: Vec<TransitionId>,
}

impl PetriNet {
    /// The places in this net.
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// The transitions in this net.
    pub fn transitions(&self) -> &[Transition] {
        &self.transitions
    }

    /// The arcs in this net.
    pub fn arcs(&self) -> &[Arc] {
        &self.arcs
    }

    /// Number of places.
    pub fn place_count(&self) -> usize {
        self.places.len()
    }

    /// Number of transitions.
    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }

    /// The initial marking set when the net was built.
    pub fn initial_marking(&self) -> &Marking {
        &self.initial_marking
    }

    /// The incidence matrix C = Post - Pre (places × transitions).
    pub fn incidence_matrix(&self) -> &IncidenceMatrix {
        &self.incidence_matrix
    }

    /// Input places (and arc weights) for a transition.
    /// Returns empty slice if the transition ID is out of range.
    pub fn preset(&self, transition: TransitionId) -> &[(PlaceId, u32)] {
        self.transition_preset
            .get(transition.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Output places (and arc weights) for a transition.
    /// Returns empty slice if the transition ID is out of range.
    pub fn postset(&self, transition: TransitionId) -> &[(PlaceId, u32)] {
        self.transition_postset
            .get(transition.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Kind of a transition (`Normal` for out-of-range ids and legacy nets).
    pub fn transition_kind(&self, transition: TransitionId) -> TransitionKind {
        self.transition_kinds
            .get(transition.0 as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Immediate transitions of the net.
    pub fn immediate_transitions(&self) -> &[TransitionId] {
        &self.immediate_transitions
    }

    /// All inhibitor arcs.
    pub fn inhibitor_arcs(&self) -> &[InhibitorArc] {
        &self.inhibitor_arcs
    }

    /// Inhibitor arcs of one transition as (place, threshold) pairs.
    pub fn inhibitors(&self, transition: TransitionId) -> &[(PlaceId, u64)] {
        self.transition_inhibitors
            .get(transition.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Whether the net uses inhibitor arcs.
    pub fn has_inhibitor_arcs(&self) -> bool {
        !self.inhibitor_arcs.is_empty()
    }

    /// Whether the net has immediate transitions (precedence over normal ones).
    pub fn has_immediate_transitions(&self) -> bool {
        !self.immediate_transitions.is_empty()
    }

    /// Whether the net leaves the plain P/T class (inhibitors or immediates).
    ///
    /// Monotonicity-based techniques (Karp–Miller ω-acceleration, Finkel
    /// pruning, stubborn sets, symmetry/structural reductions) are not valid
    /// for such nets without a dedicated proof.
    pub fn is_extended(&self) -> bool {
        self.has_inhibitor_arcs() || self.has_immediate_transitions()
    }

    /// Basic enabling `BasicEnabled(t, M)`: the ordinary pre-condition
    /// `M >= Pre(:,t)` **and** every inhibitor test `M(p) < k`.
    ///
    /// Immediate-over-normal precedence is *not* applied here; see
    /// [`PetriNet::is_enabled`] and [`PetriNet::fireable_transitions`].
    pub fn is_basic_enabled(&self, transition: TransitionId, marking: &Marking) -> bool {
        if transition.0 as usize >= self.transition_count() || marking.len() != self.place_count() {
            return false;
        }
        let preset_ok = self.preset(transition).iter().all(|&(place, weight)| {
            match marking.get(place.0 as usize) {
                Some(TokenValue::Omega) => true,
                Some(TokenValue::Finite(n)) => *n >= weight as u64,
                Some(TokenValue::Exact(n)) => *n >= BigUint::from(weight),
                None => false,
            }
        });
        preset_ok
            && self.inhibitors(transition).iter().all(|&(place, threshold)| {
                match marking.get(place.0 as usize) {
                    // ω stands for arbitrarily large counts: never below k.
                    Some(TokenValue::Omega) => false,
                    Some(TokenValue::Finite(n)) => *n < threshold,
                    Some(TokenValue::Exact(n)) => *n < BigUint::from(threshold),
                    None => false,
                }
            })
    }

    /// Whether some immediate transition is basically enabled in `marking`
    /// (a *vanishing* marking in the GSPN terminology, used here qualitatively).
    pub fn has_enabled_immediate(&self, marking: &Marking) -> bool {
        self.immediate_transitions
            .iter()
            .any(|&t| self.is_basic_enabled(t, marking))
    }

    /// Whether `transition` belongs to `Fireable(M)`.
    ///
    /// This is the **single source of truth** for the enabling relation used by
    /// simulation, reachability, deadlock, liveness, reversibility and CTL:
    /// `Fireable(M) = E_imm(M)` if non-empty, else `E(M)`.
    pub fn is_enabled(&self, transition: TransitionId, marking: &Marking) -> bool {
        if !self.is_basic_enabled(transition, marking) {
            return false;
        }
        match self.transition_kind(transition) {
            TransitionKind::Immediate => true,
            TransitionKind::Normal => {
                self.immediate_transitions.is_empty() || !self.has_enabled_immediate(marking)
            }
        }
    }

    /// `Fireable(M)` in increasing transition-id order. All elements are
    /// alternative successors under interleaving semantics.
    pub fn fireable_transitions(&self, marking: &Marking) -> Vec<TransitionId> {
        let mut immediates = Vec::new();
        let mut normals = Vec::new();
        for transition in &self.transitions {
            if !self.is_basic_enabled(transition.id, marking) {
                continue;
            }
            match self.transition_kind(transition.id) {
                TransitionKind::Immediate => immediates.push(transition.id),
                TransitionKind::Normal => normals.push(transition.id),
            }
        }
        if immediates.is_empty() {
            normals
        } else {
            immediates
        }
    }

    /// Transitions that are basically enabled but suppressed by the
    /// precedence of an enabled immediate transition.
    pub fn preempted_transitions(&self, marking: &Marking) -> Vec<TransitionId> {
        if !self.has_enabled_immediate(marking) {
            return Vec::new();
        }
        self.transitions
            .iter()
            .map(|t| t.id)
            .filter(|&t| {
                self.transition_kind(t) == TransitionKind::Normal
                    && self.is_basic_enabled(t, marking)
            })
            .collect()
    }

    /// Fire a transition in a given marking, producing the successor marking
    /// `M' = M - Pre(:,t) + Post(:,t) = M + C(:,t)`.
    ///
    /// Inhibitor arcs and the immediate kind only affect *enabling*; they have
    /// no effect on the token update.
    ///
    /// # Panics
    /// Panics if the transition is not enabled. Call `is_enabled` first.
    /// For analysis code that needs a safer path, check enabled before firing.
    pub fn fire(&self, transition: TransitionId, marking: &Marking) -> Marking {
        assert!(
            (transition.0 as usize) < self.transition_count(),
            "invalid transition id"
        );
        assert_eq!(
            marking.len(),
            self.place_count(),
            "marking has wrong dimension"
        );
        assert!(
            self.is_enabled(transition, marking),
            "transition is not enabled"
        );
        let mut tokens = marking.tokens.clone();
        for &(place, weight) in self.preset(transition) {
            let idx = place.0 as usize;
            tokens[idx] = tokens[idx]
                .clone()
                .saturating_sub(TokenValue::Finite(weight as u64));
        }
        for &(place, weight) in self.postset(transition) {
            let idx = place.0 as usize;
            tokens[idx] = tokens[idx]
                .clone()
                .saturating_add(TokenValue::Finite(weight as u64));
        }
        Marking::new(tokens)
    }

    /// Fire a transition only when the input marking has the net dimension
    /// and the transition is enabled. This is the checked form of the
    /// mathematical relation `m[t⟩m'` from Starke's Definition 2.4.
    pub fn try_fire(
        &self,
        transition: TransitionId,
        marking: &Marking,
    ) -> Result<Marking, RaconError> {
        if transition.0 as usize >= self.transition_count() {
            return Err(RaconError::InvalidInput(format!(
                "invalid transition id: {}",
                transition.0
            )));
        }
        if marking.len() != self.place_count() {
            return Err(RaconError::InvalidInput(format!(
                "Marking size {} != place count {}",
                marking.len(),
                self.place_count()
            )));
        }
        if !self.is_enabled(transition, marking) {
            return Err(RaconError::InvalidInput(format!(
                "Transition {:?} is not enabled",
                transition
            )));
        }
        Ok(self.fire(transition, marking))
    }

    /// Whether two distinct transitions are individually enabled but cannot
    /// fire together because their combined input demand exceeds the marking.
    /// This is the weighted, dynamic conflict definition; shared presets alone
    /// are only evidence of a structural conflict.
    pub fn is_dynamic_conflict(
        &self,
        first: TransitionId,
        second: TransitionId,
        marking: &Marking,
    ) -> bool {
        if first == second {
            return false;
        }
        if self.is_extended() {
            // Interleaving semantics only (spec §2.4): two fireable transitions
            // conflict when firing either one disables the other. Inhibitor
            // tests and immediate precedence can disable a transition without
            // any shared input place, so preset overlap is not enough.
            if !self.is_enabled(first, marking) || !self.is_enabled(second, marking) {
                return false;
            }
            let disables = |a: TransitionId, b: TransitionId| {
                self.try_fire(a, marking)
                    .map(|next| !self.is_enabled(b, &next))
                    .unwrap_or(false)
            };
            return disables(first, second) || disables(second, first);
        }
        self.is_dynamic_conflict_multiset(&[(first, 1), (second, 1)], marking)
    }

    /// Whether a transition multiset is concurrently fireable at `marking`.
    /// This is Starke's `u^- <= m`, where `u^- = Σ u(t) · t^-`.
    pub fn is_concurrent_multiset(
        &self,
        transitions: &[(TransitionId, u32)],
        marking: &Marking,
    ) -> bool {
        if marking.len() != self.place_count() {
            return false;
        }

        let mut demand: HashMap<PlaceId, u64> = HashMap::new();
        for &(transition, multiplicity) in transitions {
            if multiplicity == 0 {
                continue;
            }
            if !self.is_enabled(transition, marking) {
                return false;
            }
            for &(place, weight) in self.preset(transition) {
                let Some(amount) = (weight as u64).checked_mul(multiplicity as u64) else {
                    return false;
                };
                let entry = demand.entry(place).or_default();
                let Some(total) = entry.checked_add(amount) else {
                    return false;
                };
                *entry = total;
            }
        }

        demand
            .into_iter()
            .all(|(place, required)| match marking.get(place.0 as usize) {
                Some(TokenValue::Omega) => true,
                Some(TokenValue::Finite(available)) => required <= *available,
                Some(TokenValue::Exact(available)) => BigUint::from(required) <= *available,
                None => false,
            })
    }

    /// Whether an enabled transition multiset is in dynamic conflict because
    /// its combined input demand cannot be supplied by the marking.
    pub fn is_dynamic_conflict_multiset(
        &self,
        transitions: &[(TransitionId, u32)],
        marking: &Marking,
    ) -> bool {
        if transitions.is_empty() {
            return false;
        }
        let individually_enabled = transitions.iter().all(|&(transition, multiplicity)| {
            multiplicity == 0 || self.is_enabled(transition, marking)
        });
        individually_enabled && !self.is_concurrent_multiset(transitions, marking)
    }
}

// ---------------------------------------------------------------------------
// PetriNetBuilder
// ---------------------------------------------------------------------------

/// Builder for constructing a `PetriNet`.
///
/// # Example
///
/// ```ignore
/// let mut builder = PetriNetBuilder::default();
/// let p0 = builder.add_place("buffer".into(), None);
/// let t0 = builder.add_transition("produce".into(), None);
/// builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
/// let net = builder.build(vec![TokenValue::Finite(0)]).unwrap();
/// ```
#[derive(Debug, Default)]
pub struct PetriNetBuilder {
    places: Vec<Place>,
    transitions: Vec<Transition>,
    arcs: Vec<Arc>,
    inhibitors: Vec<InhibitorArc>,
    kinds: Vec<(TransitionId, TransitionKind)>,
}

impl PetriNetBuilder {
    /// Add a place with a name and optional label. Returns the assigned ID.
    pub fn add_place(&mut self, name: String, label: Option<String>) -> PlaceId {
        let id = PlaceId(self.places.len() as u32);
        self.places.push(Place { id, name, label });
        id
    }

    /// Add a transition with a name and optional label. Returns the assigned ID.
    pub fn add_transition(&mut self, name: String, label: Option<String>) -> TransitionId {
        let id = TransitionId(self.transitions.len() as u32);
        self.transitions.push(Transition { id, name, label });
        id
    }

    /// Add an arc between a place and a transition (in either direction).
    /// Weight must be > 0.
    pub fn add_arc(&mut self, source: ArcEndpoint, target: ArcEndpoint, weight: u32) {
        self.arcs.push(Arc {
            source,
            target,
            weight,
        });
    }

    /// Add an inhibitor arc: `transition` requires `marking[place] < threshold`.
    ///
    /// `threshold` must be >= 1 (0 means "no arc" and is rejected at `build`).
    /// Duplicate `(place, transition)` inhibitors are normalized to the
    /// **smallest** threshold (conjunction of tests); thresholds are never added.
    pub fn add_inhibitor_arc(&mut self, place: PlaceId, transition: TransitionId, threshold: u64) {
        self.inhibitors.push(InhibitorArc {
            place,
            transition,
            threshold,
        });
    }

    /// Set the kind of a transition. Transitions are `Normal` by default.
    pub fn set_transition_kind(&mut self, transition: TransitionId, kind: TransitionKind) {
        self.kinds.push((transition, kind));
    }

    /// Consume the builder and produce a validated `PetriNet`.
    ///
    /// # Errors
    /// Returns `RaconError::InvalidInput` if:
    /// - No places or no transitions or no arcs.
    /// - Arc weight is 0.
    /// - Arc connects Place ↔ Place or Transition ↔ Transition.
    /// - The marking length does not match the number of places.
    /// - The marking contains Omega values.
    pub fn build(self, initial_tokens: Vec<TokenValue>) -> Result<PetriNet, RaconError> {
        let place_count = self.places.len();
        let trans_count = self.transitions.len();

        // ----- structural validation -----
        if place_count == 0 {
            return Err(RaconError::InvalidInput("No places in net".into()));
        }
        if trans_count == 0 {
            return Err(RaconError::InvalidInput("No transitions in net".into()));
        }
        if self.arcs.is_empty() {
            return Err(RaconError::InvalidInput("No arcs in net".into()));
        }

        // Validate arcs
        let mut arc_endpoints = HashSet::new();
        for arc in &self.arcs {
            if arc.weight == 0 {
                return Err(RaconError::InvalidInput("Arc weight must be > 0".into()));
            }
            match (&arc.source, &arc.target) {
                (ArcEndpoint::Place(_), ArcEndpoint::Transition(_))
                | (ArcEndpoint::Transition(_), ArcEndpoint::Place(_)) => {}
                _ => {
                    return Err(RaconError::InvalidInput(
                        "Arc must connect Place ↔ Transition".into(),
                    ));
                }
            }
            if !arc_endpoints.insert((arc.source, arc.target)) {
                return Err(RaconError::InvalidInput(format!(
                    "Duplicate arc between {:?} and {:?}",
                    arc.source, arc.target
                )));
            }
            // Check endpoint IDs are within range
            match &arc.source {
                ArcEndpoint::Place(p) if p.0 as usize >= place_count => {
                    return Err(RaconError::InvalidInput(format!(
                        "Arc source PlaceId({}) out of range ({} places)",
                        p.0, place_count
                    )));
                }
                ArcEndpoint::Transition(t) if t.0 as usize >= trans_count => {
                    return Err(RaconError::InvalidInput(format!(
                        "Arc source TransitionId({}) out of range ({} transitions)",
                        t.0, trans_count
                    )));
                }
                _ => {}
            }
            match &arc.target {
                ArcEndpoint::Place(p) if p.0 as usize >= place_count => {
                    return Err(RaconError::InvalidInput(format!(
                        "Arc target PlaceId({}) out of range ({} places)",
                        p.0, place_count
                    )));
                }
                ArcEndpoint::Transition(t) if t.0 as usize >= trans_count => {
                    return Err(RaconError::InvalidInput(format!(
                        "Arc target TransitionId({}) out of range ({} transitions)",
                        t.0, trans_count
                    )));
                }
                _ => {}
            }
        }

        // ----- validate inhibitor arcs and transition kinds -----
        let mut inhibitor_map: std::collections::BTreeMap<(u32, u32), u64> =
            std::collections::BTreeMap::new();
        for inh in &self.inhibitors {
            if inh.place.0 as usize >= place_count {
                return Err(RaconError::InvalidInput(format!(
                    "Inhibitor arc PlaceId({}) out of range ({} places)",
                    inh.place.0, place_count
                )));
            }
            if inh.transition.0 as usize >= trans_count {
                return Err(RaconError::InvalidInput(format!(
                    "Inhibitor arc TransitionId({}) out of range ({} transitions)",
                    inh.transition.0, trans_count
                )));
            }
            if inh.threshold == 0 {
                return Err(RaconError::InvalidInput(
                    "Inhibitor arc threshold must be >= 1".into(),
                ));
            }
            inhibitor_map
                .entry((inh.transition.0, inh.place.0))
                .and_modify(|current| *current = (*current).min(inh.threshold))
                .or_insert(inh.threshold);
        }
        let inhibitor_arcs: Vec<InhibitorArc> = inhibitor_map
            .iter()
            .map(|(&(t, p), &threshold)| InhibitorArc {
                place: PlaceId(p),
                transition: TransitionId(t),
                threshold,
            })
            .collect();
        let mut transition_inhibitors: Vec<Vec<(PlaceId, u64)>> = vec![Vec::new(); trans_count];
        for inh in &inhibitor_arcs {
            transition_inhibitors[inh.transition.0 as usize].push((inh.place, inh.threshold));
        }

        let mut transition_kinds = vec![TransitionKind::Normal; trans_count];
        for &(transition, kind) in &self.kinds {
            if transition.0 as usize >= trans_count {
                return Err(RaconError::InvalidInput(format!(
                    "Transition kind assigned to TransitionId({}) out of range ({} transitions)",
                    transition.0, trans_count
                )));
            }
            transition_kinds[transition.0 as usize] = kind;
        }
        let immediate_transitions: Vec<TransitionId> = transition_kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| **kind == TransitionKind::Immediate)
            .map(|(index, _)| TransitionId(index as u32))
            .collect();

        // ----- validate marking -----
        if initial_tokens.len() != place_count {
            return Err(RaconError::InvalidInput(format!(
                "Initial marking size {} != place count {}",
                initial_tokens.len(),
                place_count,
            )));
        }
        if initial_tokens
            .iter()
            .any(|t| matches!(t, TokenValue::Omega))
        {
            return Err(RaconError::InvalidInput(
                "Omega not allowed in initial marking".into(),
            ));
        }
        let initial_marking = Marking::new(initial_tokens);

        // ----- pre-compute preset / postset for transitions -----
        let mut transition_preset: Vec<Vec<(PlaceId, u32)>> = vec![Vec::new(); trans_count];
        let mut transition_postset: Vec<Vec<(PlaceId, u32)>> = vec![Vec::new(); trans_count];

        for arc in &self.arcs {
            match (arc.source, arc.target, arc.weight) {
                (ArcEndpoint::Place(p), ArcEndpoint::Transition(t), w) => {
                    transition_preset[t.0 as usize].push((p, w));
                }
                (ArcEndpoint::Transition(t), ArcEndpoint::Place(p), w) => {
                    transition_postset[t.0 as usize].push((p, w));
                }
                _ => unreachable!("validated above"),
            }
        }

        // ----- build incidence matrix -----
        // Pre matrix: pre[i][j] = weight of arc from p_i to t_j (or 0)
        // Post matrix: post[i][j] = weight of arc from t_j to p_i (or 0)
        let mut pre = vec![vec![0u32; trans_count]; place_count];
        let mut post = vec![vec![0u32; trans_count]; place_count];

        for (t_idx, preset) in transition_preset.iter().enumerate() {
            for &(p, w) in preset {
                pre[p.0 as usize][t_idx] = w;
            }
        }
        for (t_idx, postset) in transition_postset.iter().enumerate() {
            for &(p, w) in postset {
                post[p.0 as usize][t_idx] = w;
            }
        }

        let incidence_matrix = IncidenceMatrix::new(place_count, trans_count, &pre, &post);

        let net = PetriNet {
            places: self.places,
            transitions: self.transitions,
            arcs: self.arcs,
            transition_preset,
            transition_postset,
            incidence_matrix,
            initial_marking,
            transition_kinds,
            inhibitor_arcs,
            transition_inhibitors,
            immediate_transitions,
        };

        Ok(net)
    }
}
