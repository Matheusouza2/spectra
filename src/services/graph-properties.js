/**
 * Behavioural properties that follow directly from a complete reachability graph.
 * Only valid for the full, unreduced graph: truncated or stubborn-set graphs must not be used.
 */
export function deriveGraphProperties(nodes, edges, transitionIds) {
  if (!Array.isArray(nodes) || nodes.length === 0) return null;
  const ids = nodes.map((n) => n.id);
  const initial = ids.includes(0) ? 0 : ids[0];
  const concrete = (edges || []).filter((e) => !e.is_abstract);

  const hasSuccessor = new Set(concrete.map((e) => e.source));
  const deadlockNodes = ids.filter((id) => !hasSuccessor.has(id));

  const fired = new Set(concrete.map((e) => e.transition));
  const deadTransitions = (transitionIds || []).filter((id) => !fired.has(id));

  // Reversible: M0 is reachable from every reachable marking (reverse BFS from M0).
  const incoming = new Map();
  concrete.forEach((e) => {
    if (!incoming.has(e.target)) incoming.set(e.target, []);
    incoming.get(e.target).push(e.source);
  });
  const reaches = new Set([initial]);
  const queue = [initial];
  while (queue.length) {
    const current = queue.pop();
    (incoming.get(current) || []).forEach((prev) => {
      if (!reaches.has(prev)) { reaches.add(prev); queue.push(prev); }
    });
  }
  const nonReversible = ids.filter((id) => !reaches.has(id));

  return {
    deadlockFree: deadlockNodes.length === 0,
    deadlockNodes,
    deadTransitions,
    reversible: nonReversible.length === 0,
    nonReversibleNodes: nonReversible,
  };
}
