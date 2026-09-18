import { useCallback, useEffect, useRef, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  LinearProgress,
  Stack,
  Typography,
} from "@mui/material";
import RefreshIcon from "@mui/icons-material/Refresh";
import { listRuns, triggerProjectUpdate, type Run } from "../lib/api";
import { useProject } from "../lib/projectContext";

const statusColor = (s: string) =>
  s === "succeeded" || s === "no_op" ? "success" : s === "failed" ? "error" : "default";

export default function RunsPage() {
  const { projectId, projects } = useProject();
  const [runs, setRuns] = useState<Run[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [polling, setPolling] = useState(false);
  const pollRef = useRef<number | null>(null);

  const load = useCallback((id: string | null) => {
    listRuns(id || undefined)
      .then((r) => setRuns(r.runs || []))
      .catch((e) => setErr(String(e)));
  }, []);

  useEffect(() => {
    load(projectId);
  }, [load, projectId]);

  useEffect(() => () => {
    if (pollRef.current) window.clearInterval(pollRef.current);
  }, []);

  const startPolling = useCallback(
    (id: string) => {
      if (pollRef.current) window.clearInterval(pollRef.current);
      setPolling(true);
      let ticks = 0;
      pollRef.current = window.setInterval(() => {
        ticks += 1;
        listRuns(id)
          .then((r) => {
            setRuns(r.runs || []);
            const running = (r.runs || []).some((x) => x.status === "running");
            if (!running || ticks > 60) {
              if (pollRef.current) window.clearInterval(pollRef.current);
              pollRef.current = null;
              setPolling(false);
            }
          })
          .catch(() => {
            if (pollRef.current) window.clearInterval(pollRef.current);
            pollRef.current = null;
            setPolling(false);
          });
      }, 3000);
    },
    []
  );

  const projectLabel =
    projects.find((p) => p.id === projectId)?.name || projectId || "当前项目";

  return (
    <Stack spacing={2.5}>
      <Stack
        direction={{ xs: "column", sm: "row" }}
        justifyContent="space-between"
        alignItems={{ sm: "center" }}
        spacing={2}
      >
        <Box className="atlas-fade">
          <Typography variant="h4">运行记录</Typography>
          <Typography color="text.secondary" sx={{ mt: 0.5 }}>
            项目「{projectLabel}」· init / update · token 用量 · no-op 门禁
          </Typography>
        </Box>
        <Button
          variant="contained"
          startIcon={<RefreshIcon />}
          disabled={busy || !projectId}
          onClick={async () => {
            if (!projectId) return;
            setBusy(true);
            setErr(null);
            try {
              await triggerProjectUpdate(projectId, { mode: "update" });
              startPolling(projectId);
              load(projectId);
            } catch (e) {
              const msg = String(e);
              setErr(
                msg.includes("409") || msg.toLowerCase().includes("lock held")
                  ? `项目「${projectLabel}」已有更新在进行中，请稍后再试。`
                  : msg
              );
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? "提交中…" : "触发当前项目 update"}
        </Button>
      </Stack>
      {(busy || polling) && <LinearProgress sx={{ borderRadius: 1, height: 6 }} />}
      {err && <Alert severity="error">{err}</Alert>}
      <Box className="atlas-stagger" sx={{ display: "grid", gap: 2 }}>
        {runs.map((r) => (
          <Card key={r.id}>
            <CardContent sx={{ py: 2.2 }}>
              <Stack
                direction={{ xs: "column", sm: "row" }}
                justifyContent="space-between"
                spacing={1.5}
                alignItems={{ sm: "center" }}
              >
                <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
                  <Chip size="small" color={statusColor(r.status)} label={r.status} />
                  <Chip size="small" variant="outlined" label={r.mode} />
                  <Typography variant="subtitle2" fontFamily="IBM Plex Mono, monospace">
                    {r.id.slice(0, 8)}
                  </Typography>
                </Stack>
                <Stack direction="row" spacing={2} alignItems="center">
                  <Typography variant="caption" color="text.secondary">
                    {r.provider || "-"} / {r.model || "-"}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    tokens {r.prompt_tokens}+{r.completion_tokens}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    {new Date(r.created_at).toLocaleString()}
                  </Typography>
                </Stack>
              </Stack>
            </CardContent>
          </Card>
        ))}
        {runs.length === 0 && (
          <Card>
            <CardContent>
              <Typography color="text.secondary">
                {projectId ? "该项目暂无运行记录" : "请选择项目后查看运行记录"}
              </Typography>
            </CardContent>
          </Card>
        )}
      </Box>
    </Stack>
  );
}
