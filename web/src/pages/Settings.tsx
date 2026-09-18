import { useCallback, useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  Divider,
  IconButton,
  MenuItem,
  Stack,
  TextField,
  Typography,
} from "@mui/material";
import SaveIcon from "@mui/icons-material/Save";
import DeleteOutlineIcon from "@mui/icons-material/DeleteOutline";
import AddIcon from "@mui/icons-material/Add";
import {
  api,
  registerProject,
  unregisterProject,
  type Project,
  type ProjectsResponse,
} from "../lib/api";
import { useProject } from "../lib/projectContext";
import { withProject } from "../lib/routes";

type Config = {
  project?: string;
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
  const { projectId } = useProject();
  const [cfg, setCfg] = useState<Config | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [newRoot, setNewRoot] = useState("");
  const [newName, setNewName] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api<Config>(withProject("/api/config", projectId))
      .then(setCfg)
      .catch((e) => setErr(String(e)));

  const loadProjects = useCallback(() => {
    api<ProjectsResponse>("/api/projects")
      .then((r) => setProjects(r.projects || []))
      .catch((e) => setErr(String(e)));
  }, []);

  useEffect(() => {
    load();
    loadProjects();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, loadProjects]);

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
          project: projectId || undefined,
        }),
      });
      setMsg(`已写入项目「${projectId || "default"}」的 atlas.toml（不含密钥）。`);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onRegister = async () => {
    const root = newRoot.trim();
    if (!root) return;
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const p = await registerProject({
        root,
        name: newName.trim() || undefined,
      });
      setMsg(`已登记项目 ${p.id}`);
      setNewRoot("");
      setNewName("");
      loadProjects();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onUnregister = async (id: string) => {
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      await unregisterProject(id);
      setMsg(`已移除项目 ${id}`);
      loadProjects();
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
          服务入口 <code>atlas web</code> · 文档更新按项目维度 · 密钥可写 <code>atlas.toml</code>{" "}
          或环境变量（环境优先）
        </Typography>
      </Box>
      {err && <Alert severity="error">{err}</Alert>}
      {msg && <Alert severity="success">{msg}</Alert>}

      <Card className="atlas-fade">
        <CardContent sx={{ p: 3 }}>
          <Typography variant="h6">项目注册表</Typography>
          <Typography color="text.secondary" sx={{ mt: 0.5, mb: 2 }}>
            当前启动仓库始终在列表中；其他仓库写入 <code>atlas.projects.json</code>（与 <code>atlas</code> 可执行文件同目录）。
            项目卡片与 MCP <code>atlas_update</code> 都按项目 id 更新文档。
          </Typography>
          <Stack spacing={2}>
            {projects.map((p) => (
              <Stack
                key={p.id}
                direction={{ xs: "column", sm: "row" }}
                spacing={1}
                alignItems={{ sm: "center" }}
                justifyContent="space-between"
                sx={{
                  border: "1px solid",
                  borderColor: "divider",
                  borderRadius: 2,
                  px: 2,
                  py: 1.25,
                }}
              >
                <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
                  <Typography variant="subtitle2">{p.name || p.id}</Typography>
                  <Chip size="small" label={p.id} variant="outlined" />
                  {p.isLaunch && <Chip size="small" color="primary" label="启动项目" />}
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ fontFamily: '"IBM Plex Mono", monospace' }}
                  >
                    {p.root}
                  </Typography>
                </Stack>
                <IconButton
                  size="small"
                  color="warning"
                  disabled={busy || !!p.isLaunch}
                  title={p.isLaunch ? "启动项目不可移除" : "从注册表移除"}
                  onClick={() => void onUnregister(p.id)}
                >
                  <DeleteOutlineIcon fontSize="small" />
                </IconButton>
              </Stack>
            ))}
            {projects.length === 0 && (
              <Typography color="text.secondary">暂无项目（启动仓库加载后会自动出现）</Typography>
            )}
            <Divider />
            <Stack direction={{ xs: "column", sm: "row" }} spacing={1.5}>
              <TextField
                label="仓库绝对路径"
                value={newRoot}
                onChange={(e) => setNewRoot(e.target.value)}
                placeholder="E:/repos/other-project"
                fullWidth
                size="small"
              />
              <TextField
                label="显示名（可选）"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                sx={{ minWidth: { sm: 180 } }}
                size="small"
              />
              <Button
                variant="outlined"
                startIcon={<AddIcon />}
                disabled={busy || !newRoot.trim()}
                onClick={() => void onRegister()}
              >
                登记项目
              </Button>
            </Stack>
          </Stack>
        </CardContent>
      </Card>

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
