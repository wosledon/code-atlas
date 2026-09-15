import { Box, Chip, Stack, Typography } from "@mui/material";
import SourceRoundedIcon from "@mui/icons-material/SourceRounded";
import { Link } from "react-router-dom";
import type { ChatSource } from "../../lib/api";
import { readerHref } from "../../lib/routes";

/** The wiki pages a chat answer was grounded in. */
export function SourceChips({ sources }: { sources: ChatSource[] }) {
  if (sources.length === 0) return null;
  return (
    <Box sx={{ mt: 2, pt: 1.5, borderTop: "1px dashed #E4E9F0" }}>
      <Stack direction="row" alignItems="center" spacing={0.75} sx={{ mb: 1 }}>
        <SourceRoundedIcon sx={{ fontSize: 16, color: "text.secondary" }} />
        <Typography variant="caption" color="text.secondary" fontWeight={600}>
          关联文件
        </Typography>
      </Stack>
      <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
        {sources.slice(0, 6).map((s, j) => (
          <Chip
            key={j}
            size="small"
            label={s.page_path || s.title}
            variant="outlined"
            {...(s.page_path
              ? {
                  component: Link,
                  to: readerHref(s.page_path),
                  clickable: true,
                }
              : {})}
            sx={{
              borderRadius: 1.5,
              bgcolor: "#fff",
              maxWidth: 280,
              "& .MuiChip-label": {
                overflow: "hidden",
                textOverflow: "ellipsis",
              },
            }}
          />
        ))}
      </Stack>
    </Box>
  );
}
