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
        py: 1.5,
        borderRadius: 3,
        border: "1px solid",
        maxWidth: "100%",
        ...(variant === "user"
          ? {
              borderColor: alpha("#3D8B6E", 0.28),
              bgcolor: "linear-gradient(135deg, rgba(61,139,110,0.12), rgba(61,139,110,0.05))",
              borderBottomRightRadius: 8,
            }
          : {
              borderColor: "#E4E9F0",
              bgcolor: "#FFFFFF",
              borderBottomLeftRadius: 8,
              boxShadow: "0 2px 12px rgba(20,26,34,0.04)",
            }),
      }}
    >
      {children}
    </Box>
  );
}
