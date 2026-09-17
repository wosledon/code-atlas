import { Avatar, Box, CircularProgress, Stack, Typography } from "@mui/material";
import AutoAwesomeRoundedIcon from "@mui/icons-material/AutoAwesomeRounded";
import PersonOutlineRoundedIcon from "@mui/icons-material/PersonOutlineRounded";
import { MarkdownView } from "../MarkdownView";
import { ChatBubble } from "./ChatBubble";
import { RecalledContexts } from "./RecalledContexts";
import { SourceChips } from "./SourceChips";
import type { ChatTurn } from "./types";

const AVATAR_SX = { width: 34, height: 34 } as const;

/** One conversation turn, rendered as an avatar row with a bubble. */
export function ChatMessage({ turn: m }: { turn: ChatTurn }) {
  if (m.role === "user") {
    return (
      <Stack direction="row" justifyContent="flex-end" className="atlas-fade">
        <Stack direction="row" spacing={1} alignItems="flex-end" sx={{ maxWidth: "85%" }}>
          <ChatBubble variant="user">
            <Typography sx={{ whiteSpace: "pre-wrap", fontSize: 15, lineHeight: 1.55 }}>
              {m.content}
            </Typography>
          </ChatBubble>
          <Avatar
            sx={{
              ...AVATAR_SX,
              bgcolor: "secondary.main",
              boxShadow: "0 4px 12px rgba(61,139,110,0.3)",
            }}
          >
            <PersonOutlineRoundedIcon fontSize="small" />
          </Avatar>
        </Stack>
      </Stack>
    );
  }

  const isLlm = m.mode === "llm";
  return (
    <Stack direction="row" spacing={1} alignItems="flex-start" className="atlas-fade">
      <Avatar
        sx={{
          ...AVATAR_SX,
          bgcolor: isLlm ? "primary.main" : "#5A6678",
          boxShadow: isLlm ? "0 4px 12px rgba(26,111,181,0.3)" : "0 4px 12px rgba(90,102,120,0.2)",
        }}
      >
        <AutoAwesomeRoundedIcon fontSize="small" />
      </Avatar>
      <Box sx={{ flex: 1, minWidth: 0 }}>
        <ChatBubble variant="bot">
          {m.content ? (
            <MarkdownView source={m.content} />
          ) : (
            <Stack direction="row" spacing={1} alignItems="center">
              <CircularProgress size={14} thickness={5} />
              <Typography color="text.secondary" sx={{ fontSize: 14 }}>
                正在检索知识库…
              </Typography>
            </Stack>
          )}
          {m.sources && <SourceChips sources={m.sources} />}
          {m.contexts && m.contexts.length > 0 && <RecalledContexts contexts={m.contexts} />}
        </ChatBubble>
        {m.mode === "retrieval" && (
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ ml: 0.5, display: "block", mt: 0.75 }}
          >
            {m.error ? `检索模式 · 模型调用失败：${m.error}` : "检索模式 · 配置模型后可综合回答"}
          </Typography>
        )}
      </Box>
    </Stack>
  );
}
