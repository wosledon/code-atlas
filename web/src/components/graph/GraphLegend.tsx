import { Box, Chip, alpha } from "@mui/material";
import { colorOf } from "./types";

/** Colour key overlay drawn on top of the canvas. */
export function GraphLegend({ kinds, kindFilter }: { kinds: string[]; kindFilter: string }) {
  return (
    <Box
      sx={{
        position: "absolute",
        left: 16,
        bottom: 14,
        display: "flex",
        gap: 1,
        flexWrap: "wrap",
        pointerEvents: "none",
      }}
    >
      {kinds
        .filter((k) => kindFilter === "all" || k === kindFilter)
        .slice(0, 6)
        .map((k) => (
          <Chip
            key={k}
            size="small"
            label={k}
            sx={{
              bgcolor: "rgba(255,255,255,0.88)",
              color: colorOf(k),
              border: `1px solid ${alpha(colorOf(k), 0.35)}`,
              fontWeight: 600,
              boxShadow: "0 2px 8px rgba(16,24,40,0.08)",
            }}
          />
        ))}
    </Box>
  );
}
