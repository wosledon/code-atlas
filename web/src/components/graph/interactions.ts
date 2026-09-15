import type { RefObject } from "react";
import type { DragState, SimNode, ViewTransform } from "./types";

export type InteractionRefs = {
  nodesRef: RefObject<SimNode[]>;
  transformRef: RefObject<ViewTransform>;
  dragRef: RefObject<DragState>;
  onSelect: (id: string | null) => void;
  onHover: (id: string | null) => void;
};

const HIT_RADIUS = 14;

/** Wire wheel-zoom, node dragging and background panning. Returns the cleanup function. */
export function attachInteractions(canvas: HTMLCanvasElement, refs: InteractionRefs) {
  const { nodesRef, transformRef, dragRef, onSelect, onHover } = refs;

  const toWorld = (clientX: number, clientY: number) => {
    const rect = canvas.getBoundingClientRect();
    const t = transformRef.current;
    const x = (clientX - rect.left - rect.width / 2 - t.x) / t.k;
    const y = (clientY - rect.top - rect.height / 2 - t.y) / t.k;
    return { x, y };
  };
  const hit = (wx: number, wy: number) => {
    const nodes = nodesRef.current;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const dx = n.x - wx;
      const dy = n.y - wy;
      if (dx * dx + dy * dy < HIT_RADIUS * HIT_RADIUS) return n;
    }
    return null;
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const t = transformRef.current;
    const factor = e.deltaY > 0 ? 0.92 : 1.08;
    t.k = Math.min(3.5, Math.max(0.35, t.k * factor));
  };
  const onDown = (e: MouseEvent) => {
    const { x, y } = toWorld(e.clientX, e.clientY);
    const n = hit(x, y);
    dragRef.current.lx = e.clientX;
    dragRef.current.ly = e.clientY;
    if (n) {
      dragRef.current.id = n.id;
      n.fx = n.x;
      n.fy = n.y;
      onSelect(n.id);
    } else {
      dragRef.current.panning = true;
      onSelect(null);
    }
  };
  const onMove = (e: MouseEvent) => {
    const { x, y } = toWorld(e.clientX, e.clientY);
    const n = hit(x, y);
    onHover(n?.id ?? null);
    canvas.style.cursor = n ? "pointer" : dragRef.current.panning ? "grabbing" : "grab";
    if (dragRef.current.id) {
      const node = nodesRef.current.find((v) => v.id === dragRef.current.id);
      if (node) {
        node.fx = x;
        node.fy = y;
      }
    } else if (dragRef.current.panning) {
      const t = transformRef.current;
      t.x += e.clientX - dragRef.current.lx;
      t.y += e.clientY - dragRef.current.ly;
    }
    dragRef.current.lx = e.clientX;
    dragRef.current.ly = e.clientY;
  };
  const onUp = () => {
    if (dragRef.current.id) {
      const node = nodesRef.current.find((v) => v.id === dragRef.current.id);
      if (node) {
        node.fx = null;
        node.fy = null;
      }
    }
    dragRef.current.id = null;
    dragRef.current.panning = false;
  };

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("mousedown", onDown);
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
  return () => {
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("mousedown", onDown);
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
}
