import { alpha } from "@mui/material";
import type { SimEdge, SimNode, ViewTransform } from "./types";
import { colorOf } from "./types";

/** Size the backing store to the element box so drawing stays crisp on HiDPI screens. */
export function fitCanvas(
  canvas: HTMLCanvasElement,
  wrap: HTMLElement,
  ctx: CanvasRenderingContext2D
) {
  const dpr = window.devicePixelRatio || 1;
  const w = wrap.clientWidth;
  const h = Math.max(560, Math.min(720, window.innerHeight - 220));
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

export type Scene = {
  nodes: SimNode[];
  edges: SimEdge[];
  transform: ViewTransform;
  selected: string | null;
  hover: string | null;
};

/** Paint one frame: dark gradient background, links, glowing nodes with labels. */
export function drawScene(
  ctx: CanvasRenderingContext2D,
  canvas: HTMLCanvasElement,
  scene: Scene
) {
  const { nodes, edges, transform: t, selected, hover } = scene;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  ctx.clearRect(0, 0, w, h);
  // background gradient like Obsidian
  const g = ctx.createRadialGradient(w * 0.5, h * 0.45, 20, w * 0.5, h * 0.5, Math.max(w, h) * 0.7);
  g.addColorStop(0, "#1a1f2e");
  g.addColorStop(1, "#0d1117");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);

  ctx.save();
  ctx.translate(w / 2 + t.x, h / 2 + t.y);
  ctx.scale(t.k, t.k);

  const selectedId = selected;
  const related = new Set<string>();
  if (selectedId) {
    related.add(selectedId);
    edges.forEach((e) => {
      if (e.source === selectedId) related.add(e.target);
      if (e.target === selectedId) related.add(e.source);
    });
  }

  const byId = new Map(nodes.map((n) => [n.id, n]));

  // edges
  edges.forEach((e) => {
    const a = byId.get(e.source);
    const b = byId.get(e.target);
    if (!a || !b) return;
    const active = selectedId && (e.source === selectedId || e.target === selectedId);
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.strokeStyle = active ? "rgba(124,184,245,0.9)" : "rgba(148,163,184,0.18)";
    ctx.lineWidth = active ? 1.6 / t.k : 0.8 / t.k;
    ctx.stroke();
  });

  // nodes
  nodes.forEach((n) => {
    const c = colorOf(n.kind);
    const isSel = n.id === selectedId;
    const isHov = n.id === hover;
    const dim = selectedId && !related.has(n.id);
    const r = (isSel || isHov ? 9 : 6.5) / Math.min(1.2, Math.max(0.8, t.k));
    ctx.beginPath();
    ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
    if (dim) {
      ctx.fillStyle = alpha(c, 0.25);
    } else {
      const grd = ctx.createRadialGradient(n.x - r * 0.3, n.y - r * 0.3, r * 0.1, n.x, n.y, r);
      grd.addColorStop(0, "#fff");
      grd.addColorStop(0.25, c);
      grd.addColorStop(1, alpha(c, 0.85));
      ctx.fillStyle = grd;
    }
    ctx.shadowColor = dim ? "transparent" : alpha(c, 0.55);
    ctx.shadowBlur = isSel || isHov ? 18 : 8;
    ctx.fill();
    ctx.shadowBlur = 0;
    // ring
    ctx.beginPath();
    ctx.arc(n.x, n.y, r + 1.5, 0, Math.PI * 2);
    ctx.strokeStyle = isSel ? "#fff" : alpha(c, 0.5);
    ctx.lineWidth = isSel ? 1.5 / t.k : 0.6 / t.k;
    ctx.stroke();
    // label
    if (t.k > 0.55 && (!selectedId || related.has(n.id) || isHov)) {
      ctx.font = `${11 / Math.min(t.k, 1.2)}px "IBM Plex Sans", "Segoe UI", sans-serif`;
      ctx.fillStyle = dim ? "rgba(226,232,240,0.25)" : "rgba(226,232,240,0.92)";
      ctx.textAlign = "center";
      ctx.fillText(n.name.length > 18 ? n.name.slice(0, 17) + "…" : n.name, n.x, n.y + r + 12);
    }
  });
  ctx.restore();
}
