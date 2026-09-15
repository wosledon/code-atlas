import { Card, IconButton, Stack, TextField, alpha } from "@mui/material";
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
    <Card
      className="atlas-fade"
      sx={{
        position: "sticky",
        bottom: 16,
        p: 1.25,
        bgcolor: "rgba(255,255,255,0.92)",
        backdropFilter: "blur(10px)",
        boxShadow: "0 8px 32px rgba(20,26,34,0.08)",
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
          maxRows={4}
          variant="standard"
          InputProps={{
            disableUnderline: true,
            sx: { px: 1, py: 0.5, fontSize: 15 },
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
            "&:hover": { bgcolor: "primary.dark" },
            "&.Mui-disabled": { bgcolor: alpha("#1A6FB5", 0.35) },
          }}
        >
          <SendRoundedIcon fontSize="small" />
        </IconButton>
      </Stack>
    </Card>
  );
}
