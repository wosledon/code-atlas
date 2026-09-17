import { useEffect, useRef, useState } from "react";
import { Box, Stack, Typography, alpha } from "@mui/material";
import AutoAwesomeRoundedIcon from "@mui/icons-material/AutoAwesomeRounded";
import { streamApi } from "../lib/api";
import { ChatComposer } from "../components/chat/ChatComposer";
import { ChatMessage, ChatPendingRow } from "../components/chat/ChatMessage";
import type { ChatTurn } from "../components/chat/types";

const SUGGESTIONS = [
  "这个项目架构是怎样的？",
  "SQLite 存了哪些表？",
  "LLM / chunk 怎么配置？",
  "atlas web 能做什么？",
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
    const history: ChatTurn[] = [...chat, { role: "user", content }];
    setChat(history);
    setBusy(true);

    // Placeholder assistant turn that fills as SSE deltas arrive.
    const assistantIdx = history.length;
    setChat((c) => [
      ...c,
      { role: "assistant", content: "", mode: "llm", sources: [], contexts: [] },
    ]);

    const patchAssistant = (patch: Partial<ChatTurn>) => {
      setChat((c) => {
        const next = [...c];
        const cur = next[assistantIdx];
        if (!cur) return c;
        next[assistantIdx] = { ...cur, ...patch };
        return next;
      });
    };

    try {
      const messages = history.map((m) => ({ role: m.role, content: m.content }));
      let acc = "";
      await streamApi(
        "/api/kb/chat",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ messages, top_k: 8 }),
        },
        (ev) => {
          if (ev.type === "meta") {
            patchAssistant({
              mode: ev.mode || "llm",
              sources: ev.sources || [],
              contexts: ev.contexts || [],
            });
          } else if (ev.type === "delta") {
            acc += ev.text;
            patchAssistant({ content: acc });
          } else if (ev.type === "done") {
            patchAssistant({
              content: ev.answer || acc || "（空回答）",
              mode: ev.mode || "llm",
              sources: ev.sources || [],
              contexts: ev.contexts || [],
            });
          }
        }
      );
      // If the stream ended without a done frame, keep whatever arrived.
      if (acc) patchAssistant({ content: acc });
    } catch (e) {
      patchAssistant({
        content: `请求失败：${String(e)}`,
        mode: "retrieval",
      });
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
          {busy && chat[chat.length - 1]?.role === "user" && <ChatPendingRow />}
          {busy && chat[chat.length - 1]?.role === "assistant" && !chat[chat.length - 1]?.content && (
            <ChatPendingRow />
          )}
          <div ref={endRef} />
        </Stack>
      )}

      {empty && (
        <Stack
          direction="row"
          spacing={1}
          flexWrap="wrap"
          useFlexGap
          justifyContent="center"
          className="atlas-stagger"
          sx={{ mb: 3 }}
        >
          {SUGGESTIONS.map((s) => (
            <Box
              key={s}
              onClick={() => void send(s)}
              sx={{
                px: 1.75,
                py: 1,
                borderRadius: 1.5,
                border: "1px solid #E4E9F0",
                bgcolor: "#fff",
                cursor: "pointer",
                transition: "border-color 0.15s ease, box-shadow 0.15s ease",
                "&:hover": {
                  borderColor: alpha("#1A6FB5", 0.45),
                  boxShadow: "0 4px 14px rgba(26,111,181,0.1)",
                },
              }}
            >
              <Typography variant="body2" fontWeight={600}>
                {s}
              </Typography>
            </Box>
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
        基于仓内 Wiki / 知识库流式回答。无模型时自动退回检索结果。
      </Typography>
    </Stack>
  );
}
