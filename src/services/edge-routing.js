import { register, Polyline } from '@antv/g6';

// Arc routing for the Petri-net graphs: every arc is an orthogonal path with
// rounded corners that goes AROUND the other places and transitions (and their
// labels) instead of across them. G6's A* "shortest-path" router does the search
// on a grid, treating every node as an obstacle.

const ROUTER_BASE = {
  type: 'shortest-path',
  enableObstacleAvoidance: true,
  // Coarser grid = far fewer A* cells; arcs still hug nodes at a 14 px ring.
  gridSize: 14,
  maximumLoops: 3000,
  // Labels sit below the nodes, so arcs never leave or enter through the bottom.
  startDirections: ['top', 'left', 'right'],
  endDirections: ['top', 'left', 'right'],
};

// Opposite arcs between the same two nodes (p→t and t→p) would trace the same
// route; the second one runs on a wider ring so the pair stays readable.
const NEAR = 14;
const FAR = 28;

/** Tag arcs that have a reverse partner with a lane (0 = inner ring, 1 = outer ring). */
export function assignLanes(edges) {
  const seen = new Set();
  edges.forEach((edge) => {
    const key = `${edge.source}\u0000${edge.target}`;
    const reverse = `${edge.target}\u0000${edge.source}`;
    edge.data = { ...edge.data, lane: seen.has(reverse) ? 1 : 0 };
    seen.add(key);
  });
  return edges;
}

/** Edge options for a G6 graph: orthogonal, rounded, obstacle-avoiding. */
export function routedEdgeStyle() {
  return {
    type: 'routed-polyline',
    style: {
      radius: 10,
      router: (d) => ({ ...ROUTER_BASE, offset: d.data?.lane ? FAR : NEAR }),
    },
  };
}

/**
 * Polyline whose A* route is computed once and reused until something that can
 * change it moves: this arc's endpoints, or any node (the routing "version",
 * bumped when the layout settles or a drag ends). Without this, every redraw,
 * drag frame and token animation re-ran the search for every arc.
 */
class RoutedPolyline extends Polyline {
  getControlPoints(attributes) {
    // Until the layout settles the nodes are still moving: draw straight, route later.
    if (this.context?.graph?.__routeHold) return [];
    const { sourceNode, targetNode } = this;
    const [sx, sy] = sourceNode.getCenter();
    const [tx, ty] = targetNode.getCenter();
    const version = this.context?.graph?.__routeVersion ?? 0;
    const key = `${Math.round(sx)}|${Math.round(sy)}|${Math.round(tx)}|${Math.round(ty)}|${version}|${attributes.router?.offset}`;
    if (this._routeKey === key && this._routePoints) return this._routePoints;
    const points = super.getControlPoints(attributes);
    this._routeKey = key;
    this._routePoints = points;
    return points;
  }
}
register('edge', 'routed-polyline', RoutedPolyline);

/** Invalidate every cached route (call after nodes moved), then redraw arcs. */
export function rerouteAll(graph) {
  if (!graph || graph.destroyed) return;
  graph.__routeVersion = (graph.__routeVersion ?? 0) + 1;
  graph.__routeHold = false;
  try {
    graph.updateEdgeData(graph.getEdgeData().map((e) => ({ id: e.id, style: { routeVersion: graph.__routeVersion } })));
    return graph.draw();
  } catch (e) {
    if (!String(e).includes('destroyed')) console.warn('G6 reroute error:', e);
  }
}

/** Re-route once a drag ends; during the drag only the dragged node's own arcs move. */
export function watchRouting(graph) {
  graph.__routeHold = true; // released by the first rerouteAll, once positions are final
  graph.on('node:dragend', () => rerouteAll(graph));
}
