/**
 * Removes overlaps left by a force layout. Each box is { x, y, w, h } (centre +
 * size, including the label area). Boxes are pushed apart along the axis of least
 * penetration, a few times over, until nothing overlaps (or the budget runs out).
 * Mutates and returns the boxes. O(n²) per pass, so it is skipped for huge nets.
 */
export function resolveOverlaps(boxes, { padding = 10, maxPasses = 300, maxNodes = 800 } = {}) {
  if (boxes.length > maxNodes) return boxes;
  for (let pass = 0; pass < maxPasses; pass += 1) {
    let moved = false;
    for (let i = 0; i < boxes.length; i += 1) {
      for (let j = i + 1; j < boxes.length; j += 1) {
        const a = boxes[i];
        const b = boxes[j];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const overlapX = (a.w + b.w) / 2 + padding - Math.abs(dx);
        const overlapY = (a.h + b.h) / 2 + padding - Math.abs(dy);
        if (overlapX <= 0 || overlapY <= 0) continue;
        moved = true;
        // Separate along the cheaper axis; identical centres get a deterministic nudge.
        if (overlapX < overlapY) {
          const dir = dx === 0 ? (i % 2 ? 1 : -1) : Math.sign(dx);
          const shift = overlapX / 2 + 0.5;
          a.x -= dir * shift;
          b.x += dir * shift;
        } else {
          const dir = dy === 0 ? (j % 2 ? 1 : -1) : Math.sign(dy);
          const shift = overlapY / 2 + 0.5;
          a.y -= dir * shift;
          b.y += dir * shift;
        }
      }
    }
    if (!moved) break;
  }
  return boxes;
}
