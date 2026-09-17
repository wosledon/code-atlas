import { useEffect, useMemo, useState } from "react";
import { Box, Stack, Typography } from "@mui/material";
import { api, GraphData } from "../lib/api";
import { GraphLegend } from "../components/graph/GraphLegend";
import { KindFilter } from "../components/graph/KindFilter";
import { useForceGraph } from "../components/graph/useForceGraph";

/** Force graph: zoom/pan/drag, light canvas, soft links. Fills the app shell. */
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
    <Box
      sx={{
        height: { md: "100%" },
        minHeight: 0,
        flexGrow: { md: 1 },
        display: "flex",
        flexDirection: "column",
      }}
    >
      <Stack
        direction={{ xs: "column", sm: "row" }}
        justifyContent="space-between"
        alignItems={{ sm: "center" }}
        spacing={1.5}
        sx={{
          flexShrink: 0,
          px: { xs: 2, md: 3 },
          pt: 0.75,
          pb: 1.25,
          borderBottom: 1,
          borderColor: "divider",
          bgcolor: "rgba(255,255,255,0.55)",
          backdropFilter: "blur(8px)",
        }}
      >
        <Stack direction="row" alignItems="baseline" spacing={1.5} flexWrap="wrap">
          <Typography variant="h5" sx={{ fontWeight: 700 }}>
            知识图谱
          </Typography>
          <Typography variant="body2" color="text.secondary">
            拖拽 / 缩放 / 平移 · {stats.n} 节点 · {stats.e} 边
          </Typography>
        </Stack>
        <KindFilter kinds={kinds} value={kindFilter} onChange={setKindFilter} />
      </Stack>

      <Box
        ref={wrapRef}
        sx={{
          position: "relative",
          flexGrow: 1,
          minHeight: { xs: 480, md: 0 },
          width: "100%",
          bgcolor: "#F0F3F8",
          overflow: "hidden",
        }}
      >
        <canvas ref={canvasRef} style={{ display: "block", width: "100%", cursor: "grab" }} />
        <GraphLegend kinds={kinds} kindFilter={kindFilter} />
      </Box>
    </Box>
  );
}
