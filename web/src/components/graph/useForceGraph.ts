import { useEffect, useRef, useState } from "react";
import type { GraphData } from "../../lib/api";
import { attachInteractions } from "./interactions";
import { seedGraph, stepPhysics } from "./layout";
import { drawScene, fitCanvas } from "./render";
import type { DragState, SimEdge, SimNode, ViewTransform } from "./types";

/**
 * Owns the canvas graph: simulation state lives in refs (mutated 60×/s by the
 * animation loop), while selection/hover/statistics are React state so the page
 * re-renders when they change.
 */
export function useForceGraph(data: GraphData | null, kindFilter: string) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const nodesRef = useRef<SimNode[]>([]);
  const edgesRef = useRef<SimEdge[]>([]);
  const transformRef = useRef<ViewTransform>({ k: 1, x: 0, y: 0 });
  const dragRef = useRef<DragState>({ id: null, panning: false, lx: 0, ly: 0 });
  const rafRef = useRef(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [stats, setStats] = useState({ n: 0, e: 0 });

  // init simulation when data/filter changes
  useEffect(() => {
    if (!data) return;
    const { nodes, edges } = seedGraph(data, kindFilter);
    nodesRef.current = nodes;
    edgesRef.current = edges;
    setStats({ n: nodes.length, e: edges.length });
    // center transform
    transformRef.current = { k: 1, x: 0, y: 0 };
  }, [data, kindFilter]);

  // force simulation + render loop
  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const resize = () => fitCanvas(canvas, wrap, ctx);
    resize();
    window.addEventListener("resize", resize);

    const tick = () => {
      const nodes = nodesRef.current;
      const edges = edgesRef.current;
      stepPhysics(nodes, edges);
      drawScene(ctx, canvas, {
        nodes,
        edges,
        transform: transformRef.current,
        selected,
        hover,
      });
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(rafRef.current);
      window.removeEventListener("resize", resize);
    };
  }, [selected, hover]);

  // interactions
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    return attachInteractions(canvas, {
      nodesRef,
      transformRef,
      dragRef,
      onSelect: setSelected,
      onHover: setHover,
    });
  }, []);

  return { canvasRef, wrapRef, stats, selected, hover };
}
