import { Box, IconButton, Stack, TextField, alpha } from "@mui/material";
import SendRoundedIcon from "@mui/icons-material/SendRounded";
import type { RefObject } from "react";

/** Sticky input bar at the bottom of the chat page. */
export function ChatComposer({
  value,
  busy,
  inputRef,
  onChange,
  onSend,
}: {
  value: string;
  busy: boolean;
  inputRef: RefObject<HTMLDivElement | null>;
  onChange: (value: string) => void;
  onSend: () => void;
}) {
  return (
    <Box
      className="atlas-fade"
      sx={{
        position: "sticky",
        bottom: 16,
        zIndex: 5,
        p: 1,
        borderRadius: 1.5,
        bgcolor: "rgba(255,255,255,0.96)",
        border: "1px solid #E4E9F0",
        boxShadow: "0 8px 28px rgba(20,26,34,0.08)",
        backdropFilter: "blur(12px)",
      }}
    >
      <Stack direction="row" spacing={1} alignItems="flex-end">
        <TextField
          inputRef={inputRef}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              onSend();
            }
          }}
          placeholder="问仓库任何事…"
          fullWidth
          multiline
          maxRows={5}
          variant="standard"
          InputProps={{
            disableUnderline: true,
            sx: {
              px: 1.25,
              py: 0.75,
              fontSize: 15,
              lineHeight: 1.5,
              bgcolor: "transparent",
            },
          }}
        />
        <IconButton
          color="primary"
          onClick={onSend}
          disabled={busy || !value.trim()}
          sx={{
            bgcolor: "primary.main",
            color: "#fff",
            width: 40,
            height: 40,
            borderRadius: 1.5,
            flexShrink: 0,
            boxShadow: "0 4px 12px rgba(26,111,181,0.28)",
            "&:hover": { bgcolor: "primary.dark" },
            "&.Mui-disabled": {
              bgcolor: alpha("#1A6FB5", 0.28),
              color: "rgba(255,255,255,0.9)",
            },
          }}
        >
          <SendRoundedIcon fontSize="small" />
        </IconButton>
      </Stack>
    </Box>
  );
}
