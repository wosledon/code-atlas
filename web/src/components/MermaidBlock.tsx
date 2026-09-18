import { useEffect, useState } from "react";
import { Box, CircularProgress, Dialog, DialogContent, Typography } from "@mui/material";
import { useDiagramRender } from "./mermaid/useDiagramRender";
import { DiagramToolbar } from "./mermaid/DiagramToolbar";
import { DiagramCanvas } from "./mermaid/DiagramCanvas";

export function MermaidBlock({ code, pending = false }: { code: string; pending?: boolean }) {
  const { svg, err, repaired } = useDiagramRender(code, pending);
  const [zoom, setZoom] = useState(1);
  const [panKey, setPanKey] = useState(0);
  const [fullscreen, setFullscreen] = useState(false);
  const [copied, setCopied] = useState(false);

  const fit = () => {
    setZoom(1);
    setPanKey((k) => k + 1);
  };

  const copy = () => {
    void navigator.clipboard?.writeText(code).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };

  useEffect(() => {
    if (!fullscreen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setFullscreen(false);
    };
    window.addEventListener("keydown", onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prev;
    };
  }, [fullscreen]);

  if (pending) {
    return (
      <Box
        sx={{
          border: "1px dashed",
          borderColor: "divider",
          borderRadius: 1.5,
          my: 2,
          py: 6,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 1,
          color: "text.disabled",
        }}
      >
        <CircularProgress size={14} thickness={5} />
        <Typography variant="caption">图表生成中…</Typography>
      </Box>
    );
  }

  if (err) {
    return (
      <Box
        sx={{
          background: "#FFF7F7",
          border: "1px solid #F0C4C4",
          borderRadius: 1.5,
          p: 1.5,
          my: 2,
        }}
      >
        <Typography variant="caption" color="error" sx={{ display: "block", mb: 1 }}>
          Mermaid 解析失败：{err}
        </Typography>
        <Box
          component="pre"
          sx={{
            m: 0,
            fontSize: 12,
            overflow: "auto",
            color: "text.secondary",
            fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
          }}
        >
          {code}
        </Box>
      </Box>
    );
  }

  const toolbar = (
    <DiagramToolbar
      zoom={zoom}
      copied={copied}
      fullscreen={fullscreen}
      onZoomIn={() => setZoom((z) => Math.min(5, +(z + 0.2).toFixed(2)))}
      onZoomOut={() => setZoom((z) => Math.max(0.25, +(z - 0.2).toFixed(2)))}
      onFit={fit}
      onCopy={copy}
      onFullscreen={() => setFullscreen(true)}
      onCloseFullscreen={() => setFullscreen(false)}
    />
  );

  return (
    <Box sx={{ my: 2 }}>
      <Box
        sx={{
          border: "1px solid",
          borderColor: "divider",
          borderRadius: 1.5,
          overflow: "hidden",
          bgcolor: "background.paper",
          height: { xs: 320, sm: 380, md: 420 },
          display: "flex",
          flexDirection: "column",
        }}
      >
        {toolbar}
        <DiagramCanvas key={panKey} svg={svg} zoom={zoom} onZoomChange={setZoom} />
      </Box>
      {repaired && (
        <Typography variant="caption" color="text.disabled" sx={{ display: "block", mt: 0.75 }}>
          已自动修正该图的 mermaid 语法
        </Typography>
      )}

      <Dialog
        open={fullscreen}
        onClose={() => setFullscreen(false)}
        fullScreen
        slotProps={{
          paper: {
            sx: {
              bgcolor: "rgba(247,248,250,0.96)",
              backdropFilter: "blur(10px)",
              m: 0,
              borderRadius: 0,
              height: "100%",
              maxHeight: "100%",
            },
          },
        }}
      >
        <DialogContent
          sx={{ p: 0, display: "flex", flexDirection: "column", minHeight: 0, height: "100%" }}
        >
          <DiagramToolbar
            zoom={zoom}
            copied={copied}
            fullscreen
            onZoomIn={() => setZoom((z) => Math.min(5, +(z + 0.2).toFixed(2)))}
            onZoomOut={() => setZoom((z) => Math.max(0.25, +(z - 0.2).toFixed(2)))}
            onFit={fit}
            onCopy={copy}
            onFullscreen={() => setFullscreen(true)}
            onCloseFullscreen={() => setFullscreen(false)}
          />
          <DiagramCanvas key={`fs-${panKey}`} svg={svg} zoom={zoom} onZoomChange={setZoom} />
        </DialogContent>
      </Dialog>
    </Box>
  );
}
