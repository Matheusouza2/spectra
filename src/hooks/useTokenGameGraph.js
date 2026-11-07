import { useEffect, useRef, useState, useCallback } from 'react';
import { Graph, register, Circle as G6Circle } from '@antv/g6';
import { Circle as GCircle, Text as GText, Group as GGroup } from '@antv/g';
import { assignLanes, routedEdgeStyle, watchRouting, rerouteAll } from '../services/edge-routing';

// ── Custom Place Node ────────────────────────────────────────
// Extends G6 Circle to draw token indicators inside

/** Dot positions for 1-3 tokens */
function getDotPositions(count) {
  if (count === 1) return [{ x: 0, y: 0 }];
  if (count === 2) return [{ x: -5, y: 0 }, { x: 5, y: 0 }];
  // count === 3: triangle
  return [{ x: 0, y: -5 }, { x: -5, y: 5 }, { x: 5, y: 5 }];
}

class PlaceNode extends G6Circle {
  render(attributes = this.parsedAttributes, container = this) {
    // Draw the base circle + label
    super.render(attributes, container);

    const tokens = attributes.tokenCount || 0;
    const size = this.getKeyStyle(attributes).size || 40;
    const r = size / 2;

    // ── Token indicators ──
    // Remove old token group
    const oldGroup = this.shapeMap['token-group'];
    if (oldGroup) {
      container.removeChild(oldGroup);
      delete this.shapeMap['token-group'];
    }

    if (tokens <= 0) return;

    // Create a group for all token indicators (easier cleanup)
    const tokenGroup = this.upsert('token-group', GGroup, {
      zIndex: 10,
      pointerEvents: 'none',
    }, container);

    if (tokens <= 3) {
      // Dots: small filled circles
      const positions = getDotPositions(tokens);
      positions.forEach((pos) => {
        tokenGroup.appendChild(new GCircle({
          style: {
            cx: pos.x,
            cy: pos.y,
            r: 3.5,
            fill: '#fff',
            stroke: 'none',
          },
        }));
      });
    } else {
      // Number text centered
      tokenGroup.appendChild(new GText({
        style: {
          text: String(tokens),
          fontSize: Math.min(r * 0.65, 16),
          fill: '#fff',
          textAlign: 'center',
          textBaseline: 'middle',
          fontWeight: 700,
          fontFamily: 'JetBrains Mono, monospace',
        },
      }));
    }
  }
}

// Register once (safe on hot-reload — G6 warns but overrides)
register('node', 'place-node', PlaceNode);

/**
 * useTokenGameGraph — G6 lifecycle for Token Game
 *
 * Creates and manages a standalone G6 graph instance for Petri net
 * visualization with:
 * - Custom place-node with token indicators inside circles
 * - Animated token movement on transition fire
 * - Animation completion callback for state locking
 *
 * Never shares graph refs, listeners, or state with other hooks.
 */
export default function useTokenGameGraph({ immediateLabel = 'immediate' } = {}) {
  const containerRef = useRef(null);
  const graphRef = useRef(null);
  const [graphReady, setGraphReady] = useState(false);

  const initGraph = useCallback((structure, markingData) => {
    if (graphRef.current) return;

    const container = containerRef.current;
    if (!container || !structure) return;

    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    const colors = {
      place: isDark ? '#5b8def' : '#5a8fec',
      trans: isDark ? '#f0b429' : '#e88d3d',
      immediate: isDark ? '#a78bfa' : '#7c3aed',
      transEnabled: isDark ? '#34d399' : '#2b9d6f',
      edge: isDark ? '#6b6f8a' : '#9c9cab',
      text: isDark ? '#e8e9f1' : '#1a1a2e',
      stroke: isDark ? '#262a3e' : '#fff',
    };

    const nodes = [
      ...structure.places.map((p) => ({
        id: p.name,
        data: {
          label: p.name,
          type: 'place',
          tokens: markingData[p.id] ?? 0,
          origId: p.id,
        },
      })),
      ...structure.transitions.map((t) => ({
        id: t.name,
        data: { label: t.immediate ? `${t.name} (${immediateLabel})` : t.name, type: 'transition', immediate: Boolean(t.immediate), origId: t.id },
      })),
    ];

    const edges = structure.arcs.map((a, i) => ({
      id: `a${i}`,
      source: a.source,
      target: a.target,
      data: { weight: a.weight, inhibitor: a.kind === 'inhibitor', threshold: a.threshold },
    }));
    assignLanes(edges);

    try {
      const graph = new Graph({
        container,
        width: container.clientWidth || 600,
        height: container.clientHeight || 300,
        autoFit: 'center',
        data: { nodes, edges },
        node: {
          type: (d) => (d.data.type === 'place' ? 'place-node' : 'rect'),
          style: {
            size: (d) => (d.data.type === 'place' ? 40 : d.data.immediate ? [10, 34] : [42, 24]),
            labelText: (d) => d.data.label,
            labelPlacement: 'bottom',
            labelOffsetY: 3,
            labelFontSize: 9,
            labelFill: colors.text,
            fill: (d) => {
              if (d.data.type === 'place') return colors.place;
              return d.data.immediate ? colors.immediate : colors.trans;
            },
            stroke: colors.stroke,
            lineWidth: 2,
            // Custom attribute: token count for PlaceNode
            tokenCount: (d) => (d.data.type === 'place' ? (d.data.tokens ?? 0) : 0),
          },
        },
        edge: {
          type: routedEdgeStyle().type,
          style: {
            ...routedEdgeStyle().style,
            stroke: colors.edge,
            lineWidth: 1.5,
            lineDash: (d) => (d.data.inhibitor ? [5, 4] : 0),
            labelText: (d) => (d.data.inhibitor ? `<${d.data.threshold}` : d.data.weight > 1 ? String(d.data.weight) : ''),
            labelFontSize: 9,
            labelFill: colors.text,
            endArrow: true,
            endArrowType: (d) => (d.data.inhibitor ? 'circle' : 'triangle'),
          },
        },
        layout: { type: 'force', preventOverlap: true, linkDistance: 100 },
        behaviors: ['drag-canvas', 'zoom-canvas', 'drag-element'],
        animation: false,
      });

      graphRef.current = graph;
      watchRouting(graph);
      graph.render().then(() => rerouteAll(graph)).then(() => {
        setGraphReady(true);
      }).catch((e) => {
        console.warn('G6 render error:', e);
        graphRef.current = null;
        setGraphReady(false);
      });
    } catch (e) {
      console.warn('G6 init error:', e);
      graphRef.current = null;
      setGraphReady(false);
    }
  }, [immediateLabel]);

  /**
   * Animate tokens along edges and update graph data.
   * Calls onComplete when animation finishes.
   */
  const animateAndUpdate = useCallback((
    beforeMarking,
    afterMarking,
    transitionIdx,
    onComplete,
  ) => {
    const graph = graphRef.current;
    if (!graph) {
      if (onComplete) onComplete();
      return;
    }

    let animating = true;

    try {
      // Node IDs are place/transition NAMES from the PNT file; look up
      // the transition node by its sequential origId.
      const allNodes = graph.getNodeData();
      const transNode = allNodes.find((n) => n.data.type === 'transition' && n.data.origId === transitionIdx);
      const tid = transNode ? transNode.id : `t${transitionIdx}`;
      const edgeData = graph.getEdgeData();
      // Arcs keep their Petri-net direction: input Place→Transition and
      // output Transition→Place.
      // Inhibitor arcs only test the marking: no token travels along them.
      const inputArcs = edgeData.filter((e) => e.target === tid && !e.data?.inhibitor);
      const outputArcs = edgeData.filter((e) => e.source === tid);

      // Collect tokens consumed from each source place
      const consumedTokens = {};
      inputArcs.forEach((e) => {
        const pId = e.source;
        const pIdx = parseInt(pId.slice(1), 10);
        const consumed = beforeMarking[pIdx] - (afterMarking[pIdx] ?? 0);
        if (consumed > 0) consumedTokens[pId] = (consumedTokens[pId] || 0) + consumed;
      });

      // Collect tokens produced to each destination place
      const producedTokens = {};
      outputArcs.forEach((e) => {
        const pId = e.target;
        const pIdx = parseInt(pId.slice(1), 10);
        const produced = (afterMarking[pIdx] ?? 0) - beforeMarking[pIdx];
        if (produced > 0) producedTokens[pId] = (producedTokens[pId] || 0) + produced;
      });

      const animContainer = graph.getCanvas().getContainer();
      let maxAnimDelay = 0;

      // Phase 1: Animate tokens consumed (places → transition)
      if (animContainer) {
        inputArcs.forEach((e) => {
          const pId = e.source;
          const count = consumedTokens[pId];
          if (!count || count <= 0) return;
          for (let i = 0; i < Math.min(count, 2); i++) {
            const delay = i * 80;
            maxAnimDelay = Math.max(maxAnimDelay, delay + 300);
            animateDot(graph, pId, tid, delay, animContainer, e.id);
          }
        });
      }

      // Phase 2: Animate tokens produced (transition → places)
      const totalDelay = maxAnimDelay + 50;
      setTimeout(() => {
        if (!graphRef.current) {
          animating = false;
          if (onComplete) onComplete();
          return;
        }
        outputArcs.forEach((e) => {
          const pId = e.target;
          const count = producedTokens[pId];
          if (!count || count <= 0) return;
          for (let i = 0; i < Math.min(count, 2); i++) {
            animateDot(graph, tid, pId, i * 80, animContainer, e.id);
          }
        });
      }, totalDelay);

      // Highlight transition briefly
      graph.setElementState(tid, ['firing']);

      // Update graph data after animation completes
      const updateDelay = Math.max(totalDelay + 400, 600);
      setTimeout(() => {
        const g = graphRef.current;
        if (!g) {
          animating = false;
          if (onComplete) onComplete();
          return;
        }
        try {
          g.setElementState(tid, []);

          // Build updated node data with new token counts
          const nodes = g.getNodeData();
          const updated = nodes.map((n) => {
            if (n.data.type === 'place') {
              const idx = n.data.origId;
              const tokens = afterMarking[idx] ?? 0;
              return { ...n, data: { ...n.data, tokens, label: n.data.label } };
            }
            return n;
          });
          // Edge data unchanged
          g.setData({ nodes: updated, edges: g.getEdgeData() });
          g.draw();
        } catch (e) { /* ignore */ }

        animating = false;
        if (onComplete) onComplete();
      }, updateDelay);
    } catch (e) {
      if (!String(e).includes('destroyed')) {
        console.warn('G6 animation error:', e);
      }
      animating = false;
      if (onComplete) onComplete();
    }
  }, []);

  // ── Token Animation helper ──
  function animateDot(graph, sourceId, targetId, delay, container, edgeId) {
    try {
      const sModel = graph.getNodeModel(sourceId);
      const tModel = graph.getNodeModel(targetId);
      if (!sModel || !tModel) return;

      const sx = sModel.x ?? 0;
      const sy = sModel.y ?? 0;
      const tx = tModel.x ?? 0;
      const ty = tModel.y ?? 0;

      const dot = document.createElement('div');
      dot.style.cssText = [
        'position:absolute',
        'width:7px',
        'height:7px',
        'border-radius:50%',
        'background:#f0b429',
        'transform:translate(-50%,-50%)',
        'pointer-events:none',
        'z-index:100',
        'opacity:0.9',
        'box-shadow:0 0 4px rgba(240,180,41,0.6)',
      ].join(';');
      container.appendChild(dot);

      const start = Date.now() + delay;
      const duration = 280;
      const gCanvas = graph.getCanvas();

      // Follow the routed arc itself when its path is available; otherwise go straight.
      let keyShape = null;
      try {
        keyShape = graph.context?.element?.getElement?.(edgeId)?.getShape?.('key') ?? null;
        if (typeof keyShape?.getPoint !== 'function') keyShape = null;
      } catch (_) { keyShape = null; }

      const animate = () => {
        const elapsed = Date.now() - start;
        if (elapsed < 0) { requestAnimationFrame(animate); return; }
        const t = Math.min(elapsed / duration, 1);
        // Ease-out quad
        const ease = t * (2 - t);
        let cx = sx + (tx - sx) * ease;
        let cy = sy + (ty - sy) * ease;
        if (keyShape) {
          try {
            const along = keyShape.getPoint(ease);
            if (along && Number.isFinite(along.x) && Number.isFinite(along.y)) { cx = along.x; cy = along.y; }
          } catch (_) { /* straight line fallback */ }
        }
        try {
          const pt = gCanvas.getViewportByCanvas({ x: cx, y: cy });
          dot.style.left = pt.x + 'px';
          dot.style.top = pt.y + 'px';
        } catch (_) {
          dot.style.left = cx + 'px';
          dot.style.top = cy + 'px';
        }
        if (t < 1) {
          requestAnimationFrame(animate);
        } else {
          dot.remove();
        }
      };
      requestAnimationFrame(animate);
    } catch (e) {
      if (!String(e).includes('destroyed')) {
        console.warn('G6 dot animation error:', e);
      }
    }
  }

  const updateMarking = useCallback((newMarking, highlightTrans) => {
    const graph = graphRef.current;
    if (!graph) return;
    try {
      const nodes = graph.getNodeData();
      const updated = nodes.map((n) => {
        if (n.data.type === 'place') {
          const idx = n.data.origId;
          const tokens = newMarking[idx] ?? 0;
          return { ...n, data: { ...n.data, tokens, label: n.data.label } };
        }
        return n;
      });

      if (highlightTrans !== undefined) {
        const hNodes = graph.getNodeData();
        const hTrans = hNodes.find((n) => n.data.type === 'transition' && n.data.origId === highlightTrans);
        const tid = hTrans ? hTrans.id : `t${highlightTrans}`;
        graph.setElementState(tid, ['firing']);
        setTimeout(() => {
          const g = graphRef.current;
          if (g) {
            try { g.setElementState(tid, []); } catch (e) { /* ignore */ }
          }
        }, 400);
      }

      graph.setData({ nodes: updated, edges: graph.getEdgeData() });
      graph.draw();
    } catch (e) {
      if (String(e).includes('destroyed')) return;
      console.warn('G6 update error:', e);
    }
  }, []);

  const destroyGraph = useCallback(() => {
    if (graphRef.current) {
      graphRef.current.destroy();
      graphRef.current = null;
      setGraphReady(false);
    }
  }, []);

  return {
    containerRef,
    graphRef,
    graphReady,
    initGraph,
    updateMarking,
    animateAndUpdate,
    destroyGraph,
  };
}
