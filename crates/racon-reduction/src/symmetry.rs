//! Symmetry reduction for place/transition nets.
//!
//! Follows T. Junttila, *On the Symmetry Reduction Method for Petri Nets and
//! Similar Formalisms*, HUT-TCS-A80, Helsinki University of Technology, 2003
//! (Chapters 2–4). Reference numbers below are to that thesis.
//!
//! * **Symmetries (Def. 3.2).** A symmetry of a net is a permutation of
//!   `P ∪ T` that maps places to places, transitions to transitions and
//!   preserves the flow relation and the arc multiplicities. It acts on
//!   markings by `σ(M) = M ∘ σ⁻¹`, and by Lemma 3.3 (Starke) `M [t⟩ M'` iff
//!   `σ(M) [σ(t)⟩ σ(M')`.
//! * **Finding the group (§3.2, Thm 3.7).** The net, with the initial marking
//!   as a vertex label, is read as a directed, vertex- and edge-labelled graph;
//!   its automorphism group is the group stabilizing that marking. We search it
//!   with individualization–refinement (the "split and refine" scheme of §3.2.2)
//!   and **verify every generator against the net**, so the group used is
//!   always a subgroup of `Aut(N)`, as §2.2.2 allows.
//! * **Representation (§3.1.1, §4.1.1).** The group is stored as a
//!   Schreier–Sims representation over a base that lists all moved places
//!   before all moved transitions.
//! * **Canonical representative (§3.3.2, §4.3, Alg. 4.2).** `canonicalize`
//!   returns the `<β`-smallest marking in the orbit by depth-first search in
//!   the Schreier–Sims representation, with the thesis' pruning by the fixed
//!   prefix and by marking stabilizers.
//!
//! What a quotient built with these representatives preserves is stated in
//! §2.2.3 (Cor. 2.10–2.13) and in [`crate::catalog`].

use std::cmp::Ordering;
use std::collections::HashMap;

use racon_core::marking::{Marking, TokenValue};
use racon_core::net::PetriNet;

type Perm = Vec<u32>;

/// Upper bound on search-tree nodes visited while looking for generators.
/// Hitting it keeps the generators found so far (still a sound subgroup).
const SEARCH_BUDGET: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymmetryOptions {
    /// Only accept automorphisms that fix the initial marking (Cor. 2.13 then
    /// holds: a marking is reachable iff some equivalent one is in the quotient).
    pub respect_initial_marking: bool,
    /// Places that must remain fixed by every accepted automorphism.
    pub fixed_places: Vec<u32>,
    /// Transitions that must remain fixed by every accepted automorphism.
    pub fixed_transitions: Vec<u32>,
}

impl Default for SymmetryOptions {
    fn default() -> Self {
        Self {
            respect_initial_marking: true,
            fixed_places: Vec::new(),
            fixed_transitions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetryGeneratorKind {
    PlaceTransposition,
    TransitionTransposition,
    /// Any other automorphism (rotations, swaps of whole subnets, …).
    Automorphism,
}

impl SymmetryGeneratorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlaceTransposition => "PLACE_TRANSPOSITION",
            Self::TransitionTransposition => "TRANSITION_TRANSPOSITION",
            Self::Automorphism => "AUTOMORPHISM",
        }
    }
}

/// One generator of the symmetry group.
///
/// `permutation[i]` is the image of node `i`, where places are numbered
/// `0..places` and transitions `places..places + transitions`.
/// `first`/`second` are the first moved node and its image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymmetryGenerator {
    pub kind: SymmetryGeneratorKind,
    pub first: u32,
    pub second: u32,
    pub permutation: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymmetryReport {
    pub generators: Vec<SymmetryGenerator>,
    pub generator_count: usize,
    /// `|Aut(N)|` (or of the stabilizer of the initial marking), saturating.
    pub group_size: u128,
    pub place_equivalence_classes: Vec<Vec<u32>>,
    pub transition_equivalence_classes: Vec<Vec<u32>>,
    /// `false` when the generator search hit its budget; the group is then a
    /// proper or equal subgroup of `Aut(N)` and the quotient may be larger
    /// than necessary, but remains sound.
    pub complete: bool,
}

impl SymmetryReport {
    fn trivial(complete: bool) -> Self {
        Self {
            generators: Vec::new(),
            generator_count: 0,
            group_size: 1,
            place_equivalence_classes: Vec::new(),
            transition_equivalence_classes: Vec::new(),
            complete,
        }
    }
}

/// Precomputed symmetry of one net, able to canonicalize markings.
#[derive(Debug, Clone)]
pub struct Symmetry {
    chain: Chain,
    /// Number of leading base points that are places.
    place_levels: usize,
    report: SymmetryReport,
}

impl Symmetry {
    /// Compute the symmetry group (§3.2) and its Schreier–Sims representation (§4.1.1).
    pub fn compute(net: &PetriNet, options: &SymmetryOptions) -> Self {
        let graph = Graph::from_net(net, options);
        let (generators, complete) = find_automorphisms(&graph);
        let places = graph.places;
        let nodes = graph.n;

        if generators.is_empty() {
            return Self {
                chain: Chain::empty(nodes),
                place_levels: 0,
                report: SymmetryReport::trivial(complete),
            };
        }

        // Base: moved places first, then moved transitions (§4.3 requires places before transitions).
        let mut moved = vec![false; nodes];
        for generator in &generators {
            for (node, &image) in generator.iter().enumerate() {
                if image as usize != node {
                    moved[node] = true;
                }
            }
        }
        let base: Vec<u32> = (0..nodes).filter(|&v| moved[v]).map(|v| v as u32).collect();
        let place_levels = base.iter().take_while(|&&v| (v as usize) < places).count();

        let chain = Chain::build(nodes, base, &generators);

        let mut place_union = UnionFind::new(places);
        let mut transition_union = UnionFind::new(nodes - places);
        for generator in &generators {
            for (node, &image) in generator.iter().enumerate() {
                if node < places {
                    place_union.union(node, image as usize);
                } else {
                    transition_union.union(node - places, image as usize - places);
                }
            }
        }

        let report = SymmetryReport {
            generator_count: generators.len(),
            generators: generators
                .iter()
                .map(|permutation| describe_generator(permutation, places))
                .collect(),
            group_size: chain.order(),
            place_equivalence_classes: place_union.non_trivial_components(0),
            transition_equivalence_classes: transition_union.non_trivial_components(places as u32),
            complete,
        };

        Self {
            chain,
            place_levels,
            report,
        }
    }

    pub fn is_trivial(&self) -> bool {
        self.report.generator_count == 0
    }

    pub fn report(&self) -> &SymmetryReport {
        &self.report
    }

    pub fn place_equivalence_classes(&self) -> &[Vec<u32>] {
        &self.report.place_equivalence_classes
    }

    pub fn transition_equivalence_classes(&self) -> &[Vec<u32>] {
        &self.report.transition_equivalence_classes
    }

    /// The canonical representative `canrepr(M)` of §4.3: the `<β`-smallest
    /// marking in the orbit of `marking`, where `β` lists the base places first
    /// and the remaining places by index (they are fixed by the whole group).
    pub fn canonicalize(&self, marking: &Marking) -> Marking {
        if self.place_levels == 0 {
            return marking.clone();
        }
        let mut search = CanonSearch {
            chain: &self.chain,
            levels: self.place_levels,
            marking: &marking.tokens,
            best: None,
            best_path: Vec::new(),
        };
        let identity: Perm = (0..self.chain.n as u32).collect();
        let mut values = Vec::with_capacity(self.place_levels);
        let mut path = Vec::with_capacity(self.place_levels);
        search.backtrack(0, &identity, &mut values, &mut path);

        let mut tokens = marking.tokens.clone();
        if let Some(best) = search.best {
            for (level, value) in best.into_iter().enumerate() {
                tokens[self.chain.base[level] as usize] = value;
            }
        }
        Marking::new(tokens)
    }
}

// ───────────────────────── the net as a labelled graph ─────────────────────────

struct Graph {
    n: usize,
    places: usize,
    /// Initial vertex class (canonical rank), equal for vertices that may be exchanged.
    initial: Vec<u32>,
    /// `(neighbour, weight)`, sorted.
    out_adj: Vec<Vec<(u32, u32)>>,
    in_adj: Vec<Vec<(u32, u32)>>,
}

impl Graph {
    /// The labelled graph of Thm 3.7: vertices `P ∪ T`, edges `F`, place labels
    /// from the marking, transition label `"T"`, edge labels `W`.
    fn from_net(net: &PetriNet, options: &SymmetryOptions) -> Self {
        let places = net.place_count();
        let transitions = net.transition_count();
        let n = places + transitions;

        let mut out_adj = vec![Vec::new(); n];
        let mut in_adj = vec![Vec::new(); n];
        for transition in net.transitions() {
            let t = places + transition.id.0 as usize;
            for &(place, weight) in net.preset(transition.id) {
                let p = place.0 as usize;
                out_adj[p].push((t as u32, weight));
                in_adj[t].push((p as u32, weight));
            }
            for &(place, weight) in net.postset(transition.id) {
                let p = place.0 as usize;
                out_adj[t].push((p as u32, weight));
                in_adj[p].push((t as u32, weight));
            }
        }
        for list in out_adj.iter_mut().chain(in_adj.iter_mut()) {
            list.sort_unstable();
        }

        // Distinct token values, so equal markings get equal labels.
        let tokens = &net.initial_marking().tokens;
        let mut distinct: Vec<&TokenValue> = Vec::new();
        let mut token_class = vec![0u64; places];
        for (p, value) in tokens.iter().enumerate().take(places) {
            let index = distinct.iter().position(|seen| *seen == value).unwrap_or_else(|| {
                distinct.push(value);
                distinct.len() - 1
            });
            token_class[p] = index as u64;
        }

        // Class keys: (kind, marking label, unique id for fixed vertices).
        let mut keys: Vec<(u8, u64, u64)> = Vec::with_capacity(n);
        for p in 0..places {
            keys.push(if options.fixed_places.contains(&(p as u32)) {
                (2, 0, p as u64)
            } else if options.respect_initial_marking {
                (0, token_class[p], 0)
            } else {
                (0, 0, 0)
            });
        }
        for t in 0..transitions {
            keys.push(if options.fixed_transitions.contains(&(t as u32)) {
                (3, 0, t as u64)
            } else {
                (1, 0, 0)
            });
        }
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        let initial = keys
            .iter()
            .map(|key| sorted.binary_search(key).expect("key present") as u32)
            .collect();

        Self {
            n,
            places,
            initial,
            out_adj,
            in_adj,
        }
    }

    /// Whether `permutation` is a symmetry: it keeps node classes and maps every
    /// weighted arc onto a weighted arc (Def. 3.2).
    fn is_automorphism(&self, permutation: &[u32]) -> bool {
        for v in 0..self.n {
            let image = permutation[v] as usize;
            if self.initial[v] != self.initial[image] || self.out_adj[v].len() != self.out_adj[image].len() {
                return false;
            }
            for &(u, weight) in &self.out_adj[v] {
                let target = permutation[u as usize];
                if self.out_adj[image].binary_search(&(target, weight)).is_err() {
                    return false;
                }
            }
        }
        true
    }
}

/// Equitable refinement with canonical colour ranks: colours depend only on the
/// structure, never on vertex numbering, so isomorphic branches compare equal.
fn refine(graph: &Graph, mut colors: Vec<u32>) -> Vec<u32> {
    let mut classes = {
        let mut distinct = colors.clone();
        distinct.sort_unstable();
        distinct.dedup();
        distinct.len()
    };
    loop {
        let mut signatures: Vec<(u32, Vec<(u8, u32, u32)>)> = (0..graph.n)
            .map(|v| {
                let mut around: Vec<(u8, u32, u32)> = Vec::with_capacity(graph.out_adj[v].len() + graph.in_adj[v].len());
                around.extend(graph.out_adj[v].iter().map(|&(u, w)| (0u8, w, colors[u as usize])));
                around.extend(graph.in_adj[v].iter().map(|&(u, w)| (1u8, w, colors[u as usize])));
                around.sort_unstable();
                (colors[v], around)
            })
            .collect();
        let mut order: Vec<usize> = (0..graph.n).collect();
        order.sort_by(|&a, &b| signatures[a].cmp(&signatures[b]));
        let mut next = vec![0u32; graph.n];
        let mut rank = 0u32;
        for window in 0..order.len() {
            if window > 0 && signatures[order[window]] != signatures[order[window - 1]] {
                rank += 1;
            }
            next[order[window]] = rank;
        }
        signatures.clear();
        let new_classes = rank as usize + 1;
        colors = next;
        if new_classes == classes {
            return colors;
        }
        classes = new_classes;
    }
}

fn individualize(graph: &Graph, colors: &[u32], vertex: u32) -> Vec<u32> {
    let doubled: Vec<u32> = colors
        .iter()
        .enumerate()
        .map(|(v, &c)| 2 * c + u32::from(v as u32 != vertex))
        .collect();
    refine(graph, doubled)
}

/// Vertices of the first non-singleton cell, or `None` for a discrete colouring.
fn target_cell(colors: &[u32]) -> Option<Vec<u32>> {
    let mut sizes: HashMap<u32, usize> = HashMap::new();
    for &c in colors {
        *sizes.entry(c).or_default() += 1;
    }
    let color = colors.iter().copied().filter(|c| sizes[c] > 1).min()?;
    Some(
        colors
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c == color)
            .map(|(v, _)| v as u32)
            .collect(),
    )
}

/// Class sizes in colour order; equal for corresponding nodes of equivalent branches.
fn invariant(colors: &[u32]) -> Vec<u32> {
    let classes = colors.iter().max().map_or(0, |&c| c as usize + 1);
    let mut sizes = vec![0u32; classes];
    for &c in colors {
        sizes[c as usize] += 1;
    }
    sizes
}

struct PathNode {
    colors: Vec<u32>,
    cell: Vec<u32>,
    chosen: u32,
    invariant: Vec<u32>,
}

struct GeneratorSearch<'a> {
    graph: &'a Graph,
    path: Vec<PathNode>,
    leaf_inverse: Vec<u32>,
    visited: usize,
    exhausted: bool,
}

impl GeneratorSearch<'_> {
    /// Look below `colors` (at path depth `depth`) for a discrete colouring that
    /// yields an automorphism mapping the first leaf onto it.
    fn descend(&mut self, colors: &[u32], depth: usize) -> Option<Perm> {
        if self.visited >= SEARCH_BUDGET {
            self.exhausted = true;
            return None;
        }
        self.visited += 1;
        let Some(cell) = target_cell(colors) else {
            let permutation: Perm = colors.iter().map(|&c| self.leaf_inverse[c as usize]).collect();
            return self.graph.is_automorphism(&permutation).then_some(permutation);
        };
        for vertex in cell {
            let child = individualize(self.graph, colors, vertex);
            match self.path.get(depth + 1) {
                Some(expected) if expected.invariant == invariant(&child) => {}
                _ => continue,
            }
            if let Some(found) = self.descend(&child, depth + 1) {
                return Some(found);
            }
            if self.exhausted {
                return None;
            }
        }
        None
    }
}

/// Generators of the automorphism group of the labelled graph, and whether the
/// search finished. Every returned permutation has been verified.
fn find_automorphisms(graph: &Graph) -> (Vec<Perm>, bool) {
    // First path: always individualize the first vertex of the target cell.
    let mut colors = refine(graph, graph.initial.clone());
    let mut path: Vec<PathNode> = Vec::new();
    loop {
        match target_cell(&colors) {
            None => {
                path.push(PathNode {
                    invariant: invariant(&colors),
                    colors: colors.clone(),
                    cell: Vec::new(),
                    chosen: 0,
                });
                break;
            }
            Some(cell) => {
                let chosen = cell[0];
                path.push(PathNode {
                    invariant: invariant(&colors),
                    colors: colors.clone(),
                    cell,
                    chosen,
                });
                colors = individualize(graph, &colors, chosen);
            }
        }
    }
    let depth = path.len() - 1;
    if depth == 0 {
        return (Vec::new(), true);
    }

    let mut leaf_inverse = vec![0u32; graph.n];
    for (v, &c) in path[depth].colors.iter().enumerate() {
        leaf_inverse[c as usize] = v as u32;
    }

    let mut search = GeneratorSearch {
        graph,
        path,
        leaf_inverse,
        visited: 0,
        exhausted: false,
    };
    let mut generators: Vec<Perm> = Vec::new();

    for level in (0..depth).rev() {
        let (cell, chosen, colors) = {
            let node = &search.path[level];
            (node.cell.clone(), node.chosen, node.colors.clone())
        };
        // Generators found at deeper levels fix the first `level` path vertices.
        let mut orbits = UnionFind::new(graph.n);
        for generator in &generators {
            for (v, &image) in generator.iter().enumerate() {
                orbits.union(v, image as usize);
            }
        }
        for &candidate in &cell {
            if candidate == chosen || orbits.find(candidate as usize) == orbits.find(chosen as usize) {
                continue;
            }
            let child = individualize(graph, &colors, candidate);
            if search.path[level + 1].invariant != invariant(&child) {
                continue;
            }
            if let Some(found) = search.descend(&child, level + 1) {
                for (v, &image) in found.iter().enumerate() {
                    orbits.union(v, image as usize);
                }
                generators.push(found);
            }
            if search.exhausted {
                return (generators, false);
            }
        }
    }
    (generators, true)
}

fn describe_generator(permutation: &[u32], places: usize) -> SymmetryGenerator {
    let moved: Vec<usize> = permutation
        .iter()
        .enumerate()
        .filter(|&(v, &image)| image as usize != v)
        .map(|(v, _)| v)
        .collect();
    let first = moved.first().copied().unwrap_or(0);
    let second = permutation[first];
    let is_swap = moved.len() == 2 && permutation[second as usize] as usize == first;
    let kind = match (is_swap, first < places) {
        (true, true) => SymmetryGeneratorKind::PlaceTransposition,
        (true, false) => SymmetryGeneratorKind::TransitionTransposition,
        (false, _) => SymmetryGeneratorKind::Automorphism,
    };
    SymmetryGenerator {
        kind,
        first: first as u32,
        second,
        permutation: permutation.to_vec(),
    }
}

// ───────────────────────── Schreier–Sims representation ─────────────────────────

fn compose(outer: &[u32], inner: &[u32]) -> Perm {
    inner.iter().map(|&x| outer[x as usize]).collect()
}

fn inverse(permutation: &[u32]) -> Perm {
    let mut result = vec![0u32; permutation.len()];
    for (x, &image) in permutation.iter().enumerate() {
        result[image as usize] = x as u32;
    }
    result
}

/// Stabilizer chain `G = G⁰ ≥ G¹ ≥ …` over a fixed base (§4.1.1). `Gⁱ` fixes
/// `base[0..i]`; `transversal[i]` maps each point `β` of the orbit of `base[i]`
/// under `Gⁱ` to an element `u_β ∈ Gⁱ` with `u_β(base[i]) = β`.
#[derive(Debug, Clone)]
struct Chain {
    n: usize,
    base: Vec<u32>,
    generators: Vec<Vec<Perm>>,
    transversal: Vec<HashMap<u32, Perm>>,
}

impl Chain {
    fn empty(n: usize) -> Self {
        Self {
            n,
            base: Vec::new(),
            generators: Vec::new(),
            transversal: Vec::new(),
        }
    }

    /// Schreier–Sims with a prescribed base that contains every moved point.
    fn build(n: usize, base: Vec<u32>, generators: &[Perm]) -> Self {
        let levels = base.len();
        let mut chain = Self {
            n,
            base,
            generators: vec![Vec::new(); levels],
            transversal: vec![HashMap::new(); levels],
        };
        for generator in generators {
            if let Some(level) = chain.first_moved_level(generator) {
                for j in 0..=level {
                    chain.generators[j].push(generator.clone());
                }
            }
        }
        for level in 0..levels {
            chain.recompute_orbit(level);
        }

        // Close under Schreier generators, from the deepest level up; whenever a
        // sifted residue is non-trivial it becomes a new strong generator.
        let mut level = levels;
        while level > 0 {
            level -= 1;
            if let Some(restart) = chain.close_level(level) {
                level = restart + 1;
            }
        }
        chain
    }

    fn first_moved_level(&self, permutation: &[u32]) -> Option<usize> {
        self.base
            .iter()
            .position(|&b| permutation[b as usize] != b)
    }

    fn recompute_orbit(&mut self, level: usize) {
        let origin = self.base[level];
        let mut orbit: HashMap<u32, Perm> = HashMap::new();
        orbit.insert(origin, (0..self.n as u32).collect());
        let mut queue = vec![origin];
        while let Some(point) = queue.pop() {
            let reach = orbit[&point].clone();
            for generator in &self.generators[level] {
                let image = generator[point as usize];
                if !orbit.contains_key(&image) {
                    orbit.insert(image, compose(generator, &reach));
                    queue.push(image);
                }
            }
        }
        self.transversal[level] = orbit;
    }

    /// Sift `permutation` through the levels `from..`; returns the residue and the
    /// level at which it left the chain, or `None` if it belongs to it.
    fn sift(&self, mut permutation: Perm, from: usize) -> Option<(Perm, usize)> {
        for level in from..self.base.len() {
            let image = permutation[self.base[level] as usize];
            match self.transversal[level].get(&image) {
                Some(rep) => permutation = compose(&inverse(rep), &permutation),
                None => return Some((permutation, level)),
            }
        }
        None
    }

    /// Check the Schreier generators of `level`; on a failure add the residue and
    /// return the deepest level that changed.
    fn close_level(&mut self, level: usize) -> Option<usize> {
        let points: Vec<(u32, Perm)> = self.transversal[level]
            .iter()
            .map(|(&p, u)| (p, u.clone()))
            .collect();
        let generators = self.generators[level].clone();
        for (point, rep) in &points {
            for generator in &generators {
                let image = generator[*point as usize];
                let image_rep = &self.transversal[level][&image];
                let schreier = compose(&inverse(image_rep), &compose(generator, rep));
                if let Some((residue, failed)) = self.sift(schreier, level + 1) {
                    for j in (level + 1)..=failed {
                        self.generators[j].push(residue.clone());
                    }
                    for j in (level + 1)..=failed {
                        self.recompute_orbit(j);
                    }
                    return Some(failed);
                }
            }
        }
        None
    }

    fn order(&self) -> u128 {
        self.transversal
            .iter()
            .fold(1u128, |acc, orbit| acc.saturating_mul(orbit.len() as u128))
    }
}

// ───────────────────── canonical representative (Algorithm 4.2) ─────────────────────

struct CanonSearch<'a> {
    chain: &'a Chain,
    /// Number of leading place levels; transition levels cannot change a marking.
    levels: usize,
    marking: &'a [TokenValue],
    best: Option<Vec<TokenValue>>,
    best_path: Vec<usize>,
}

impl CanonSearch<'_> {
    /// `ĝ` is `h₀ ∘ … ∘ h_{l-1}`. At level `l` the value given to `base[l]` is
    /// `M(ĝ(h(base[l])))` for `h ∈ U_l`, so only transversal elements that
    /// minimize it can lead to the smallest marking (the minimal-element
    /// "select" of Alg. 4.2). Returns the level whose current child should be
    /// abandoned when a marking equal to the best one is reached again.
    fn backtrack(
        &mut self,
        level: usize,
        prefix: &Perm,
        values: &mut Vec<TokenValue>,
        path: &mut Vec<usize>,
    ) -> Option<usize> {
        if level == self.levels {
            return self.reach_leaf(values, path);
        }

        // Prune with the fixed prefix: it must not exceed the best one so far.
        let mut tight = false;
        if let Some(best) = &self.best {
            match compare_values(values, &best[..level]) {
                Ordering::Greater => return None,
                Ordering::Equal => tight = true,
                Ordering::Less => {}
            }
        }

        let chain = self.chain;
        let marking = self.marking;
        let mut candidates: Vec<(usize, &TokenValue, &Perm)> = Vec::new();
        let mut entries: Vec<(&u32, &Perm)> = chain.transversal[level].iter().collect();
        entries.sort_by_key(|(point, _)| **point);
        let mut minimum: Option<&TokenValue> = None;
        for (index, (&point, rep)) in entries.iter().enumerate() {
            let value = &marking[prefix[point as usize] as usize];
            match minimum {
                Some(current) if token_order(value, current) != Ordering::Less => {}
                _ => minimum = Some(value),
            }
            candidates.push((index, value, rep));
        }
        let minimum = minimum?.clone();
        if tight {
            if let Some(best) = &self.best {
                if token_order(&minimum, &best[level]) == Ordering::Greater {
                    return None;
                }
            }
        }

        for (index, value, rep) in candidates {
            if token_order(value, &minimum) != Ordering::Equal {
                continue;
            }
            let extended = compose(prefix, rep);
            values.push(minimum.clone());
            path.push(index);
            let outcome = self.backtrack(level + 1, &extended, values, path);
            values.pop();
            path.pop();
            if let Some(target) = outcome {
                if target < level {
                    return Some(target);
                }
            }
        }
        None
    }

    fn reach_leaf(&mut self, values: &[TokenValue], path: &[usize]) -> Option<usize> {
        match &self.best {
            None => {
                self.best = Some(values.to_vec());
                self.best_path = path.to_vec();
                None
            }
            Some(best) => match compare_values(values, best) {
                Ordering::Less => {
                    self.best = Some(values.to_vec());
                    self.best_path = path.to_vec();
                    None
                }
                Ordering::Equal => {
                    // Two paths give the same marking, so their product is a
                    // stabilizer of M and the subtree below the first level at
                    // which they differ yields nothing new (§4.3).
                    path.iter()
                        .zip(&self.best_path)
                        .position(|(a, b)| a != b)
                }
                Ordering::Greater => None,
            },
        }
    }
}

fn compare_values(left: &[TokenValue], right: &[TokenValue]) -> Ordering {
    for (a, b) in left.iter().zip(right) {
        match token_order(a, b) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

fn token_order(left: &TokenValue, right: &TokenValue) -> Ordering {
    if left == right {
        Ordering::Equal
    } else if left.geq(right) {
        Ordering::Greater
    } else {
        Ordering::Less
    }
}

/// Minimal union-find used to build orbit components.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, first: usize, second: usize) {
        let root_first = self.find(first);
        let root_second = self.find(second);
        if root_first != root_second {
            self.parent[root_second] = root_first;
        }
    }

    /// Components with more than one element, with `offset` added to every id.
    fn non_trivial_components(&mut self, offset: u32) -> Vec<Vec<u32>> {
        let mut buckets: HashMap<usize, Vec<u32>> = HashMap::new();
        for node in 0..self.parent.len() {
            let root = self.find(node);
            buckets.entry(root).or_default().push(node as u32 + offset);
        }
        let mut components: Vec<Vec<u32>> = buckets
            .into_values()
            .filter(|component| component.len() > 1)
            .collect();
        components.sort();
        components
    }
}

/// `COMPUTE_SYMMETRIES` operation from the protocol.
pub fn compute_symmetries(net: &PetriNet, options: &SymmetryOptions) -> SymmetryReport {
    Symmetry::compute(net, options).report().clone()
}

/// Partial convenience constructors for the protocol DTOs.
pub fn transposition_generator_pairs(report: &SymmetryReport) -> Vec<(u32, u32)> {
    report
        .generators
        .iter()
        .map(|generator| (generator.first, generator.second))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_core::net::{ArcEndpoint, PetriNetBuilder};

    /// Two places with identical incidence (both consume from and produce into
    /// the same transition).
    fn twin_places_net(initial: [u64; 2]) -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let p0 = builder.add_place("p0".into(), None);
        let p1 = builder.add_place("p1".into(), None);
        let t0 = builder.add_transition("t0".into(), None);
        builder.add_arc(ArcEndpoint::Place(p0), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Place(p1), ArcEndpoint::Transition(t0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p0), 1);
        builder.add_arc(ArcEndpoint::Transition(t0), ArcEndpoint::Place(p1), 1);
        builder
            .build(vec![TokenValue::Finite(initial[0]), TokenValue::Finite(initial[1])])
            .unwrap()
    }

    /// `ring(n)`: places p0..p{n-1}, transitions t_i moving a token p_i → p_{i+1 mod n}.
    /// Its group is the cyclic rotation group (order n) — it moves places *and*
    /// transitions together, which transposition-only symmetry cannot see.
    fn ring_net(n: usize, tokens: &[u64]) -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let places: Vec<_> = (0..n).map(|i| builder.add_place(format!("p{i}"), None)).collect();
        for i in 0..n {
            let t = builder.add_transition(format!("t{i}"), None);
            builder.add_arc(ArcEndpoint::Place(places[i]), ArcEndpoint::Transition(t), 1);
            builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(places[(i + 1) % n]), 1);
        }
        builder
            .build(tokens.iter().map(|&k| TokenValue::Finite(k)).collect())
            .unwrap()
    }

    /// Three interchangeable processes, each with its own idle/busy places and
    /// two transitions, sharing a mutex place. `Aut = S3`.
    fn mutex_net(processes: usize) -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let mutex = builder.add_place("mutex".into(), None);
        let mut initial = vec![TokenValue::Finite(1)];
        for i in 0..processes {
            let idle = builder.add_place(format!("idle{i}"), None);
            let busy = builder.add_place(format!("busy{i}"), None);
            let enter = builder.add_transition(format!("enter{i}"), None);
            let leave = builder.add_transition(format!("leave{i}"), None);
            builder.add_arc(ArcEndpoint::Place(idle), ArcEndpoint::Transition(enter), 1);
            builder.add_arc(ArcEndpoint::Place(mutex), ArcEndpoint::Transition(enter), 1);
            builder.add_arc(ArcEndpoint::Transition(enter), ArcEndpoint::Place(busy), 1);
            builder.add_arc(ArcEndpoint::Place(busy), ArcEndpoint::Transition(leave), 1);
            builder.add_arc(ArcEndpoint::Transition(leave), ArcEndpoint::Place(idle), 1);
            builder.add_arc(ArcEndpoint::Transition(leave), ArcEndpoint::Place(mutex), 1);
            initial.push(TokenValue::Finite(1));
            initial.push(TokenValue::Finite(0));
        }
        builder.build(initial).unwrap()
    }

    #[test]
    fn asymmetric_initial_marking_breaks_the_symmetry() {
        let net = twin_places_net([2, 0]);
        let symmetry = Symmetry::compute(&net, &SymmetryOptions::default());
        assert!(symmetry.is_trivial());
        assert_eq!(symmetry.report().group_size, 1);
    }

    #[test]
    fn canonicalization_sorts_within_an_orbit() {
        let net = twin_places_net([0, 0]);
        let symmetry = Symmetry::compute(&net, &SymmetryOptions::default());
        assert!(!symmetry.is_trivial());
        assert_eq!(symmetry.report().group_size, 2);
        let marking = Marking::from_finite(&[3, 1]);
        assert_eq!(symmetry.canonicalize(&marking), Marking::from_finite(&[1, 3]));
        let canonical = Marking::from_finite(&[1, 3]);
        assert_eq!(symmetry.canonicalize(&canonical), canonical);
    }

    #[test]
    fn fixed_places_are_excluded_from_the_group() {
        let net = twin_places_net([0, 0]);
        let options = SymmetryOptions {
            respect_initial_marking: true,
            fixed_places: vec![0],
            fixed_transitions: Vec::new(),
        };
        let symmetry = Symmetry::compute(&net, &options);
        assert!(symmetry.is_trivial());
    }

    #[test]
    fn respect_initial_marking_false_keeps_structural_symmetry() {
        let net = twin_places_net([2, 0]);
        let options = SymmetryOptions {
            respect_initial_marking: false,
            ..SymmetryOptions::default()
        };
        let symmetry = Symmetry::compute(&net, &options);
        assert!(!symmetry.is_trivial());
        assert_eq!(symmetry.report().group_size, 2);
    }

    #[test]
    fn report_lists_generators_and_equivalence_classes() {
        let net = twin_places_net([0, 0]);
        let report = compute_symmetries(&net, &SymmetryOptions::default());
        assert_eq!(report.generator_count, 1);
        assert_eq!(report.place_equivalence_classes, vec![vec![0, 1]]);
        assert!(report.complete);
    }

    #[test]
    fn finds_rotations_that_move_places_and_transitions_together() {
        // Thesis Ex. 3.4 uses exactly this kind of symmetry (σ_rot).
        let net = ring_net(5, &[0; 5]);
        let report = compute_symmetries(&net, &SymmetryOptions::default());
        assert_eq!(report.group_size, 5, "rotation group of a 5-ring");
        assert!(report.complete);
        assert_eq!(report.place_equivalence_classes, vec![vec![0, 1, 2, 3, 4]]);
        assert_eq!(report.transition_equivalence_classes, vec![vec![5, 6, 7, 8, 9]]);
    }

    #[test]
    fn every_reported_generator_is_a_symmetry_of_the_net() {
        let net = mutex_net(3);
        let options = SymmetryOptions::default();
        let graph = Graph::from_net(&net, &options);
        let report = compute_symmetries(&net, &options);
        assert!(report.generator_count >= 2);
        for generator in &report.generators {
            assert!(graph.is_automorphism(&generator.permutation));
        }
    }

    #[test]
    fn symmetric_group_order_matches_the_factorial() {
        let net = mutex_net(3);
        let report = compute_symmetries(&net, &SymmetryOptions::default());
        assert_eq!(report.group_size, 6);
        let net = mutex_net(4);
        let report = compute_symmetries(&net, &SymmetryOptions::default());
        assert_eq!(report.group_size, 24);
    }

    /// Orbit of `marking` by brute force over the group elements enumerated from
    /// the generators (closure), for small groups only.
    fn brute_orbit(net: &PetriNet, symmetry: &Symmetry, marking: &Marking) -> Vec<Marking> {
        let places = net.place_count();
        let mut perms: Vec<Perm> = vec![(0..(places + net.transition_count()) as u32).collect()];
        let generators: Vec<Perm> = symmetry.report().generators.iter().map(|g| g.permutation.clone()).collect();
        let mut index = 0;
        while index < perms.len() {
            for g in &generators {
                let next = compose(g, &perms[index]);
                if !perms.contains(&next) {
                    perms.push(next);
                }
            }
            index += 1;
        }
        let mut orbit: Vec<Marking> = Vec::new();
        for perm in &perms {
            // σ(M)(σ(p)) = M(p)
            let mut tokens = marking.tokens.clone();
            for p in 0..places {
                tokens[perm[p] as usize] = marking.tokens[p].clone();
            }
            let image = Marking::new(tokens);
            if !orbit.contains(&image) {
                orbit.push(image);
            }
        }
        orbit
    }

    #[test]
    fn canonical_representative_is_the_orbit_minimum_and_orbit_invariant() {
        let net = ring_net(4, &[0, 0, 0, 0]);
        let symmetry = Symmetry::compute(&net, &SymmetryOptions::default());
        for tokens in [[1u64, 0, 2, 0], [0, 3, 0, 1], [2, 2, 0, 1], [0, 0, 0, 0], [1, 1, 1, 1]] {
            let marking = Marking::from_finite(&tokens);
            let orbit = brute_orbit(&net, &symmetry, &marking);
            let representative = symmetry.canonicalize(&marking);
            assert!(orbit.contains(&representative), "{tokens:?}");
            for other in &orbit {
                assert_eq!(symmetry.canonicalize(other), representative, "{tokens:?}");
            }
            let smallest = orbit
                .iter()
                .min_by(|a, b| compare_values(&a.tokens, &b.tokens))
                .unwrap();
            // Base order is the order of moved places, here the index order.
            assert_eq!(&representative, smallest, "{tokens:?}");
        }
    }

    #[test]
    fn mutex_markings_in_one_orbit_share_a_representative() {
        let net = mutex_net(3);
        let symmetry = Symmetry::compute(&net, &SymmetryOptions::default());
        // mutex, then (idle_i, busy_i) per process: process 0 busy vs process 2 busy.
        let a = Marking::from_finite(&[0, 0, 1, 1, 0, 1, 0]);
        let b = Marking::from_finite(&[0, 1, 0, 1, 0, 0, 1]);
        let c = Marking::from_finite(&[0, 1, 0, 0, 1, 1, 0]);
        assert_eq!(symmetry.canonicalize(&a), symmetry.canonicalize(&b));
        assert_eq!(symmetry.canonicalize(&a), symmetry.canonicalize(&c));
        let different = Marking::from_finite(&[1, 1, 0, 1, 0, 1, 0]);
        assert_ne!(symmetry.canonicalize(&a), symmetry.canonicalize(&different));
    }

    #[test]
    fn a_fully_symmetric_marking_does_not_blow_up_the_search() {
        // 8 interchangeable places, all-equal marking: every branch ties.
        let mut builder = PetriNetBuilder::default();
        let t = builder.add_transition("t".into(), None);
        for i in 0..8 {
            let p = builder.add_place(format!("p{i}"), None);
            builder.add_arc(ArcEndpoint::Place(p), ArcEndpoint::Transition(t), 1);
            builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(p), 1);
        }
        let net = builder.build(vec![TokenValue::Finite(0); 8]).unwrap();
        let symmetry = Symmetry::compute(&net, &SymmetryOptions::default());
        assert_eq!(symmetry.report().group_size, 40320);
        let marking = Marking::from_finite(&[1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(symmetry.canonicalize(&marking), marking);
        let spread = Marking::from_finite(&[0, 5, 0, 5, 1, 1, 0, 2]);
        assert_eq!(symmetry.canonicalize(&spread), Marking::from_finite(&[0, 0, 0, 1, 1, 2, 5, 5]));
    }

    /// Genrich's railroad net of the thesis (Ex. 3.1): trains a and b on a ring of
    /// six segments, `ta_i: Ua_i + V_{i+1} → Ua_{i+1} + V_{i-1}`.
    fn railroad_net() -> PetriNet {
        let mut builder = PetriNetBuilder::default();
        let ua: Vec<_> = (0..6).map(|i| builder.add_place(format!("Ua{i}"), None)).collect();
        let ub: Vec<_> = (0..6).map(|i| builder.add_place(format!("Ub{i}"), None)).collect();
        let v: Vec<_> = (0..6).map(|i| builder.add_place(format!("V{i}"), None)).collect();
        for (train, units) in [("a", &ua), ("b", &ub)] {
            for i in 0..6 {
                let t = builder.add_transition(format!("t{train}{i}"), None);
                builder.add_arc(ArcEndpoint::Place(units[i]), ArcEndpoint::Transition(t), 1);
                builder.add_arc(ArcEndpoint::Place(v[(i + 1) % 6]), ArcEndpoint::Transition(t), 1);
                builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(units[(i + 1) % 6]), 1);
                builder.add_arc(ArcEndpoint::Transition(t), ArcEndpoint::Place(v[(i + 5) % 6]), 1);
            }
        }
        let mut initial = vec![0u64; 18];
        initial[0] = 1; // Ua0
        initial[6 + 3] = 1; // Ub3
        initial[12 + 1] = 1; // V1
        initial[12 + 4] = 1; // V4
        builder.build(initial.into_iter().map(TokenValue::Finite).collect()).unwrap()
    }

    fn reachable(net: &PetriNet, canon: impl Fn(&Marking) -> Marking) -> usize {
        use std::collections::HashSet;
        let start = canon(net.initial_marking());
        let mut seen: HashSet<Marking> = HashSet::from([start.clone()]);
        let mut queue = vec![start];
        while let Some(marking) = queue.pop() {
            for transition in net.transitions() {
                if net.is_enabled(transition.id, &marking) {
                    let next = canon(&net.fire(transition.id, &marking));
                    if seen.insert(next.clone()) {
                        queue.push(next);
                    }
                }
            }
        }
        seen.len()
    }

    #[test]
    fn railroad_example_of_the_thesis() {
        let net = railroad_net();
        // Ex. 3.4: Aut(N) is generated by a rotation and the swap of trains, |Aut(N)| = 12.
        let options = SymmetryOptions { respect_initial_marking: false, ..SymmetryOptions::default() };
        let symmetry = Symmetry::compute(&net, &options);
        assert_eq!(symmetry.report().group_size, 12);
        assert!(symmetry.report().complete);
        // Fig. 3.2 draws 18 reachable markings; Fig. 3.3 (left) keeps one per orbit: 18 = 6 + 12, i.e. 2 orbits.
        assert_eq!(reachable(&net, |m| m.clone()), 18);
        let quotient = reachable(&net, |m| symmetry.canonicalize(m));
        assert_eq!(quotient, 2);
        // Ex. 3.4: the orbit of M0 has six markings, so the stabilizer of M0 has order 2.
        let stabilizer = Symmetry::compute(&net, &SymmetryOptions::default());
        assert_eq!(stabilizer.report().group_size, 2);
    }
}
