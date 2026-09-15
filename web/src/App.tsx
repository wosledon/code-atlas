import { useEffect, useMemo, useState } from "react";
import {
  AppBar,
  Box,
  Container,
  Drawer,
  List,
  ListItemButton,
  ListItemIcon,
  ListItemText,
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
import { NavLink, Route, Routes } from "react-router-dom";
import GraphPage from "./pages/Graph";
import HomePage from "./pages/Home";
import RunsPage from "./pages/Runs";
import SearchPage from "./pages/Search";
import ReaderPage from "./pages/Reader";
import SettingsPage from "./pages/Settings";
import { globalMotion } from "./theme/motion.css";
import { api } from "./lib/api";

const drawerWidth = 240;
const nav = [
  { to: "/", label: "项目", icon: <GridViewIcon fontSize="small" /> },
  { to: "/chat", label: "对话", icon: <AutoAwesomeIcon fontSize="small" /> },
  { to: "/reader", label: "项目文档", icon: <MenuBookIcon fontSize="small" /> },
  { to: "/graph", label: "知识图谱", icon: <AccountTreeIcon fontSize="small" /> },
  { to: "/runs", label: "运行记录", icon: <HistoryIcon fontSize="small" /> },
  { to: "/settings", label: "设置", icon: <SettingsIcon fontSize="small" /> },
];

export default function App() {
  const [health, setHealth] = useState<{ model?: string; provider?: string } | null>(null);

  useEffect(() => {
    const url = new URLSearchParams(window.location.search).get("t");
    if (url) localStorage.setItem("atlas_token", url);
    api<{ model: string; provider: string }>("/api/health")
      .then(setHealth)
      .catch(() => setHealth(null));
  }, []);

  const styleTag = useMemo(() => globalMotion, []);

  return (
    <Box sx={{ display: "flex", minHeight: "100vh", bgcolor: "background.default" }}>
      <style>{styleTag}</style>
      {/* soft ambient blobs */}
      <Box
        sx={{
          position: "fixed",
          inset: 0,
          pointerEvents: "none",
          zIndex: 0,
          background:
            "radial-gradient(600px 300px at 10% -10%, rgba(26,111,181,0.10), transparent 60%), radial-gradient(500px 280px at 90% 0%, rgba(61,139,110,0.08), transparent 55%)",
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
      <Box component="main" sx={{ flexGrow: 1, pt: 11, px: { xs: 2, md: 4 }, pb: 6, position: "relative", zIndex: 1 }}>
        <Container maxWidth="lg">
          <div className="atlas-fade">
            <Routes>
              <Route path="/" element={<HomePage />} />
              <Route path="/chat" element={<SearchPage />} />
              <Route path="/graph" element={<GraphPage />} />
              <Route path="/reader" element={<ReaderPage />} />
              <Route path="/runs" element={<RunsPage />} />
              <Route path="/settings" element={<SettingsPage />} />
              <Route path="*" element={<HomePage />} />
            </Routes>
          </div>
        </Container>
      </Box>
    </Box>
  );
}
