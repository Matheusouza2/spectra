//! Persistent cache of the state-space graphs, one file per net.
//!
//! The graph is generated once, from the Graphs tab, and every other screen
//! (properties, evidence, verification) reads it back from here instead of
//! exploring the net again.
//!
//! File format `.rgc` ("RACoN graph cache"), little-endian, LEB128 varints:
//!
//! ```text
//! magic    "RGC1"
//! kind     u8        1 = reachability, 2 = coverability
//! digest   u64       FNV-1a of the net file, so an edited net never reuses a stale graph
//! places   varint
//! nodes    varint
//! edges    varint
//! flags    u8        bit0 = complete (reachability) / bounded proof (coverability)
//! stats    varint construction_time_ms, varint omega_nodes, varint pruned_nodes
//! node*    token*places, varint depth, [varint level | u8 node-flags]
//! edge*    varint source, varint target, varint transition, [u8 abstract]
//! parents  varint count, (varint child, varint parent)*      (coverability only)
//! ```
//!
//! A token is a tag byte (0 finite, 1 exact big integer, 2 omega) followed by
//! a varint, or by a length-prefixed big-endian byte string for the exact case.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use num_bigint::BigUint;
use racon_core::graph::NodeId;
use racon_core::marking::{Marking, TokenValue};
use racon_core::net::{PetriNet, TransitionId};
use racon_coverability::graph::CoverabilityStats;
use racon_coverability::CoverabilityGraph;
use racon_reachability::graph::GraphStats;
use racon_reachability::ReachabilityGraph;
use serde::Serialize;

const MAGIC: &[u8; 4] = b"RGC1";
const KIND_REACHABILITY: u8 = 1;
const KIND_COVERABILITY: u8 = 2;

/// Prefix of the error returned when a screen asks for a graph that was never generated.
pub const NOT_GENERATED: &str = "GRAFO_NAO_GERADO: gere o grafo na aba Graphs antes de usar este recurso.";

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GraphCacheStatus {
    pub reachability: bool,
    pub coverability: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Reachability,
    Coverability,
}

#[derive(Default)]
struct Entry {
    digest: u64,
    reachability: Option<Arc<ReachabilityGraph>>,
    coverability: Option<Arc<CoverabilityGraph>>,
}

static DIR: OnceLock<PathBuf> = OnceLock::new();
static MEMORY: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();

pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = DIR.set(dir);
}

fn memory() -> &'static Mutex<HashMap<String, Entry>> {
    MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn key_of(file_path: &str) -> String {
    fs::canonicalize(file_path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file_path.to_string())
}

fn digest_of(file_path: &str) -> Result<u64, String> {
    fs::read(file_path).map(|bytes| fnv1a(&bytes)).map_err(|e| e.to_string())
}

fn disk_path(key: &str, kind: Kind) -> Option<PathBuf> {
    let suffix = if kind == Kind::Reachability { "reach" } else { "cover" };
    DIR.get().map(|dir| dir.join(format!("{:016x}-{suffix}.rgc", fnv1a(key.as_bytes()))))
}

// ── Public API ──────────────────────────────────────────────────────────

pub fn status(file_path: &str) -> GraphCacheStatus {
    let Ok(digest) = digest_of(file_path) else { return GraphCacheStatus::default() };
    let key = key_of(file_path);
    GraphCacheStatus {
        reachability: has(&key, digest, Kind::Reachability),
        coverability: has(&key, digest, Kind::Coverability),
    }
}

pub fn store_reachability(file_path: &str, graph: &ReachabilityGraph) -> Result<(), String> {
    let digest = digest_of(file_path)?;
    let key = key_of(file_path);
    write_file(&key, Kind::Reachability, &encode_reachability(graph, digest))?;
    let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
    let entry = fresh_entry(&mut map, &key, digest);
    entry.reachability = Some(Arc::new(graph.clone()));
    Ok(())
}

pub fn store_coverability(file_path: &str, graph: &CoverabilityGraph) -> Result<(), String> {
    let digest = digest_of(file_path)?;
    let key = key_of(file_path);
    write_file(&key, Kind::Coverability, &encode_coverability(graph, digest))?;
    let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
    let entry = fresh_entry(&mut map, &key, digest);
    entry.coverability = Some(Arc::new(graph.clone()));
    Ok(())
}

/// The saved reachability graph, or `NOT_GENERATED`.
pub fn load_reachability(file_path: &str, net: &Arc<PetriNet>) -> Result<Arc<ReachabilityGraph>, String> {
    let digest = digest_of(file_path)?;
    let key = key_of(file_path);
    {
        let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
        let entry = fresh_entry(&mut map, &key, digest);
        if let Some(graph) = &entry.reachability {
            return Ok(Arc::clone(graph));
        }
    }
    let bytes = read_file(&key, Kind::Reachability).ok_or_else(|| NOT_GENERATED.to_string())?;
    let graph = Arc::new(decode_reachability(&bytes, digest, net)?);
    let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
    fresh_entry(&mut map, &key, digest).reachability = Some(Arc::clone(&graph));
    Ok(graph)
}

/// The saved coverability graph, or `NOT_GENERATED`.
pub fn load_coverability(file_path: &str, net: &Arc<PetriNet>) -> Result<Arc<CoverabilityGraph>, String> {
    let digest = digest_of(file_path)?;
    let key = key_of(file_path);
    {
        let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
        let entry = fresh_entry(&mut map, &key, digest);
        if let Some(graph) = &entry.coverability {
            return Ok(Arc::clone(graph));
        }
    }
    let bytes = read_file(&key, Kind::Coverability).ok_or_else(|| NOT_GENERATED.to_string())?;
    let graph = Arc::new(decode_coverability(&bytes, digest, net)?);
    let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
    fresh_entry(&mut map, &key, digest).coverability = Some(Arc::clone(&graph));
    Ok(graph)
}

// ── Storage helpers ─────────────────────────────────────────────────────

/// The in-memory entry for `key`, dropped and recreated if the net file changed.
fn fresh_entry<'a>(map: &'a mut HashMap<String, Entry>, key: &str, digest: u64) -> &'a mut Entry {
    let entry = map.entry(key.to_string()).or_default();
    if entry.digest != digest {
        *entry = Entry { digest, ..Entry::default() };
    }
    entry
}

fn has(key: &str, digest: u64, kind: Kind) -> bool {
    {
        let mut map = memory().lock().unwrap_or_else(|p| p.into_inner());
        let entry = fresh_entry(&mut map, key, digest);
        match kind {
            Kind::Reachability if entry.reachability.is_some() => return true,
            Kind::Coverability if entry.coverability.is_some() => return true,
            _ => {}
        }
    }
    read_file(key, kind).is_some_and(|bytes| header_matches(&bytes, kind, digest))
}

fn header_matches(bytes: &[u8], kind: Kind, digest: u64) -> bool {
    let tag = if kind == Kind::Reachability { KIND_REACHABILITY } else { KIND_COVERABILITY };
    bytes.len() >= 13 && &bytes[..4] == MAGIC && bytes[4] == tag && bytes[5..13] == digest.to_le_bytes()
}

fn read_file(key: &str, kind: Kind) -> Option<Vec<u8>> {
    fs::read(disk_path(key, kind)?).ok()
}

fn write_file(key: &str, kind: Kind, bytes: &[u8]) -> Result<(), String> {
    let path = disk_path(key, kind).ok_or("graph cache directory is not initialised")?;
    let tmp = path.with_extension("rgc.tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("could not save the graph: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("could not save the graph: {e}"))
}

// ── Encoding ────────────────────────────────────────────────────────────

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_marking(out: &mut Vec<u8>, marking: &Marking) {
    for token in &marking.tokens {
        match token {
            TokenValue::Finite(v) => {
                out.push(0);
                put_varint(out, *v);
            }
            TokenValue::Exact(v) => {
                out.push(1);
                let bytes = v.to_bytes_be();
                put_varint(out, bytes.len() as u64);
                out.extend_from_slice(&bytes);
            }
            TokenValue::Omega => out.push(2),
        }
    }
}

fn header(kind: u8, digest: u64, places: usize, nodes: usize, edges: usize, flags: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + nodes * (places + 4) + edges * 4);
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&digest.to_le_bytes());
    put_varint(&mut out, places as u64);
    put_varint(&mut out, nodes as u64);
    put_varint(&mut out, edges as u64);
    out.push(flags);
    out
}

fn encode_reachability(graph: &ReachabilityGraph, digest: u64) -> Vec<u8> {
    let places = graph.nodes.first().map_or(0, |n| n.marking.tokens.len());
    let mut out = header(KIND_REACHABILITY, digest, places, graph.nodes.len(), graph.edges.len(), u8::from(graph.complete));
    put_varint(&mut out, graph.stats.construction_time_ms);
    put_varint(&mut out, 0);
    put_varint(&mut out, 0);
    for node in &graph.nodes {
        put_marking(&mut out, &node.marking);
        put_varint(&mut out, u64::from(node.depth));
        put_varint(&mut out, u64::from(node.level));
    }
    for edge in &graph.edges {
        put_varint(&mut out, u64::from(edge.source.0));
        put_varint(&mut out, u64::from(edge.target.0));
        put_varint(&mut out, u64::from(edge.transition.0));
    }
    out
}

fn encode_coverability(graph: &CoverabilityGraph, digest: u64) -> Vec<u8> {
    let places = graph.nodes.first().map_or(0, |n| n.marking.tokens.len());
    let mut out = header(KIND_COVERABILITY, digest, places, graph.nodes.len(), graph.edges.len(), u8::from(graph.bounded_proof_available));
    put_varint(&mut out, graph.stats.construction_time_ms);
    put_varint(&mut out, graph.stats.omega_nodes as u64);
    put_varint(&mut out, graph.stats.pruned_nodes as u64);
    for node in &graph.nodes {
        put_marking(&mut out, &node.marking);
        put_varint(&mut out, u64::from(node.depth));
        out.push(u8::from(node.is_active) | (u8::from(node.is_expanded) << 1));
    }
    for edge in &graph.edges {
        put_varint(&mut out, u64::from(edge.source.0));
        put_varint(&mut out, u64::from(edge.target.0));
        put_varint(&mut out, u64::from(edge.transition.0));
        out.push(u8::from(edge.is_abstract));
    }
    put_varint(&mut out, graph.parent.len() as u64);
    let mut parents: Vec<_> = graph.parent.iter().collect();
    parents.sort_by_key(|(child, _)| child.0);
    for (child, parent) in parents {
        put_varint(&mut out, u64::from(child.0));
        put_varint(&mut out, u64::from(parent.0));
    }
    out
}

// ── Decoding ────────────────────────────────────────────────────────────

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let value = *self.bytes.get(self.at).ok_or("the saved graph is truncated")?;
        self.at += 1;
        Ok(value)
    }

    fn varint(&mut self) -> Result<u64, String> {
        let (mut value, mut shift) = (0u64, 0u32);
        loop {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift > 63 {
                return Err("the saved graph is corrupt".into());
            }
        }
    }

    fn u32(&mut self) -> Result<u32, String> {
        u32::try_from(self.varint()?).map_err(|_| "the saved graph is corrupt".to_string())
    }

    fn marking(&mut self, places: usize) -> Result<Marking, String> {
        let mut tokens = Vec::with_capacity(places);
        for _ in 0..places {
            tokens.push(match self.byte()? {
                0 => TokenValue::Finite(self.varint()?),
                1 => {
                    let len = self.varint()? as usize;
                    let slice = self.bytes.get(self.at..self.at + len).ok_or("the saved graph is truncated")?;
                    self.at += len;
                    TokenValue::Exact(BigUint::from_bytes_be(slice))
                }
                2 => TokenValue::Omega,
                _ => return Err("the saved graph is corrupt".into()),
            });
        }
        Ok(Marking { tokens })
    }
}

struct Header {
    places: usize,
    nodes: usize,
    edges: usize,
    flags: u8,
}

fn read_header(reader: &mut Reader, kind: Kind, digest: u64, net: &PetriNet) -> Result<Header, String> {
    if !header_matches(reader.bytes, kind, digest) {
        return Err("the saved graph does not belong to this net; generate it again in the Graphs tab".into());
    }
    reader.at = 13;
    let places = reader.varint()? as usize;
    if places != net.place_count() {
        return Err("the saved graph does not match the net; generate it again in the Graphs tab".into());
    }
    Ok(Header {
        places,
        nodes: reader.varint()? as usize,
        edges: reader.varint()? as usize,
        flags: reader.byte()?,
    })
}

fn decode_reachability(bytes: &[u8], digest: u64, net: &Arc<PetriNet>) -> Result<ReachabilityGraph, String> {
    let mut reader = Reader { bytes, at: 0 };
    let head = read_header(&mut reader, Kind::Reachability, digest, net)?;
    let construction_time_ms = reader.varint()?;
    reader.varint()?;
    reader.varint()?;

    let mut graph = ReachabilityGraph::with_net(Arc::clone(net));
    let mut max_depth = 0;
    let mut max_level = 0;
    for _ in 0..head.nodes {
        let marking = reader.marking(head.places)?;
        let depth = reader.u32()?;
        let level = reader.u32()?;
        max_depth = max_depth.max(depth);
        max_level = max_level.max(level);
        let id = graph.add_node(marking.clone(), depth, level);
        graph.nodes[id.0 as usize].is_expanded = true;
        graph.marking_to_node.insert(marking, id.0 as usize);
    }
    for _ in 0..head.edges {
        let (source, target, transition) = (reader.u32()?, reader.u32()?, reader.u32()?);
        if source as usize >= head.nodes || target as usize >= head.nodes {
            return Err("the saved graph is corrupt".into());
        }
        graph.add_edge(NodeId(source), NodeId(target), TransitionId(transition));
    }
    graph.level_index = vec![Vec::new(); max_level as usize + 1];
    for node in &graph.nodes {
        graph.level_index[node.level as usize].push(node.id);
    }
    graph.current_level = max_level;
    graph.complete = head.flags & 1 == 1;
    graph.stats = GraphStats {
        total_nodes: head.nodes,
        total_edges: head.edges,
        max_depth,
        max_level,
        construction_time_ms,
    };
    Ok(graph)
}

fn decode_coverability(bytes: &[u8], digest: u64, net: &Arc<PetriNet>) -> Result<CoverabilityGraph, String> {
    let mut reader = Reader { bytes, at: 0 };
    let head = read_header(&mut reader, Kind::Coverability, digest, net)?;
    let construction_time_ms = reader.varint()?;
    let omega_nodes = reader.varint()? as usize;
    let pruned_nodes = reader.varint()? as usize;

    let mut graph = CoverabilityGraph::with_net(Arc::clone(net));
    for _ in 0..head.nodes {
        let marking = reader.marking(head.places)?;
        let depth = reader.u32()?;
        let flags = reader.byte()?;
        let id = graph.add_node(marking.clone(), depth);
        let node = &mut graph.nodes[id.0 as usize];
        node.is_active = flags & 1 == 1;
        node.is_expanded = flags & 2 == 2;
        graph.marking_to_node.insert(marking, id.0 as usize);
    }
    for _ in 0..head.edges {
        let (source, target, transition) = (reader.u32()?, reader.u32()?, reader.u32()?);
        let is_abstract = reader.byte()? == 1;
        if source as usize >= head.nodes || target as usize >= head.nodes {
            return Err("the saved graph is corrupt".into());
        }
        graph.add_edge(NodeId(source), NodeId(target), TransitionId(transition));
        if let Some(edge) = graph.edges.last_mut() {
            edge.is_abstract = is_abstract;
        }
    }
    for _ in 0..reader.varint()? {
        let (child, parent) = (reader.u32()?, reader.u32()?);
        graph.parent.insert(NodeId(child), NodeId(parent));
    }
    graph.bounded_proof_available = head.flags & 1 == 1;
    graph.stats = CoverabilityStats {
        total_nodes: head.nodes,
        total_edges: head.edges,
        omega_nodes,
        pruned_nodes,
        construction_time_ms,
    };
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use racon_coverability::{CoverabilityConfig, CoverabilityEngine, CoverabilityEngineImpl, CoverabilityStrategy};
    use racon_parsers::{PntParser, PntParserConfig, PntParserImpl};
    use racon_reachability::{ReachabilityConfig, ReachabilityEngine, ReachabilityEngineImpl};

    fn net_from(path: &str) -> Arc<PetriNet> {
        let text = fs::read_to_string(path).expect("sample net");
        Arc::new(PntParserImpl::new(PntParserConfig::default()).parse_string(&text).expect("parse"))
    }

    #[test]
    fn reachability_graph_survives_a_round_trip() {
        let net = net_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../model.pnt"));
        let ctx = racon_core::execution::AnalysisContext::new(racon_core::execution::AnalysisConfig::default());
        let graph = ReachabilityEngineImpl { config: ReachabilityConfig::default() }.build_complete(&net, &ctx).expect("graph");
        let bytes = encode_reachability(&graph, 42);
        let back = decode_reachability(&bytes, 42, &net).expect("decode");
        assert_eq!(back.nodes.len(), graph.nodes.len());
        assert_eq!(back.edges.len(), graph.edges.len());
        assert!(back.complete);
        for (a, b) in graph.nodes.iter().zip(&back.nodes) {
            assert_eq!(a.marking, b.marking);
        }
        for (a, b) in graph.edges.iter().zip(&back.edges) {
            assert_eq!((a.source, a.target, a.transition), (b.source, b.target, b.transition));
        }
        assert_eq!(back.adjacency, graph.adjacency);
        // A graph saved for another version of the net is refused.
        assert!(decode_reachability(&bytes, 43, &net).is_err());
    }

    #[test]
    fn coverability_graph_survives_a_round_trip() {
        let net = net_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../model.pnt"));
        let ctx = racon_core::execution::AnalysisContext::new(racon_core::execution::AnalysisConfig::default());
        let graph = CoverabilityEngineImpl { config: CoverabilityConfig { max_nodes: 100_000, max_edges: 500_000 } }
            .build_complete(&net, CoverabilityStrategy::KarpMiller, &ctx)
            .expect("graph");
        let back = decode_coverability(&encode_coverability(&graph, 7), 7, &net).expect("decode");
        assert_eq!(back.nodes.len(), graph.nodes.len());
        assert_eq!(back.edges.len(), graph.edges.len());
        assert_eq!(back.bounded_status(), graph.bounded_status());
        assert_eq!(back.parent, graph.parent);
    }

    #[test]
    fn varints_round_trip() {
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX >> 1] {
            let mut out = Vec::new();
            put_varint(&mut out, value);
            assert_eq!(Reader { bytes: &out, at: 0 }.varint().unwrap(), value);
        }
    }
}
