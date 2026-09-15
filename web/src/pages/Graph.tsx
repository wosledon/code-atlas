import { useEffect, useMemo, useState } from "react";
import { Box, Card, Stack, Typography } from "@mui/material";
import { api, GraphData } from "../lib/api";
import { GraphLegend } from "../components/graph/GraphLegend";
import { KindFilter } from "../components/graph/KindFilter";
import { useForceGraph } from "../components/graph/useForceGraph";

/** Obsidian-like force graph: zoom/pan/drag, dark canvas, soft links. */
export default function GraphPage() {
  const [data, setData] = useState<GraphData | null>(null);
  const [kindFilter, setKindFilter] = useState("all");

  useEffect(() => {
    api<GraphData>("/api/graph/nodes")
      .then((d) => {
        setData(d);
      })
      .catch(() => setData({ nodes: [], edges: [] }));
  }, []);

  const kinds = useMemo(
    () => Array.from(new Set((data?.nodes || []).map((n) => n.kind))),
    [data]
  );

  const { canvasRef, wrapRef, stats } = useForceGraph(data, kindFilter);

  return (
    <Stack spacing={2}>
      <Stack
        direction={{ xs: "column", sm: "row" }}
        justifyContent="space-between"
        alignItems={{ sm: "center" }}
        spacing={2}
      >
        <div>
          <Typography variant="h4">知识图谱</Typography>
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            Obsidian 风格 · 拖拽节点 / 滚轮缩放 / 拖空白平移 · {stats.n} 节点 · {stats.e} 边
          </Typography>
        </div>
        <KindFilter kinds={kinds} value={kindFilter} onChange={setKindFilter} />
      </Stack>

      <Card sx={{ overflow: "hidden", bgcolor: "#0d1117", borderColor: "#1f2937" }}>
        <Box ref={wrapRef} sx={{ position: "relative", width: "100%" }}>
          <canvas ref={canvasRef} style={{ display: "block", width: "100%", cursor: "grab" }} />
          <GraphLegend kinds={kinds} kindFilter={kindFilter} />
        </Box>
      </Card>
    </Stack>
  );
}
