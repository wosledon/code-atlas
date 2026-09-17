import { Box, alpha } from "@mui/material";
import type { ReactNode } from "react";

/** Rounded speech bubble shared by the user and assistant sides of the thread. */
export function ChatBubble({
  children,
  variant,
}: {
  children: ReactNode;
  variant: "user" | "bot";
}) {
  return (
    <Box
      sx={{
        px: 2,
        py: 1.35,
        borderRadius: 2,
        border: "1px solid",
        maxWidth: "100%",
        minWidth: 0,
        ...(variant === "user"
          ? {
              borderColor: alpha("#3D8B6E", 0.3),
              bgcolor: "#EEF6F3",
              color: "#141A22",
              borderBottomRightRadius: 6,
            }
          : {
              borderColor: "#E4E9F0",
              bgcolor: "#FFFFFF",
              color: "#141A22",
              borderBottomLeftRadius: 6,
              boxShadow: "0 1px 2px rgba(20,26,34,0.04)",
            }),
        // Markdown inside a bubble shouldn't blow out the layout.
        "& .md-view": {
          fontSize: 14.5,
          lineHeight: 1.6,
        },
        "& .md-view pre": {
          maxWidth: "100%",
        },
        "& .md-view table": {
          display: "block",
          overflowX: "auto",
        },
      }}
    >
      {children}
    </Box>
  );
}
