import { useState } from "react";
import { Box, Chip, Collapse, IconButton, Stack, Typography, alpha } from "@mui/material";
import ExpandMoreRoundedIcon from "@mui/icons-material/ExpandMoreRounded";
import OpenInNewRoundedIcon from "@mui/icons-material/OpenInNewRounded";
import { Link } from "react-router-dom";
import type { ChatContext } from "../../lib/api";
import { readerHref } from "../../lib/routes";
import { MarkdownView } from "../MarkdownView";

/** One recalled chunk: header with provenance metadata, body revealed on click. */
export function RecallCard({ context: c }: { context: ChatContext }) {
  const [open, setOpen] = useState(false);
  return (
    <Box
      sx={{
        border: "1px solid #E4E9F0",
        borderRadius: 2,
        bgcolor: "#fff",
        overflow: "hidden",
      }}
    >
      <Stack
        direction="row"
        spacing={0.75}
        alignItems="center"
        onClick={() => setOpen((v) => !v)}
        sx={{
          px: 1.25,
          py: 0.75,
          cursor: "pointer",
          userSelect: "none",
          bgcolor: "#FAFBFD",
          "&:hover": { bgcolor: "#F4F6FA" },
        }}
      >
        <ExpandMoreRoundedIcon
          sx={{
            fontSize: 17,
            color: "text.secondary",
            flexShrink: 0,
            transform: open ? "rotate(180deg)" : "none",
            transition: "transform 0.2s ease",
          }}
        />
        <Typography
          variant="caption"
          sx={{ fontWeight: 600, minWidth: 0, flexGrow: 1 }}
          noWrap
          title={c.title || c.page_path || ""}
        >
          {c.title || c.id || "(无标题)"}
        </Typography>
        {c.page_path && (
          <Chip
            size="small"
            variant="outlined"
            label={c.page_path}
            title={c.page_path}
            sx={{
              height: 20,
              borderRadius: 1,
              maxWidth: 220,
              fontSize: 11,
              fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
              flexShrink: 0,
            }}
          />
        )}
        {typeof c.score === "number" && (
          <Chip
            size="small"
            label={`score ${c.score.toFixed(2)}`}
            title="相似度得分"
            sx={{
              height: 20,
              borderRadius: 1,
              bgcolor: alpha("#1A6FB5", 0.1),
              fontSize: 11,
              flexShrink: 0,
            }}
          />
        )}
        {c.start_line != null && (
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ fontFamily: '"IBM Plex Mono", ui-monospace, monospace', flexShrink: 0 }}
          >
            L{c.start_line}-{c.end_line ?? c.start_line}
          </Typography>
        )}
        {c.page_path && (
          <IconButton
            size="small"
            component={Link}
            to={readerHref(c.page_path)}
            title="打开所在文档"
            onClick={(e) => e.stopPropagation()}
          >
            <OpenInNewRoundedIcon sx={{ fontSize: 15 }} />
          </IconButton>
        )}
      </Stack>
      <Collapse in={open} timeout="auto" unmountOnExit>
        <Box
          sx={{
            maxHeight: 260,
            overflow: "auto",
            bgcolor: "#F7F9FC",
            borderTop: "1px solid #EDF0F5",
            borderRadius: "0 0 8px 8px",
            px: 1.5,
            py: 1.25,
          }}
        >
          <MarkdownView source={c.body || "（无内容）"} />
        </Box>
      </Collapse>
    </Box>
  );
}
