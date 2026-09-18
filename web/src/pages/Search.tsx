import { useEffect, useRef, useState } from "react";
import { Box, Stack, Typography, alpha } from "@mui/material";
import AutoAwesomeRoundedIcon from "@mui/icons-material/AutoAwesomeRounded";
import { streamApi } from "../lib/api";
import { useProject } from "../lib/projectContext";
import { ChatComposer } from "../components/chat/ChatComposer";
import { ChatMessage } from "../components/chat/ChatMessage";
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
  const { projectId } = useProject();
  const endRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [chat, busy]);

  const send = async (text?: string) => {
    const content = (text ?? input).trim();
    if (!content || busy) return;
    setInput("");
    setBusy(true);

    const uid = `u-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const aid = `${uid}-a`;
    const userTurn: ChatTurn = { id: uid, role: "user", content };
    const asstTurn: ChatTurn = {
      id: aid,
      role: "assistant",
      content: "",
      mode: "llm",
      sources: [],
      contexts: [],
    };

    // Snapshot the conversation *including* the new user turn for the API body.
    const history: ChatTurn[] = [...chat, userTurn];
    setChat([...history, asstTurn]);

    const patchAssistant = (patch: Partial<ChatTurn>) => {
      setChat((c) => c.map((t) => (t.id === aid ? { ...t, ...patch } : t)));
    };

    try {
      const messages = history.map((m) => ({ role: m.role, content: m.content }));
      let acc = "";
      let sawDone = false;
      await streamApi(
        "/api/kb/chat",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ messages, top_k: 8, project: projectId || undefined }),
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
            sawDone = true;
            patchAssistant({
              content: ev.answer || acc || "（空回答）",
              mode: ev.mode || "llm",
              sources: ev.sources || [],
              contexts: ev.contexts || [],
              error: ev.error,
            });
          }
        }
      );
      if (!sawDone) {
        patchAssistant({
          content: acc || "连接已结束，但没有收到回答。请检查模型配置后重试。",
          mode: "retrieval",
          error: "连接中断：未收到流式完成事件",
        });
      }
    } catch (e) {
      patchAssistant({
        content: `请求失败：${String(e)}`,
        mode: "retrieval",
        error: String(e),
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
