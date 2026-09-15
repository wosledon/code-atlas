import type { GraphData } from "../../lib/api";
import type { SimEdge, SimNode } from "./types";

/** Scatter the selected nodes on a spiral and drop edges whose endpoints were filtered out. */
export function seedGraph(data: GraphData, kindFilter: string) {
  const nodes: SimNode[] = data.nodes
    .filter((n) => kindFilter === "all" || n.kind === kindFilter)
    .map((n, i) => {
      const angle = (i / Math.max(1, data.nodes.length)) * Math.PI * 2;
      const r = 80 + (i % 7) * 36;
      return {
        id: n.id,
        kind: n.kind,
        name: n.name,
        x: Math.cos(angle) * r + (Math.random() - 0.5) * 40,
        y: Math.sin(angle) * r + (Math.random() - 0.5) * 40,
        vx: 0,
        vy: 0,
        fx: null,
        fy: null,
      };
    });
  const ids = new Set(nodes.map((n) => n.id));
  const edges: SimEdge[] = data.edges
    .filter((e) => ids.has(e.source) && ids.has(e.target))
    .map((e) => ({ source: e.source, target: e.target, rel: e.rel }));
  return { nodes, edges };
}

/** One simulation step: pairwise repulsion, spring links, gravity, damping, then integrate. */
export function stepPhysics(nodes: SimNode[], edges: SimEdge[]) {
  for (let i = 0; i < nodes.length; i++) {
    const a = nodes[i];
    for (let j = i + 1; j < nodes.length; j++) {
      const b = nodes[j];
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist2 = dx * dx + dy * dy || 0.01;
      const dist = Math.sqrt(dist2);
      const rep = 900 / dist2;
      const fx = (dx / dist) * rep;
      const fy = (dy / dist) * rep;
      a.vx -= fx;
      a.vy -= fy;
      b.vx += fx;
      b.vy += fy;
    }
  }

  const byId = new Map(nodes.map((n) => [n.id, n]));
  edges.forEach((e) => {
    const a = byId.get(e.source);
    const b = byId.get(e.target);
    if (!a || !b) return;
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
    const ideal = 110;
    const k = 0.012;
    const f = (dist - ideal) * k;
    a.vx += (dx / dist) * f;
    a.vy += (dy / dist) * f;
    b.vx -= (dx / dist) * f;
    b.vy -= (dy / dist) * f;
  });

  nodes.forEach((n) => {
    n.vx += -n.x * 0.0015;
    n.vy += -n.y * 0.0015;
    n.vx *= 0.86;
    n.vy *= 0.86;
    if (n.fx != null && n.fy != null) {
      n.x = n.fx;
      n.y = n.fy;
      n.vx = 0;
      n.vy = 0;
    } else {
      n.x += n.vx;
      n.y += n.vy;
    }
  });
}
