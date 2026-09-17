import { useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Divider,
  MenuItem,
  Stack,
  TextField,
  Typography,
} from "@mui/material";
import SaveIcon from "@mui/icons-material/Save";
import { api } from "../lib/api";

type Config = {
  provider: string;
  model: string;
  base_url: string;
  temperature: number;
  language: string;
  strategy: string;
  atlas_root: string;
  chunk_mode: string;
  has_openai_key: boolean;
  has_anthropic_key: boolean;
};

export default function SettingsPage() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api<Config>("/api/config")
      .then(setCfg)
      .catch((e) => setErr(String(e)));

  useEffect(() => {
    load();
  }, []);

  const save = async () => {
    if (!cfg) return;
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      await api("/api/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: cfg.provider,
          model: cfg.model,
          base_url: cfg.base_url,
          temperature: cfg.temperature,
          language: cfg.language,
          chunk_mode: cfg.chunk_mode,
          strategy: cfg.strategy,
        }),
      });
      setMsg("已写入 atlas.toml（不含密钥）。重启 atlas web 后新会话生效。");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Stack spacing={2.5} maxWidth={820}>
      <Box className="atlas-fade">
        <Typography variant="h4">设置</Typography>
        <Typography color="text.secondary" sx={{ mt: 0.5 }}>
          服务入口 <code>atlas web</code> · 密钥可写 <code>atlas.toml</code> 或环境变量（环境优先）
        </Typography>
      </Box>
      {err && <Alert severity="error">{err}</Alert>}
      {msg && <Alert severity="success">{msg}</Alert>}
      {!cfg && !err && <Typography color="text.secondary">加载中…</Typography>}
      {cfg && (
        <Card className="atlas-fade">
          <CardContent sx={{ p: 3 }}>
            <Typography variant="h6" sx={{ mb: 2 }}>
              模型与生成
            </Typography>
            <Stack spacing={2}>
              <Stack direction={{ xs: "column", sm: "row" }} spacing={2}>
                <TextField
                  select
                  label="Provider"
                  value={cfg.provider}
                  onChange={(e) => setCfg({ ...cfg, provider: e.target.value })}
                  fullWidth
                >
                  <MenuItem value="openai-compatible">openai-compatible</MenuItem>
                  <MenuItem value="openai">openai</MenuItem>
                  <MenuItem value="anthropic">anthropic</MenuItem>
                  <MenuItem value="host-agent">host-agent</MenuItem>
                </TextField>
                <TextField
                  label="Model"
                  value={cfg.model}
                  onChange={(e) => setCfg({ ...cfg, model: e.target.value })}
                  fullWidth
                />
              </Stack>
              <TextField
                label="Base URL"
                value={cfg.base_url}
                onChange={(e) => setCfg({ ...cfg, base_url: e.target.value })}
                placeholder="http://127.0.0.1:11434/v1"
                fullWidth
              />
              <Stack direction={{ xs: "column", sm: "row" }} spacing={2}>
                <TextField
                  label="Temperature"
                  type="number"
                  value={cfg.temperature}
                  onChange={(e) => setCfg({ ...cfg, temperature: Number(e.target.value) })}
                  fullWidth
                />
                <TextField
                  select
                  label="输出语言"
                  value={cfg.language}
                  onChange={(e) => setCfg({ ...cfg, language: e.target.value })}
                  fullWidth
                >
                  <MenuItem value="zh-CN">zh-CN</MenuItem>
                  <MenuItem value="en">en</MenuItem>
                </TextField>
                <TextField
                  select
                  label="Chunk mode"
                  value={cfg.chunk_mode}
                  onChange={(e) => setCfg({ ...cfg, chunk_mode: e.target.value })}
                  fullWidth
                >
                  <MenuItem value="hybrid">hybrid</MenuItem>
                  <MenuItem value="structural">structural</MenuItem>
                  <MenuItem value="llm-semantic">llm-semantic</MenuItem>
                </TextField>
              </Stack>
              <TextField
                select
                label="输出位置"
                value={cfg.strategy}
                onChange={(e) => setCfg({ ...cfg, strategy: e.target.value })}
                fullWidth
              >
                <MenuItem value="in-repo">in-repo</MenuItem>
                <MenuItem value="external-dir">external-dir</MenuItem>
                <MenuItem value="db-only">db-only</MenuItem>
              </TextField>
              <Divider />
              <Typography variant="body2" color="text.secondary">
                atlas_root：<code>{cfg.atlas_root}</code>
                <br />
                OPENAI_API_KEY：{cfg.has_openai_key ? "已设置" : "未设置"} · ANTHROPIC_API_KEY：
                {cfg.has_anthropic_key ? "已设置" : "未设置"}
              </Typography>
              <Button variant="contained" startIcon={<SaveIcon />} disabled={busy} onClick={save}>
                {busy ? "保存中…" : "保存到 atlas.toml"}
              </Button>
            </Stack>
          </CardContent>
        </Card>
      )}
    </Stack>
  );
}
