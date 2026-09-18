import { useCallback, useEffect, useState } from "react";
import { Alert, Box, Stack, Typography } from "@mui/material";
import { api, type Project, type ProjectsResponse } from "../lib/api";
import { useProject } from "../lib/projectContext";
import { withProject } from "../lib/routes";
import {
  ModelConfigCard,
  ProjectRegistryCard,
  type ModelConfig,
} from "../components/settings/ProjectRegistryCard";

export default function SettingsPage() {
  const { projectId } = useProject();
  const [cfg, setCfg] = useState<ModelConfig | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api<ModelConfig>(withProject("/api/config", projectId))
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

  return (
    <Stack spacing={2.5} maxWidth={820}>
      <Box className="atlas-fade">
        <Typography variant="h4">设置</Typography>
        <Typography color="text.secondary" sx={{ mt: 0.5 }}>
          服务入口 <code>atlas web</code> · 文档更新按项目维度 · 密钥可写{" "}
          <code>atlas.toml</code> 或环境变量（环境优先）
        </Typography>
      </Box>
      {err && <Alert severity="error">{err}</Alert>}
      {msg && <Alert severity="success">{msg}</Alert>}

      <ProjectRegistryCard
        projects={projects}
        onChanged={loadProjects}
        onErr={setErr}
        onMsg={setMsg}
      />

      {!cfg && !err && <Typography color="text.secondary">加载中…</Typography>}
      {cfg && (
        <ModelConfigCard cfg={cfg} setCfg={setCfg} busy={busy} onSave={() => void save()} />
      )}
    </Stack>
  );
}
