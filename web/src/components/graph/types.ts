export type SimNode = {
  id: string;
  kind: string;
  name: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx?: number | null;
  fy?: number | null;
};

export type SimEdge = { source: string; target: string; rel: string };

/** Pan/zoom state of the canvas: scale plus translation in device-independent px. */
export type ViewTransform = { k: number; x: number; y: number };

export type DragState = { id: string | null; panning: boolean; lx: number; ly: number };

const KIND_COLORS: Record<string, string> = {
  page: "#7CB8F5",
  module: "#5CBF8A",
  symbol: "#C39BD3",
  api: "#F0C674",
  data: "#E57373",
  config: "#9AA7B8",
  external: "#6EC6E6",
};

export const colorOf = (kind: string) => KIND_COLORS[kind] || "#9AA7B8";
