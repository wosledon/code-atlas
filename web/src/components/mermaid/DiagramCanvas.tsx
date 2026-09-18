import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Typography } from "@mui/material";

type Pan = { x: number; y: number };

export function DiagramCanvas({
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
