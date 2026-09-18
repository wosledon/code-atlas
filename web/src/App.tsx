import { useEffect, useMemo, useState } from "react";
import {
  AppBar,
  Box,
  Container,
  Drawer,
  FormControl,
  InputLabel,
  List,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  MenuItem,
  Select,
  Stack,
  Toolbar,
  Typography,
  Chip,
} from "@mui/material";
import GridViewIcon from "@mui/icons-material/GridView";
import AccountTreeIcon from "@mui/icons-material/AccountTree";
import MenuBookIcon from "@mui/icons-material/MenuBook";
import AutoAwesomeIcon from "@mui/icons-material/AutoAwesome";
import HistoryIcon from "@mui/icons-material/History";
import SettingsIcon from "@mui/icons-material/Settings";
import { NavLink, Route, Routes, useLocation } from "react-router-dom";
import GraphPage from "./pages/Graph";
import HomePage from "./pages/Home";
import RunsPage from "./pages/Runs";
import SearchPage from "./pages/Search";
import ReaderPage from "./pages/Reader";
import SettingsPage from "./pages/Settings";
import { globalMotion } from "./theme/motion.css";
import { api } from "./lib/api";
import { withProject } from "./lib/routes";
import { ProjectProvider, useProject } from "./lib/projectContext";

const drawerWidth = 240;
const nav = [
  { to: "/", label: "项目", icon: <GridViewIcon fontSize="small" /> },
  { to: "/chat", label: "对话", icon: <AutoAwesomeIcon fontSize="small" /> },
  { to: "/reader", label: "项目文档", icon: <MenuBookIcon fontSize="small" /> },
  { to: "/graph", label: "知识图谱", icon: <AccountTreeIcon fontSize="small" /> },
  { to: "/runs", label: "运行记录", icon: <HistoryIcon fontSize="small" /> },
  { to: "/settings", label: "设置", icon: <SettingsIcon fontSize="small" /> },
];

const FULL_HEIGHT_ROUTES = ["/reader", "/graph"];

export default function App() {
  return (
    <ProjectProvider>
      <AppShell />
    </ProjectProvider>
  );
}

function AppShell() {
  const [health, setHealth] = useHealth();
  const { pathname } = useLocation();
  const appShell = FULL_HEIGHT_ROUTES.includes(pathname);
  const styleTag = useMemo(() => globalMotion, []);
  const { projectId, setProjectId, projects } = useProject();

  return (
    <Box sx={{ display: "flex", minHeight: "100vh", bgcolor: "background.default" }}>
      <style>{styleTag}</style>
      <Box
        sx={{
          position: "fixed",
          inset: 0,
          pointerEvents: "none",
          zIndex: 0,
          background:
            "radial-gradient(600px 300px at 10% -10%, rgba(26,111,181,0.10), transparent 60%), radial-gradient(500px 300px at 90% 0%, rgba(61,139,110,0.08), transparent 55%)",
        }}
      />
      <AppBar
        position="fixed"
        color="default"
        elevation={0}
        sx={{ zIndex: 1300, borderBottom: 1, borderColor: "divider" }}
      >
        <Toolbar>
          <Stack direction="row" alignItems="center" spacing={1.5} sx={{ flexGrow: 1 }}>
            <Box
              sx={{
                width: 32,
                height: 32,
                borderRadius: 2,
                background: "linear-gradient(135deg, #1A6FB5, #3D8B6E)",
                boxShadow: "0 4px 12px rgba(26,111,181,0.35)",
              }}
            />
            <Typography variant="h6" sx={{ fontWeight: 700, letterSpacing: "-0.02em" }}>
              Code Atlas
            </Typography>
            <Typography variant="caption" color="text.secondary" sx={{ ml: 1 }}>
              wiki · graph · knowledge base
            </Typography>
          </Stack>
          {health && (
            <Chip
              size="small"
              variant="outlined"
              label={`${health.provider || "-"} · ${health.model || "-"}`}
              sx={{ mr: 1 }}
            />
          )}
        </Toolbar>
      </AppBar>
      <Drawer
        variant="permanent"
        sx={{
          width: drawerWidth,
          flexShrink: 0,
          zIndex: 1200,
          [`& .MuiDrawer-paper`]: {
            width: drawerWidth,
            boxSizing: "border-box",
            top: 64,
            borderRight: "1px solid #E4E9F0",
            bgcolor: "rgba(255,255,255,0.72)",
            backdropFilter: "blur(10px)",
            px: 1.5,
            py: 2,
          },
        }}
      >
        {projects.length > 0 && (
          <FormControl size="small" sx={{ mb: 1.5, mx: 0.5 }}>
            <InputLabel id="global-project-label">当前项目</InputLabel>
            <Select
              labelId="global-project-label"
              label="当前项目"
              value={projectId ?? ""}
              onChange={(e) => setProjectId(e.target.value || null)}
            >
              {projects.map((p) => (
                <MenuItem key={p.id} value={p.id}>
                  {p.name || p.id}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        )}
        <List sx={{ gap: 0.5 }}>
          {nav.map((n) => (
            <ListItemButton
              key={n.to}
              component={NavLink}
              to={n.to}
              end={n.to === "/"}
              sx={{
                borderRadius: 2.5,
                mb: 0.5,
                transition: "all 0.18s ease",
                "&.active": {
                  bgcolor: "primary.main",
                  color: "primary.contrastText",
                  boxShadow: "0 6px 16px rgba(26,111,181,0.28)",
                  "& .MuiListItemIcon-root": { color: "inherit" },
                  "&:hover": { bgcolor: "primary.dark" },
                },
                "&:hover": { bgcolor: "action.hover" },
              }}
            >
              <ListItemIcon sx={{ minWidth: 36, color: "text.secondary" }}>{n.icon}</ListItemIcon>
              <ListItemText primary={n.label} primaryTypographyProps={{ fontWeight: 600 }} />
            </ListItemButton>
          ))}
        </List>
      </Drawer>
      <Box
        component="main"
        sx={{
          flexGrow: 1,
          minWidth: 0,
          pt: appShell ? 8 : 11,
          px: appShell ? 0 : { xs: 2, md: 4 },
          pb: appShell ? 0 : 6,
          position: "relative",
          zIndex: 1,
          ...(appShell
            ? {
                height: { xs: "auto", md: "100vh" },
                overflow: { xs: "visible", md: "hidden" },
                display: "flex",
                flexDirection: "column",
              }
            : {}),
        }}
      >
        {appShell ? (
          <div
            className="atlas-fade"
            style={{
              height: "100%",
              display: "flex",
              flexDirection: "column",
              minHeight: 0,
              flexGrow: 1,
              width: "100%",
            }}
          >
            <AppRoutes />
          </div>
        ) : (
          <Container maxWidth="lg">
            <div className="atlas-fade">
              <AppRoutes />
            </div>
          </Container>
        )}
      </Box>
    </Box>
  );
}

function useHealth() {
  const { projectId } = useProject();
  const [health, setHealth] = useState<{ model?: string; provider?: string } | null>(null);
  useEffect(() => {
    const url = new URLSearchParams(window.location.search).get("t");
    if (url) localStorage.setItem("atlas_token", url);
    api<{ model: string; provider: string; project?: string }>(
      withProject("/api/health", projectId)
    )
      .then(setHealth)
      .catch(() => setHealth(null));
  }, [projectId]);
  return [health, setHealth] as const;
}

function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<HomePage />} />
      <Route path="/chat" element={<SearchPage />} />
      <Route path="/graph" element={<GraphPage />} />
      <Route path="/reader" element={<ReaderPage />} />
      <Route path="/runs" element={<RunsPage />} />
      <Route path="/settings" element={<SettingsPage />} />
      <Route path="*" element={<HomePage />} />
    </Routes>
  );
}
