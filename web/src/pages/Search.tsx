import { useEffect, useRef, useState } from "react";
import { Box, Card, Stack, Typography } from "@mui/material";
import AutoAwesomeRoundedIcon from "@mui/icons-material/AutoAwesomeRounded";
import { api, type ChatResponse } from "../lib/api";
import { ChatComposer } from "../components/chat/ChatComposer";
import { ChatMessage, ChatPendingRow } from "../components/chat/ChatMessage";
import type { ChatTurn } from "../components/chat/types";

const SUGGESTIONS = [
  "这个项目架构是怎样的？",
  "SQLite 存了哪些表？",
  "LLM / chunk 怎么配置？",
  "atlas serve 能做什么？",
];

export default function SearchPage() {
  const [chat, setChat] = useState<ChatTurn[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [chat, busy]);

  const send = async (text?: string) => {
    const content = (text ?? input).trim();
    if (!content || busy) return;
    setInput("");
    const next: ChatTurn[] = [...chat, { role: "user", content }];
    setChat(next);
    setBusy(true);
    try {
      const messages = next.map((m) => ({ role: m.role, content: m.content }));
      const res = await api<ChatResponse>("/api/kb/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ messages, top_k: 8 }),
      });
      setChat((c) => [
        ...c,
        {
          role: "assistant",
          content: res.answer || res.error || "（空回答）",
          mode: res.mode || "retrieval",
          sources: res.sources || [],
          contexts: res.contexts || [],
        },
      ]);
    } catch (e) {
      setChat((c) => [
        ...c,
        { role: "assistant", content: `请求失败：${String(e)}`, mode: "retrieval" },
      ]);
    } finally {
      setBusy(false);
      inputRef.current?.focus();
    }
  };

  const empty = chat.length === 0;

  return (
    <Box sx={{ maxWidth: 880, mx: "auto" }}>
      <Hero compact={!empty} />

      {!empty && (
        <Stack spacing={2.2} sx={{ mb: 2 }}>
          {chat.map((m, i) => (
            <ChatMessage key={i} turn={m} />
          ))}
          {busy && <ChatPendingRow />}
          <div ref={endRef} />
        </Stack>
      )}

      {empty && (
        <Stack
          direction="row"
          spacing={1.25}
          flexWrap="wrap"
          useFlexGap
          justifyContent="center"
          className="atlas-stagger"
          sx={{ mb: 3 }}
        >
          {SUGGESTIONS.map((s) => (
            <Card
              key={s}
              onClick={() => void send(s)}
              sx={{
                px: 2,
                py: 1.25,
                cursor: "pointer",
                "&:hover": { borderColor: "primary.main" },
              }}
            >
              <Typography variant="body2" fontWeight={600}>
                {s}
              </Typography>
            </Card>
          ))}
        </Stack>
      )}

      <ChatComposer
        value={input}
        busy={busy}
        inputRef={inputRef}
        onChange={setInput}
        onSend={() => void send()}
      />
    </Box>
  );
}

function Hero({ compact }: { compact: boolean }) {
  return (
    <Stack spacing={1.2} alignItems="center" sx={{ mb: compact ? 2.5 : 4, pt: compact ? 0 : 2 }}>
      <Box
        className="atlas-pop"
        sx={{
          width: 56,
          height: 56,
          borderRadius: "18px",
          background: "linear-gradient(145deg, #1A6FB5 0%, #3D8B6E 100%)",
          boxShadow: "0 12px 32px rgba(26,111,181,0.35)",
          display: "grid",
          placeItems: "center",
        }}
      >
        <AutoAwesomeRoundedIcon sx={{ color: "#fff" }} />
      </Box>
      <Typography variant="h4" align="center" sx={{ fontWeight: 700 }}>
        问 Code Atlas
      </Typography>
      <Typography color="text.secondary" align="center" sx={{ maxWidth: 480 }}>
        基于仓内 Wiki / 知识库回答。无模型时自动退回检索结果，不再单独做关键词搜索页。
      </Typography>
    </Stack>
  );
}
