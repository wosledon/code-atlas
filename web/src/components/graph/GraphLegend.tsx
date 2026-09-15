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
              bgcolor: "rgba(15,23,42,0.75)",
              color: colorOf(k),
              border: `1px solid ${alpha(colorOf(k), 0.4)}`,
              fontWeight: 600,
            }}
          />
        ))}
    </Box>
  );
}
