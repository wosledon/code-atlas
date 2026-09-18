import { useCallback, useEffect, useState } from "react";
import { Alert, Box, Card, CardContent, LinearProgress, Skeleton, Stack, Typography } from "@mui/material";
import { useNavigate } from "react-router-dom";
import { api, type Project, type ProjectsResponse } from "../lib/api";
import { useProject } from "../lib/projectContext";
import { ProjectCard } from "../components/projects/ProjectCard";

export default function HomePage() {
  const navigate = useNavigate();
  const { refresh } = useProject();
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(() => {
    setErr(null);
    api<ProjectsResponse>("/api/projects")
      .then((r) => setProjects(r.projects || []))
      .catch((e) => {
        setProjects([]);
        setErr(String(e));
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <Stack spacing={3}>
      <Box className="atlas-fade">
        <Typography variant="h4" sx={{ fontWeight: 700 }}>
          项目
        </Typography>
        <Typography color="text.secondary" sx={{ mt: 0.5 }}>
          每张卡片是一份已生成的 Wiki：点击卡片进入文档阅读，也可以直接在对话里追问。
        </Typography>
      </Box>

      {err && <Alert severity="error">读取项目列表失败：{err}</Alert>}

      {projects === null && (
        <Box className="atlas-fade">
          <LinearProgress sx={{ borderRadius: 1, mb: 2 }} />
          <Stack spacing={2}>
            <Skeleton variant="rounded" height={200} />
            <Skeleton variant="rounded" height={200} />
          </Stack>
        </Box>
      )}

      {projects !== null && projects.length === 0 && (
        <Card
          className="atlas-fade"
          sx={{
            p: { xs: 2.5, md: 4 },
            border: "1px dashed",
            borderColor: "divider",
            bgcolor: "transparent",
            boxShadow: "none",
          }}
        >
          <Stack spacing={1.5} alignItems="flex-start">
            <Typography variant="h6" sx={{ fontWeight: 700 }}>
              还没有任何项目 Wiki
            </Typography>
            <Typography color="text.secondary">
              在仓库根目录运行 <code>atlas init</code> 生成 Wiki（已有 Wiki 时可用{" "}
              <code>atlas update</code> 增量更新），刷新本页即可看到项目卡片。
            </Typography>
            <Box
              sx={{
                fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
                fontSize: 13,
                bgcolor: "#F4F6FA",
                px: 1.5,
                py: 1,
                borderRadius: 1.5,
              }}
            >
              atlas init && atlas web
            </Box>
          </Stack>
        </Card>
      )}

      {projects !== null && projects.length > 0 && (
        <Box
          className="atlas-stagger"
          sx={{ display: "grid", gap: 2, gridTemplateColumns: { xs: "1fr", lg: "1fr 1fr" } }}
        >
          {projects.map((p) => (
            <ProjectCard
              key={p.id}
              project={p}
              onOpen={navigate}
              onUpdated={() => {
                void refresh();
                load();
              }}
            />
          ))}
        </Box>
      )}
    </Stack>
  );
}
