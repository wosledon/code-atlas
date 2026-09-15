import { useState } from "react";
import { Box, Collapse, Stack, Typography } from "@mui/material";
import ArticleOutlinedIcon from "@mui/icons-material/ArticleOutlined";
import ExpandMoreRoundedIcon from "@mui/icons-material/ExpandMoreRounded";
import type { ChatContext } from "../../lib/api";
import { RecallCard } from "./RecallCard";

// Shows the raw chunks the retriever fed to the model, so answers can be traced back to source text.
export function RecalledContexts({ contexts }: { contexts: ChatContext[] }) {
  const [open, setOpen] = useState(false);
  return (
    <Box sx={{ mt: 2, pt: 1.5, borderTop: "1px dashed #E4E9F0" }}>
      <Stack
        direction="row"
        alignItems="center"
        spacing={0.75}
        onClick={() => setOpen((v) => !v)}
        sx={{ cursor: "pointer", userSelect: "none" }}
      >
        <ArticleOutlinedIcon sx={{ fontSize: 16, color: "text.secondary" }} />
        <Typography variant="caption" color="text.secondary" fontWeight={600}>
          召回内容 ({contexts.length})
        </Typography>
        <ExpandMoreRoundedIcon
          sx={{
            fontSize: 18,
            color: "text.secondary",
            transform: open ? "rotate(180deg)" : "none",
            transition: "transform 0.2s ease",
          }}
        />
      </Stack>
      <Collapse in={open} timeout="auto" unmountOnExit>
        <Stack spacing={1.25} sx={{ mt: 1.25 }}>
          {contexts.map((c, i) => (
            <RecallCard key={c.id || i} context={c} />
          ))}
        </Stack>
      </Collapse>
    </Box>
  );
}
