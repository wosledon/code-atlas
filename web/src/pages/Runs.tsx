import { useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  Stack,
  Typography,
  LinearProgress,
} from "@mui/material";
import RefreshIcon from "@mui/icons-material/Refresh";
import { api, Run } from "../lib/api";

const statusColor = (s: string) =>
  s === "succeeded" || s === "no_op" ? "success" : s === "failed" ? "error" : "default";

export default function RunsPage() {
  const [runs, setRuns] = useState<Run[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api<Run[]>("/api/runs")
      .then(setRuns)
      .catch((e) => setErr(String(e)));

  useEffect(() => {
    load();
  }, []);

  return (
    <Stack spacing={2.5}>
      <Stack direction={{ xs: "column", sm: "row" }} justifyContent="space-between" alignItems={{ sm: "center" }} spacing={2}>
        <Box className="atlas-fade">
          <Typography variant="h4">运行记录</Typography>
          <Typography color="text.secondary" sx={{ mt: 0.5 }}>
            init / update · token 用量 · no-op 门禁
          </Typography>
        </Box>
        <Button
          variant="contained"
          startIcon={<RefreshIcon />}
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              await api("/api/run/update", { method: "POST" });
              await load();
            } catch (e) {
              setErr(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? "更新中…" : "触发 update"}
        </Button>
      </Stack>
      {busy && <LinearProgress sx={{ borderRadius: 1, height: 6 }} />}
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
              <Typography color="text.secondary">暂无运行记录</Typography>
            </CardContent>
          </Card>
        )}
      </Box>
    </Stack>
  );
}
