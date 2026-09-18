import { useState } from "react";
import {
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
import DeleteOutlineIcon from "@mui/icons-material/DeleteOutline";
import AddIcon from "@mui/icons-material/Add";
import {
  registerProject,
  unregisterProject,
  type Project,
} from "../../lib/api";

export function ProjectRegistryCard({
  projects,
  onChanged,
  onErr,
  onMsg,
}: {
  projects: Project[];
  onChanged: () => void;
  onErr: (m: string | null) => void;
  onMsg: (m: string | null) => void;
}) {
  const [newRoot, setNewRoot] = useState("");
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);

  const onRegister = async () => {
    const root = newRoot.trim();
    if (!root) return;
    setBusy(true);
    onErr(null);
    onMsg(null);
    try {
      const p = await registerProject({
        root,
        name: newName.trim() || undefined,
      });
      onMsg(`已登记项目 ${p.id}`);
      setNewRoot("");
      setNewName("");
      onChanged();
    } catch (e) {
      onErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onUnregister = async (id: string) => {
    setBusy(true);
    onErr(null);
    onMsg(null);
    try {
      await unregisterProject(id);
      onMsg(`已移除项目 ${id}`);
      onChanged();
    } catch (e) {
      onErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="atlas-fade">
      <CardContent sx={{ p: 3 }}>
        <Typography variant="h6">项目注册表</Typography>
        <Typography color="text.secondary" sx={{ mt: 0.5, mb: 2 }}>
          当前启动仓库始终在列表中；其他仓库写入{" "}
          <code>atlas.projects.json</code>（与 <code>atlas</code> 可执行文件同目录）。
          项目卡片、HTTP、MCP <code>atlas_update</code> 都按项目 id 更新文档；
          MCP 工具省略 <code>project</code> 时只作用于启动项目。
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
              <Stack
                direction="row"
                spacing={1}
                alignItems="center"
                flexWrap="wrap"
                useFlexGap
              >
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
            <Typography color="text.secondary">
              暂无项目（启动仓库加载后会自动出现）
            </Typography>
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
              sx={{ flexShrink: 0, whiteSpace: "nowrap" }}
            >
              登记项目
            </Button>
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  );
}

export type ModelConfig = {
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

export function ModelConfigCard({
  cfg,
  setCfg,
  busy,
  onSave,
}: {
  cfg: ModelConfig;
  setCfg: (c: ModelConfig) => void;
  busy: boolean;
  onSave: () => void;
}) {
  return (
    <Card className="atlas-fade">
      <CardContent sx={{ p: 3 }}>
        <Stack direction="row" spacing={1} alignItems="center" sx={{ mb: 2 }}>
          <Typography variant="h6">模型与生成</Typography>
          {cfg.project && (
            <Chip size="small" variant="outlined" label={`项目 ${cfg.project}`} />
          )}
        </Stack>
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
          <Box>
            <Button variant="contained" disabled={busy} onClick={onSave}>
              {busy ? "保存中…" : "保存到 atlas.toml"}
            </Button>
          </Box>
        </Stack>
      </CardContent>
    </Card>
  );
}
