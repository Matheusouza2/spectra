import React, { useEffect, useRef } from 'react';
import { Graph } from '@antv/g6';
import './PetriNetGraph.css';
import { tokenValueToNumber, tokenValueToString } from '../services/token-value';
import { useI18n } from '../i18n/I18nContext';
import { resolveOverlaps } from '../services/overlap';
import { assignLanes, routedEdgeStyle, watchRouting, rerouteAll } from '../services/edge-routing';

// Footprint of a node including its label, used to keep nodes apart.
const LABEL_CHAR_WIDTH = 5.6;
const footprint = (node) => {
  const isPlace = node.data.type === 'place';
  const w = isPlace ? 44 : node.data.immediate ? 10 : 44;
  const h = isPlace ? 44 : node.data.immediate ? 38 : 26;
  const labelW = String(node.data.label ?? '').length * LABEL_CHAR_WIDTH;
  return { w: Math.max(w, labelW), h: h + 18 };
};

const PetriNetGraph = ({ data }) => {
  const { t } = useI18n();
  const containerRef = useRef(null);
  const graphRef = useRef(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    const width = container.clientWidth || 700;
    const height = container.clientHeight || 600;

    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    const colors = {
      place: isDark ? '#5b8def' : '#5a8fec',
      trans: isDark ? '#f0b429' : '#e88d3d',
      // Immediate transitions: distinct hue, thin bar (classic GSPN notation).
      immediate: isDark ? '#a78bfa' : '#7c3aed',
      edge: isDark ? '#6b6f8a' : '#9c9cab',
      text: isDark ? '#e8e9f1' : '#1a1a2e',
      bg: isDark ? '#181b28' : '#ffffff',
    };

    const graph = new Graph({
      container,
      width,
      height,
      autoFit: 'view',
      padding: 56,
      data: { nodes: [], edges: [] },
      node: {
        type: (d) => d.data.type === 'place' ? 'circle' : 'rect',
        style: {
          size: (d) => d.data.type === 'place' ? 44 : d.data.immediate ? [10, 38] : [44, 26],
          labelText: (d) => d.data.label,
          labelPlacement: 'bottom',
          labelOffsetY: 4,
          labelFontSize: 10,
          labelFill: colors.text,
          fill: (d) => (d.data.type === 'place' ? colors.place : d.data.immediate ? colors.immediate : colors.trans),
          stroke: isDark ? '#262a3e' : '#fff',
          lineWidth: 2,
        },
      },
      edge: {
        type: routedEdgeStyle().type,
        style: {
          ...routedEdgeStyle().style,
          stroke: colors.edge,
          lineWidth: (d) => (d.data.inhibitor ? 1.5 : d.data.weight > 1 ? 3 : 1.5),
          // Inhibitor arc: dashed line, hollow circle at the transition and
          // the label "<k" (the transition needs M(p) < k; no token moves).
          lineDash: (d) => (d.data.inhibitor ? [5, 4] : 0),
          labelText: (d) => (d.data.inhibitor
            ? `<${d.data.threshold}`
            : d.data.weight > 1 ? String(d.data.weight) : ''),
          labelFontSize: 9,
          labelFill: colors.text,
          labelBackground: true,
          labelBackgroundFill: colors.bg,
          endArrow: true,
          endArrowType: (d) => (d.data.inhibitor ? 'circle' : 'triangle'),
        },
      },
      layout: {
        type: 'force',
        preventOverlap: true,
        // Reserve room for the node plus its label, and spread bigger nets out.
        nodeSize: (node) => {
          const { w, h } = footprint(node);
          return Math.max(w, h);
        },
        nodeSpacing: 14,
        linkDistance: (edge, source, target) => 110 + Math.min(80, Math.sqrt((source?.data?.degree ?? 1) + (target?.data?.degree ?? 1)) * 14),
        nodeStrength: 1800,
        collideStrength: 1,
        maxIteration: 600,
        gravity: 6,
      },
      behaviors: [
        {
          type: 'drag-canvas',
          // Keep the viewport close enough to the card that the whole graph
          // cannot be panned away from the visible analysis panel.
          range: [0.18, 0.18, 0.18, 0.18],
        },
        'zoom-canvas',
        'drag-element',
      ],
      animation: false,
    });

    graphRef.current = graph;
    watchRouting(graph);

    const resizeObserver = typeof ResizeObserver !== 'undefined'
      ? new ResizeObserver(([entry]) => {
          const nextWidth = Math.floor(entry.contentRect.width);
          const nextHeight = Math.floor(entry.contentRect.height);
          if (nextWidth > 0 && nextHeight > 0) {
            graph.setSize(nextWidth, nextHeight);
          }
        })
      : null;
    resizeObserver?.observe(container);

    return () => {
      resizeObserver?.disconnect();
      graph.destroy();
      graphRef.current = null;
    };
  }, []);

  useEffect(() => {
    const graph = graphRef.current;
    if (!graph || !data) return;

    try {
      const nodes = [
        ...data.places.map((p) => ({
          id: p.name,
          data: {
            label: `${p.name} (${tokenValueToString(p.tokens)})`,
            type: 'place',
            tokens: tokenValueToNumber(p.tokens),
            origId: p.id,
          },
        })),
        ...data.transitions.map((tr) => ({
          id: tr.name,
          data: {
            label: tr.immediate ? `${tr.name} (${t('token.immediate')})` : tr.name,
            type: 'transition',
            immediate: Boolean(tr.immediate),
            origId: tr.id,
          },
        })),
      ];

      const edges = data.arcs.map((a, i) => ({
        id: `a${i}`,
        source: a.source,
        target: a.target,
        data: {
          weight: a.weight,
          inhibitor: a.kind === 'inhibitor',
          threshold: a.threshold,
        },
      }));

      assignLanes(edges);

      // Degree of each node: hubs get longer links so their neighbours fan out.
      const degree = new Map();
      edges.forEach((e) => {
        degree.set(e.source, (degree.get(e.source) || 0) + 1);
        degree.set(e.target, (degree.get(e.target) || 0) + 1);
      });
      nodes.forEach((n) => { n.data.degree = degree.get(n.id) || 0; });

      graph.setData({ nodes, edges });
      // The force layout only discourages overlaps; once it settles, push apart
      // whatever still collides so no place or transition sits on another.
      Promise.resolve(graph.render())
        .then(() => {
          if (graph.destroyed) return;
          const boxes = nodes.map((n) => {
            const [x, y] = graph.getElementPosition(n.id);
            return { id: n.id, x, y, ...footprint(n) };
          });
          const before = boxes.map((b) => `${b.x.toFixed(1)},${b.y.toFixed(1)}`).join('|');
          resolveOverlaps(boxes);
          if (before === boxes.map((b) => `${b.x.toFixed(1)},${b.y.toFixed(1)}`).join('|')) return rerouteAll(graph);
          graph.updateNodeData(boxes.map((b) => ({ id: b.id, style: { x: b.x, y: b.y } })));
          // Nodes moved: the cached arc routes are stale.
          return Promise.resolve(rerouteAll(graph)).then(() => graph.fitView?.());
        })
        .catch((e) => {
          if (!String(e).includes('destroyed')) console.warn('G6 layout error:', e);
          rerouteAll(graph);
        });
    } catch (e) {
      if (String(e).includes('destroyed')) return;
      console.warn('G6 render error:', e);
    }
  }, [data]);

  if (!data || !data.places || data.places.length === 0) {
    return (
      <div className="pnet-graph">
        <div className="pnet-empty">{t('app.structureUnavailable')}</div>
      </div>
    );
  }

  return <div ref={containerRef} className="pnet-graph" />;
};

export default PetriNetGraph;
