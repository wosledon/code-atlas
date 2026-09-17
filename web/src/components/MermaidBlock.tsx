import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Dialog,
  DialogContent,
  IconButton,
  Stack,
  Tooltip,
  Typography,
} from "@mui/material";
import ZoomInIcon from "@mui/icons-material/ZoomIn";
import ZoomOutIcon from "@mui/icons-material/ZoomOut";
import CenterFocusStrongIcon from "@mui/icons-material/CenterFocusStrong";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import CheckIcon from "@mui/icons-material/Check";
import OpenInFullIcon from "@mui/icons-material/OpenInFull";
import CloseIcon from "@mui/icons-material/Close";
import mermaid from "mermaid";
import {
  cleanupMermaidArtifacts,
  mermaidErrorBrief,
  repairMermaid,
} from "../lib/mermaid";

type Pan = { x: number; y: number };

function useDiagramRender(code: string) {
  const [svg, setSvg] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [repaired, setRepaired] = useState(false);
  const id = useMemo(() => `mmd-${Math.random().toString(36).slice(2, 9)}`, []);

  useEffect(() => {
    let cancelled = false;
    const show = (markup: string, wasRepaired: boolean) => {
      if (cancelled) return;
      setSvg(markup);
      setErr(null);
      setRepaired(wasRepaired);
    };
    (async () => {
      let failure: unknown;
      try {
        show((await mermaid.render(id, code)).svg, false);
        return;
      } catch (e) {
        failure = e;
      }
      const fix = repairMermaid(code);
      if (fix.edits > 0) {
        try {
          show((await mermaid.render(`${id}r`, fix.code)).svg, true);
          cleanupMermaidArtifacts(id);
          return;
        } catch {
          cleanupMermaidArtifacts(`${id}r`);
        }
      }
      cleanupMermaidArtifacts(id);
      if (cancelled) return;
      setSvg("");
      setRepaired(false);
      setErr(mermaidErrorBrief(failure));
    })();
    return () => {
      cancelled = true;
    };
  }, [code, id]);

  return { svg, err, repaired };
}

/** Icon-only toolbar for the diagram canvas. */
function DiagramToolbar({
  zoom,
  onZoomIn,
  onZoomOut,
  onFit,
  onCopy,
  onFullscreen,
  onCloseFullscreen,
  fullscreen,
  copied,
}: {
  zoom: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onCopy: () => void;
  onFullscreen: () => void;
  onCloseFullscreen?: () => void;
  fullscreen?: boolean;
  copied: boolean;
}) {
  return (
    <Stack
      direction="row"
      alignItems="center"
      spacing={0.25}
      sx={{
        px: 0.75,
        py: 0.5,
        borderBottom: "1px solid",
        borderColor: "divider",
        bgcolor: "rgba(255,255,255,0.85)",
        backdropFilter: "blur(8px)",
        flexShrink: 0,
        userSelect: "none",
      }}
    >
      <Tooltip title="缩小">
        <IconButton size="small" onClick={onZoomOut}>
          <ZoomOutIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Typography
        variant="caption"
        sx={{ minWidth: 44, textAlign: "center", color: "text.secondary", fontVariantNumeric: "tabular-nums" }}
      >
        {Math.round(zoom * 100)}%
      </Typography>
      <Tooltip title="放大">
        <IconButton size="small" onClick={onZoomIn}>
          <ZoomInIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Tooltip title="适应视图">
        <IconButton size="small" onClick={onFit}>
          <CenterFocusStrongIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Tooltip title={copied ? "已复制" : "复制源码"}>
        <IconButton size="small" onClick={onCopy}>
          {copied ? <CheckIcon fontSize="small" color="success" /> : <ContentCopyIcon fontSize="small" />}
        </IconButton>
      </Tooltip>
      {fullscreen ? (
        <Tooltip title="关闭 (Esc)">
          <IconButton size="small" onClick={onCloseFullscreen}>
            <CloseIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      ) : (
        <Tooltip title="全屏">
          <IconButton size="small" onClick={onFullscreen}>
            <OpenInFullIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      )}
    </Stack>
  );
}

function DiagramCanvas({
  svg,
  zoom,
  onZoomChange,
  dark,
}: {
  svg: string;
  zoom: number;
  onZoomChange: (z: number) => void;
  dark?: boolean;
}) {
  const viewRef = useRef<HTMLDivElement | null>(null);
  const [pan, setPan] = useState<Pan>({ x: 0, y: 0 });
  const dragRef = useRef<{ x: number; y: number; ox: number; oy: number } | null>(null);
  const zoomRef = useRef(zoom);

  const clampZoom = (z: number) => Math.min(5, Math.max(0.25, +z.toFixed(2)));

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragRef.current = { x: e.clientX, y: e.clientY, ox: pan.x, oy: pan.y };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    setPan({ x: d.ox + (e.clientX - d.x), y: d.oy + (e.clientY - d.y) });
  };
  const onPointerUp = (e: React.PointerEvent) => {
    dragRef.current = null;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  };

  const onWheel = useCallback(
    (e: WheelEvent) => {
      e.preventDefault();
      const next = clampZoom(zoomRef.current * (e.deltaY > 0 ? 0.92 : 1.08));
      zoomRef.current = next;
      onZoomChange(next);
    },
    [onZoomChange]
  );

  useEffect(() => {
    zoomRef.current = zoom;
  }, [zoom]);

  useEffect(() => {
    const el = viewRef.current;
    if (!el) return;
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [onWheel]);

  return (
    <Box
      ref={viewRef}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={() => {
        setPan({ x: 0, y: 0 });
        onZoomChange(1);
      }}
      sx={{
        flexGrow: 1,
        minHeight: 0,
        overflow: "hidden",
        cursor: "grab",
        "&:active": { cursor: "grabbing" },
        touchAction: "none",
        userSelect: "none",
        WebkitUserSelect: "none",
        bgcolor: dark ? "#0d1117" : "linear-gradient(180deg, #FBFCFE 0%, #F7F9FC 100%)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        position: "relative",
        // Mermaid SVG text nodes otherwise become selectable while dragging.
        "& svg": {
          userSelect: "none",
          WebkitUserSelect: "none",
          pointerEvents: "none",
        },
        "& text, & tspan": {
          userSelect: "none",
          WebkitUserSelect: "none",
        },
      }}
    >
      <Box
        sx={{
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: "center center",
          width: "100%",
          display: "flex",
          justifyContent: "center",
          pointerEvents: "none",
          "& svg": {
            maxWidth: "100%",
            height: "auto",
            display: "block",
          },
        }}
        dangerouslySetInnerHTML={{ __html: svg }}
      />
      <Typography
        variant="caption"
        sx={{
          position: "absolute",
          left: 10,
          bottom: 8,
          color: dark ? "rgba(255,255,255,0.45)" : "text.disabled",
          pointerEvents: "none",
        }}
      >
        拖拽平移 · 滚轮缩放 · 双击复位
      </Typography>
    </Box>
  );
}

export function MermaidBlock({ code }: { code: string }) {
  const { svg, err, repaired } = useDiagramRender(code);
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
        <DialogContent sx={{ p: 0, display: "flex", flexDirection: "column", minHeight: 0, height: "100%" }}>
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
