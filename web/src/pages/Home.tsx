import { useCallback, useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardActions,
  CardContent,
  Chip,
  LinearProgress,
  Skeleton,
  Stack,
  Typography,
  alpha,
} from "@mui/material";
import MenuBookIcon from "@mui/icons-material/MenuBook";
import AutoAwesomeIcon from "@mui/icons-material/AutoAwesome";
import FolderOpenOutlinedIcon from "@mui/icons-material/FolderOpenOutlined";
import UpdateOutlinedIcon from "@mui/icons-material/UpdateOutlined";
import SyncIcon from "@mui/icons-material/Sync";
import { Link as RouterLink, useNavigate } from "react-router-dom";
import {
  api,
  triggerProjectUpdate,
  type Project,
  type ProjectsResponse,
} from "../lib/api";
import { useProject } from "../lib/projectContext";
import { readerHref } from "../lib/routes";

const STATUS_LABELS: Record<string, string> = {
  succeeded: "已成功",
  no_op: "无变化",
  failed: "失败",
};

export default function HomePage() {
  const navigate = useNavigate();
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
            <ProjectCard key={p.id} project={p} onOpen={navigate} />
          ))}
        </Box>
      )}
    </Stack>
  );
}

function ProjectCard({
  project: p,
  onOpen,
}: {
  project: Project;
  onOpen: (to: string) => void;
}) {
  const { setProjectId } = useProject();
  const entry = p.entryPath || p.highlights[0]?.path || "";
  const [updating, setUpdating] = useState(false);
  const [updateErr, setUpdateErr] = useState<string | null>(null);
  const [liveStatus, setLiveStatus] = useState<string | null>(null);
  const openEntry = () => {
    setProjectId(p.id);
    if (entry) onOpen(readerHref(entry, p.id));
  };
  const onUpdate = async () => {
    setUpdating(true);
    setUpdateErr(null);
    setLiveStatus("running");
    try {
      await triggerProjectUpdate(p.id, { mode: "update" });
      setProjectId(p.id);
      onOpen(`/runs?project=${encodeURIComponent(p.id)}`);
    } catch (e) {
      const msg = String(e);
      setLiveStatus(null);
      setUpdateErr(
        msg.includes("409") || msg.toLowerCase().includes("lock held")
          ? `项目「${p.name || p.id}」已有更新在进行中`
          : msg
      );
      setUpdating(false);
    }
  };

  return (
    <Card
      className="atlas-fade"
      onClick={openEntry}
      sx={{
        display: "flex",
        flexDirection: "column",
        cursor: entry ? "pointer" : "default",
        transition: "transform 0.18s ease, box-shadow 0.18s ease, border-color 0.18s ease",
        "&:hover": {
          transform: "translateY(-2px)",
          borderColor: "primary.main",
          boxShadow: "0 12px 32px rgba(20,26,34,0.10)",
        },
      }}
    >
      <CardContent sx={{ p: { xs: 2.25, md: 3 }, flexGrow: 1 }}>
        <Stack direction="row" spacing={1.5} alignItems="flex-start">
          <Box
            sx={{
              width: 44,
              height: 44,
              borderRadius: "14px",
              flexShrink: 0,
              background: "linear-gradient(145deg, #1A6FB5 0%, #3D8B6E 100%)",
              display: "grid",
              placeItems: "center",
              boxShadow: "0 8px 20px rgba(26,111,181,0.28)",
            }}
          >
            <MenuBookIcon sx={{ color: "#fff", fontSize: 22 }} />
          </Box>
          <Box sx={{ minWidth: 0, flexGrow: 1 }}>
            <Typography variant="h6" sx={{ fontWeight: 700, letterSpacing: "-0.01em" }} noWrap>
              {p.name || p.id}
            </Typography>
            <Stack direction="row" spacing={0.75} alignItems="center" sx={{ mt: 0.25, minWidth: 0 }}>
              <FolderOpenOutlinedIcon sx={{ fontSize: 14, color: "text.secondary", flexShrink: 0 }} />
              <Typography
                variant="caption"
                color="text.secondary"
                noWrap
                title={p.root}
                sx={{
                  fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
                  fontSize: 11.5,
                  minWidth: 0,
                }}
              >
                {p.root}
              </Typography>
            </Stack>
          </Box>
          <StatusChip status={p.lastStatus} />
        </Stack>

        <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap sx={{ mt: 2 }}>
          <Chip size="small" variant="outlined" label={p.language || "未知语言"} sx={{ borderRadius: 1.5 }} />
          <Chip
            size="small"
            variant="outlined"
            label={`${p.provider || "-"} · ${p.model || "-"}`}
            sx={{ borderRadius: 1.5, maxWidth: 240 }}
          />
          {p.gitHead && (
            <Chip
              size="small"
              variant="outlined"
              label={p.gitHead.slice(0, 8)}
              sx={{
                borderRadius: 1.5,
                fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
                fontSize: 11.5,
              }}
            />
          )}
        </Stack>

        <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap sx={{ mt: 1.5 }}>
          <StatChip label={`${p.pages} 页文档`} />
          <StatChip label={`${p.chunks} 知识块`} />
          <StatChip label={`${p.entities} 实体`} />
          <StatChip label={`${p.runs} 次运行`} />
        </Stack>

        <Stack direction="row" spacing={0.75} alignItems="center" sx={{ mt: 1.75 }}>
          <UpdateOutlinedIcon sx={{ fontSize: 15, color: "text.secondary" }} />
          <Typography variant="caption" color="text.secondary">
            更新于 {formatTime(p.updatedAt)}
            {liveStatus === "running"
              ? " · 更新中…"
              : p.lastStatus
                ? ` · 最近运行：${STATUS_LABELS[p.lastStatus] || p.lastStatus}`
                : ""}
          </Typography>
        </Stack>
        {(updating || liveStatus === "running") && (
          <Box sx={{ mt: 1.5 }}>
            <LinearProgress sx={{ borderRadius: 1, height: 6 }} />
          </Box>
        )}

        {p.highlights.length > 0 && (
          <Box sx={{ mt: 2 }}>
            <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 600 }}>
              章节入口
            </Typography>
            <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap sx={{ mt: 0.75 }}>
              {p.highlights.map((h) => (
                <Chip
                  key={h.path}
                  size="small"
                  clickable
                  component={RouterLink}
                  to={readerHref(h.path, p.id)}
                  label={h.title}
                  title={h.path}
                  onClick={(e) => e.stopPropagation()}
                  sx={{
                    borderRadius: 1.5,
                    bgcolor: alpha("#1A6FB5", 0.08),
                    border: "1px solid",
                    borderColor: alpha("#1A6FB5", 0.25),
                  }}
                />
              ))}
            </Stack>
          </Box>
        )}
      </CardContent>

      {updateErr && (
        <Alert severity="error" sx={{ mx: { xs: 2.25, md: 3 }, mb: 1 }}>
          {updateErr}
        </Alert>
      )}
      <CardActions sx={{ px: { xs: 2.25, md: 3 }, pb: { xs: 2, md: 2.5 }, pt: 0, gap: 1, flexWrap: "wrap" }}>
        <Button
          variant="contained"
          size="small"
          disableElevation
          disabled={!entry}
          startIcon={<MenuBookIcon fontSize="small" />}
          onClick={(e) => {
            e.stopPropagation();
            openEntry();
          }}
        >
          进入文档
        </Button>
        <Button
          variant="outlined"
          size="small"
          startIcon={<SyncIcon fontSize="small" />}
          disabled={updating}
          onClick={(e) => {
            e.stopPropagation();
            void onUpdate();
          }}
        >
          {updating ? "更新中…" : "更新文档"}
        </Button>
        <Button
          variant="outlined"
          size="small"
          startIcon={<AutoAwesomeIcon fontSize="small" />}
          onClick={(e) => {
            e.stopPropagation();
            setProjectId(p.id);
            onOpen(`/chat?project=${encodeURIComponent(p.id)}`);
          }}
        >
          开始对话
        </Button>
      </CardActions>
    </Card>
  );
}

function StatChip({ label }: { label: string }) {
  return (
    <Chip
      size="small"
      label={label}
      sx={{
        height: 22,
        borderRadius: 1.5,
        bgcolor: "#F4F6FA",
        color: "text.secondary",
        fontSize: 11.5,
      }}
    />
  );
}

function StatusChip({ status }: { status?: string | null }) {
  if (!status) return null;
  const tone =
    status === "succeeded"
      ? { color: "#3D8B6E", bg: alpha("#3D8B6E", 0.12) }
      : status === "no_op"
        ? { color: "#5A6678", bg: alpha("#5A6678", 0.12) }
        : { color: "#B3261E", bg: alpha("#B3261E", 0.12) };
  return (
    <Chip
      size="small"
      label={STATUS_LABELS[status] || status}
      sx={{ borderRadius: 1.5, color: tone.color, bgcolor: tone.bg, fontWeight: 600, flexShrink: 0 }}
    />
  );
}

function formatTime(iso?: string | null): string {
  if (!iso) return "未知时间";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}
