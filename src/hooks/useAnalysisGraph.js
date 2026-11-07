import { useEffect, useRef, useState, useCallback } from 'react';
import * as d3 from 'd3';
import dagre from '@dagrejs/dagre';
import { markingToString } from '../services/token-value';

function computeDepth(nodes, edges) {
  const adj = {};
  const depth = {};
  nodes.forEach((n) => { depth[n.id] = -1; adj[n.id] = []; });
  edges.forEach((e) => {
    if (!adj[e.source]) adj[e.source] = [];
    adj[e.source].push(e.target);
  });
  if (nodes.length === 0) return depth;
  const rootId = nodes[0].id;
  depth[rootId] = 0;
  const queue = [rootId];
  while (queue.length > 0) {
    const u = queue.shift();
    (adj[u] || []).forEach((v) => {
      if (depth[v] === -1) {
        depth[v] = depth[u] + 1;
        queue.push(v);
      }
    });
  }
  return depth;
}

function markingDisplay(m) {
  if (!Array.isArray(m)) return String(m);
  return markingToString(m);
}

/**
 * useAnalysisGraph — D3 + dagre rendering for RG/CT graphs
 *
 * Manages a standalone SVG-based graph visualization with:
 * - depth-based visibility
 * - node click selection
 * - zoom/pan
 */
export default function useAnalysisGraph({ transitionNames } = {}) {
  // Real transition name when known (index = transition id), else the generic tN.
  const transitionLabel = useCallback(
    (id) => transitionNames?.[id] ?? `t${id}`,
    [transitionNames],
  );
  const containerRef = useRef(null);
  const svgRef = useRef(null);
  const [selectedNode, setSelectedNode] = useState(null);
  const [visibleDepth, setVisibleDepth] = useState(5);
  const [maxDepth, setMaxDepth] = useState(0);

  // Refs for trace highlight (persist across renders)
  const graphElementsRef = useRef({ circles: null, lines: null, nodes: [], edges: [] });

  const renderGraph = useCallback((graphData, graphType, depthMax) => {
    if (!graphData || !graphData.nodes || !graphData.nodes.length || !containerRef.current) return;

    const depth = computeDepth(graphData.nodes, graphData.edges || []);
    setMaxDepth(depthMax >= 0 ? depthMax : Math.max(...Object.values(depth).filter((d) => d >= 0), 0));

    const visibleNodeIds = new Set();
    graphData.nodes.forEach((n) => {
      if (depth[n.id] !== undefined && depth[n.id] >= 0 && depth[n.id] <= visibleDepth) {
        visibleNodeIds.add(n.id);
      }
    });

    const nodes = graphData.nodes
      .filter((n) => visibleNodeIds.has(n.id))
      .map((n, i) => ({ ...n, id: n.id ?? i }));

    const edges = (graphData.edges || [])
      .filter((e) => visibleNodeIds.has(e.source) && visibleNodeIds.has(e.target));

    const width = containerRef.current.clientWidth || 800;
    const height = containerRef.current.clientHeight || 420;

    // Clear previous content
    containerRef.current.innerHTML = '';

    const svg = d3.select(containerRef.current)
      .append('svg')
      .attr('width', width)
      .attr('height', height);

    svgRef.current = svg;

    const container = svg.append('g');

    const css = getComputedStyle(document.documentElement);
    const v = (name) => css.getPropertyValue(name).trim();
    const colors = {
      node: v('--accent'),
      nodeFill: v('--bg-surface'),
      omega: v('--green'),
      edge: v('--text-muted'),
      text: v('--text-main'),
      nodeStroke: v('--accent'),
      highlight: v('--amber'),
      dimOpacity: 0.15,
      fullOpacity: 1,
    };
    const isCoverageGraph = graphType === 'coverage';
    const baseStroke = (d) => (isCoverageGraph && d.is_omega ? colors.omega : colors.nodeStroke);
    const baseFill = (d) => (d.id === nodes[0]?.id ? colors.node : colors.nodeFill);

    // Dagre layout
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'TB', nodesep: 50, ranksep: 80, marginx: 20, marginy: 20 });
    g.setDefaultEdgeLabel(() => ({}));
    nodes.forEach(n => g.setNode(n.id, { width: 36, height: 36 }));
    edges.forEach(e => g.setEdge(e.source, e.target));
    dagre.layout(g);

    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    const nodesWithPos = nodes.map(n => {
      const pos = g.node(n.id);
      const x = pos.x;
      const y = pos.y;
      minX = Math.min(minX, x - 40);
      maxX = Math.max(maxX, x + 40);
      minY = Math.min(minY, y - 40);
      maxY = Math.max(maxY, y + 40);
      return { ...n, x, y };
    });

    const edgesWithPos = edges.map(e => {
      const sn = nodesWithPos.find(n => n.id === e.source);
      const tn = nodesWithPos.find(n => n.id === e.target);
      return { ...e, sourceNode: sn, targetNode: tn };
    }).filter(e => e.sourceNode && e.targetNode);

    const graphWidth = maxX - minX;
    const graphHeight = maxY - minY;
    const offsetX = (width - graphWidth) / 2 - minX;
    const offsetY = 30;

    // Zoom behavior
    const zoom = d3.zoom()
      .scaleExtent([0.2, 5])
      .on('zoom', (event) => {
        container.attr('transform', event.transform);
      });
    svg.call(zoom);
    svg.call(zoom.transform, d3.zoomIdentity.translate(offsetX, offsetY));

    // Arrow marker
    svg.append('defs').append('marker')
      .attr('id', 'gt-arrow')
      .attr('viewBox', '0 -5 10 10')
      .attr('refX', 18)
      .attr('refY', 0)
      .attr('orient', 'auto')
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .append('path')
      .attr('d', 'M 0,-5 L 10,0 L 0,5')
      .attr('fill', colors.edge)
      .style('stroke', 'none');

    // Edges
    const lines = container.append('g')
      .selectAll('line')
      .data(edgesWithPos)
      .enter()
      .append('line')
      .attr('x1', d => d.sourceNode.x)
      .attr('y1', d => d.sourceNode.y)
      .attr('x2', d => d.targetNode.x)
      .attr('y2', d => d.targetNode.y)
      .attr('stroke', colors.edge)
      .attr('stroke-width', 2)
      .attr('marker-end', 'url(#gt-arrow)')
      .attr('class', 'trace-edge');

    // Edge labels
    container.append('g')
      .selectAll('text')
      .data(edgesWithPos)
      .enter()
      .append('text')
      .text(d => d.transition !== undefined ? transitionLabel(d.transition) : '')
      .attr('font-size', '7px')
      .attr('font-family', 'IBM Plex Mono, monospace')
      .attr('fill', colors.text)
      .attr('text-anchor', 'middle')
      .attr('x', d => (d.sourceNode.x + d.targetNode.x) / 2)
      .attr('y', d => (d.sourceNode.y + d.targetNode.y) / 2 - 6)
      .attr('opacity', 0.7);

    // Nodes
    const isCoverage = graphType === 'coverage';
    const circle = container.append('g')
      .selectAll('circle')
      .data(nodesWithPos)
      .enter()
      .append('circle')
      .attr('class', 'trace-node')
      .attr('r', 14)
      .attr('cx', d => d.x)
      .attr('cy', d => d.y)
      .attr('fill', baseFill)
      .attr('stroke', baseStroke)
      .attr('stroke-width', 2)
      .style('cursor', 'pointer');

    // Node id labels
    container.append('g')
      .selectAll('text.node-id')
      .data(nodesWithPos)
      .enter()
      .append('text')
      .attr('class', 'node-id')
      .text(d => `M${d.id}`)
      .attr('font-size', '10px')
      .attr('font-family', 'IBM Plex Mono, monospace')
      .attr('fill', colors.text)
      .attr('x', d => d.x + 18)
      .attr('y', d => d.y + 4);

    // Node marking labels
    container.append('g')
      .selectAll('text.node-marking')
      .data(nodesWithPos)
      .enter()
      .append('text')
      .attr('class', 'node-marking')
      .text(d => `[${markingDisplay(d.marking)}]`)
      .attr('font-size', '8px')
      .attr('font-family', 'IBM Plex Mono, monospace')
      .attr('fill', colors.text)
      .attr('opacity', 0.6)
      .attr('x', d => d.x + 18)
      .attr('y', d => d.y + 14);

    // ── Trace Highlight Logic ──
    // Build adjacency maps for trace highlight
    const outgoingMap = {};
    edgesWithPos.forEach((e) => {
      if (!outgoingMap[e.source]) outgoingMap[e.source] = [];
      outgoingMap[e.source].push(e);
    });
    const incomingMap = {};
    edgesWithPos.forEach((e) => {
      if (!incomingMap[e.target]) incomingMap[e.target] = [];
      incomingMap[e.target].push(e);
    });

    // Store for deselect restore
    graphElementsRef.current = {
      circles: circle,
      lines,
      nodes: nodesWithPos,
      edges: edgesWithPos,
    };

    function highlightTrace(nodeId) {
      const traceNodes = new Set([nodeId]);
      const traceEdges = new Set();

      // Outgoing (successors)
      (outgoingMap[nodeId] || []).forEach((e) => {
        traceNodes.add(e.target);
        traceEdges.add(e.source + '→' + e.target);
      });
      // Incoming (predecessors)
      (incomingMap[nodeId] || []).forEach((e) => {
        traceNodes.add(e.source);
        traceEdges.add(e.source + '→' + e.target);
      });

      // Dim non-trace, highlight trace
      circle.each(function (d) {
        const el = d3.select(this);
        if (traceNodes.has(d.id)) {
          el.attr('opacity', colors.fullOpacity)
            .attr('stroke', colors.highlight)
            .attr('stroke-width', 3);
        } else {
          el.attr('opacity', colors.dimOpacity)
            .attr('stroke', baseStroke)
            .attr('stroke-width', 2);
        }
      });

      lines.each(function (d) {
        const el = d3.select(this);
        const key = d.source + '→' + d.target;
        if (traceEdges.has(key)) {
          el.attr('opacity', colors.fullOpacity)
            .attr('stroke', colors.highlight)
            .attr('stroke-width', 3);
        } else {
          el.attr('opacity', colors.dimOpacity)
            .attr('stroke', colors.edge)
            .attr('stroke-width', 2);
        }
      });
    }

    function restoreAll() {
      circle.attr('opacity', colors.fullOpacity)
        .attr('stroke', baseStroke)
        .attr('stroke-width', 2);
      lines.attr('opacity', colors.fullOpacity)
        .attr('stroke', colors.edge)
        .attr('stroke-width', 2);
    }

    // Click handler — trace highlight + selection
    circle.on('click', (event, d) => {
      event.stopPropagation();
      setSelectedNode(d);
      highlightTrace(d.id);
    });

    // Background click to deselect and restore
    svg.on('click', (event) => {
      if (event.target === svg.node()) {
        setSelectedNode(null);
        restoreAll();
      }
    });

    return () => {
      svg.selectAll('*').remove();
    };
  }, [visibleDepth, transitionLabel]);

  const showMore = useCallback(() => {
    setVisibleDepth((prev) => prev + 5);
  }, []);

  const hasMore = visibleDepth < maxDepth;

  return {
    transitionLabel,
    containerRef,
    svgRef,
    selectedNode,
    setSelectedNode,
    visibleDepth,
    maxDepth,
    renderGraph,
    showMore,
    hasMore,
    markingDisplay,
  };
}
