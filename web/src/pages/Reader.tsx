import { useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Box,
  Chip,
  Collapse,
  Fab,
  IconButton,
  List,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  Snackbar,
  Stack,
  Tooltip,
  Typography,
  alpha,
} from "@mui/material";
import FolderOutlinedIcon from "@mui/icons-material/FolderOutlined";
import DescriptionOutlinedIcon from "@mui/icons-material/DescriptionOutlined";
import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import ChevronRightIcon from "@mui/icons-material/ChevronRight";
import UnfoldLessIcon from "@mui/icons-material/UnfoldLess";
import UnfoldMoreIcon from "@mui/icons-material/UnfoldMore";
import LinkOutlinedIcon from "@mui/icons-material/LinkOutlined";
import KeyboardArrowUpIcon from "@mui/icons-material/KeyboardArrowUp";
import SubjectIcon from "@mui/icons-material/Subject";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { PageResponse, TreeResponse } from "../lib/api";
import { useProject } from "../lib/projectContext";
import { withProject } from "../lib/routes";
import { MarkdownView, extractHeadings } from "../components/MarkdownView";

type TreeNode = {
  id: string;
  name: string;
  path?: string | null;
  children: TreeNode[];
};

export default function ReaderPage() {
  const [params, setParams] = useSearchParams();
  const { projectId } = useProject();
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [content, setContent] = useState("");
  const [current, setCurrent] = useState<string | null>(params.get("p"));
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [showTop, setShowTop] = useState(false);
  const [activeHeading, setActiveHeading] = useState<string | null>(null);
  const currentRef = useRef<string | null>(params.get("p"));
  const treeRef = useRef<TreeNode | null>(null);
  const docRef = useRef<HTMLDivElement | null>(null);
  const projectRef = useRef<string | null>(projectId);
  projectRef.current = projectId;

  const folderIds = useMemo(() => collectFolderIds(tree), [tree]);
  const allOpen = folderIds.length > 0 && folderIds.every((id) => expanded[id]);
  const headings = useMemo(() => extractHeadings(content), [content]);

  const loadPage = async (p: string, syncUrl = true) => {
    setCurrent(p);
    currentRef.current = p;
    if (syncUrl) {
      const next = new URLSearchParams();
      next.set("p", p);
      const proj = projectRef.current;
      if (proj) next.set("project", proj);
      setParams(next, { replace: true });
    }
    setExpanded((e) => ({ ...e, ...ancestorsOf(treeRef.current, p) }));
    setActiveHeading(null);
    if (docRef.current) docRef.current.scrollTop = 0;
    try {
      const res = await api<PageResponse>(withProject(`/api/pages/${encodePath(p)}`, projectRef.current));
      setContent(res.content);
      setErr(null);
    } catch (e) {
      setContent("");
      setErr(`读取 ${p} 失败：${String(e)}`);
    }
  };

  useEffect(() => {
    api<TreeResponse>(withProject("/api/tree", projectId))
      .then((res) => {
        const t = res.tree;
        setTree(t);
        treeRef.current = t;
        const deepLinked = currentRef.current;
        if (deepLinked) {
          setExpanded(ancestorsOf(t, deepLinked));
          void loadPage(deepLinked, false);
          return;
        }
        const entry = defaultEntry(t);
        if (entry) void loadPage(entry);
      })
      .catch((e) => setErr(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const urlPath = params.get("p");
    if (!urlPath || urlPath === currentRef.current) return;
    void loadPage(urlPath, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);

  // Track scroll for "back to top" and active outline item.
  useEffect(() => {
    const el = docRef.current;
    if (!el) return;
    const onScroll = () => {
      setShowTop(el.scrollTop > 280);
      const tops = headings
        .map((h) => {
          const node = el.querySelector(`#${CSS.escape(h.id)}`) as HTMLElement | null;
          return node ? { id: h.id, top: node.getBoundingClientRect().top - el.getBoundingClientRect().top } : null;
        })
        .filter(Boolean) as { id: string; top: number }[];
      let active: string | null = null;
      for (const t of tops) {
        if (t.top <= 72) active = t.id;
      }
      setActiveHeading(active || tops[0]?.id || null);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => el.removeEventListener("scroll", onScroll);
  }, [headings, current]);

  const scrollToHeading = (id: string) => {
    const el = docRef.current?.querySelector(`#${CSS.escape(id)}`) as HTMLElement | null;
    if (!el || !docRef.current) return;
    const top = el.getBoundingClientRect().top - docRef.current.getBoundingClientRect().top + docRef.current.scrollTop - 16;
    docRef.current.scrollTo({ top, behavior: "smooth" });
    setActiveHeading(id);
  };

  const count = useMemo(() => countNodes(tree), [tree]);

  return (
    <Box sx={{ height: { md: "100%" }, minHeight: 0, flexGrow: { md: 1 }, display: "flex", flexDirection: "column" }}>
      <Box
        className="atlas-fade"
        sx={{
          flexShrink: 0,
          px: { xs: 2, md: 3 },
          pt: 0.75,
          pb: 1.25,
          borderBottom: 1,
          borderColor: "divider",
          bgcolor: "rgba(255,255,255,0.55)",
          backdropFilter: "blur(8px)",
        }}
      >
        <Stack direction="row" alignItems="baseline" spacing={1.5} flexWrap="wrap">
          <Typography variant="h5" sx={{ fontWeight: 700 }}>
            项目文档
          </Typography>
          <Typography variant="body2" color="text.secondary">
            共 {count} 页
          </Typography>
        </Stack>
      </Box>
      {err && (
        <Alert severity="error" sx={{ m: 2, flexShrink: 0 }}>
          {err}
        </Alert>
      )}
      <Box
        sx={{
          flexGrow: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: { xs: "column", md: "row" },
          gap: { xs: 0, md: 2.5 },
          px: { xs: 1.5, md: 2.5 },
          py: { xs: 1.5, md: 2.5 },
        }}
      >
        {/* floating tree card, pinned left */}
        <Box
          sx={{
            width: { xs: "100%", md: 280 },
            flexShrink: 0,
            maxHeight: { xs: 300, md: "none" },
            height: { md: "100%" },
            minHeight: 0,
            display: "flex",
            flexDirection: "column",
            borderRadius: 1.5,
            bgcolor: "rgba(255,255,255,0.92)",
            border: "1px solid rgba(228,233,240,0.9)",
            boxShadow:
              "0 1px 2px rgba(16,24,40,0.04), 0 12px 32px rgba(16,24,40,0.08), 0 2px 8px rgba(26,111,181,0.06)",
            backdropFilter: "blur(12px)",
            overflow: "hidden",
          }}
        >
          <Box
            sx={{
              px: 1.5,
              py: 1,
              borderBottom: "1px solid",
              borderColor: "divider",
              flexShrink: 0,
              display: "flex",
              alignItems: "center",
              gap: 0.75,
            }}
          >
            <FolderOutlinedIcon fontSize="small" color="primary" />
            <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
              目录
            </Typography>
            <Typography variant="caption" color="text.secondary">
              {count}
            </Typography>
            <Box sx={{ ml: "auto" }}>
              <Tooltip title={allOpen ? "全部折叠" : "全部展开"}>
                <span>
                  <IconButton
                    size="small"
                    disabled={!folderIds.length}
                    onClick={() => {
                      if (allOpen) {
                        setExpanded({});
                      } else {
                        setExpanded(Object.fromEntries(folderIds.map((id) => [id, true])));
                      }
                    }}
                  >
                    {allOpen ? <UnfoldLessIcon fontSize="small" /> : <UnfoldMoreIcon fontSize="small" />}
                  </IconButton>
                </span>
              </Tooltip>
            </Box>
          </Box>
          <Box sx={{ flexGrow: 1, minHeight: 0, overflow: "auto", px: 1, py: 1 }}>
            {tree ? (
              <TreeView
                node={tree}
                expanded={expanded}
                onToggle={(id) => setExpanded((e) => ({ ...e, [id]: !e[id] }))}
                onSelect={loadPage}
                selected={current}
              />
            ) : (
              <Typography color="text.secondary" sx={{ px: 1.5, py: 1 }}>
                加载目录…
              </Typography>
            )}
          </Box>
        </Box>

        {/* document pane */}
        <Box
          ref={docRef}
          sx={{
            flexGrow: 1,
            minWidth: 0,
            height: { md: "100%" },
            overflow: "auto",
            px: { xs: 1, md: 1.5 },
            py: { xs: 0.5, md: 0.5 },
            position: "relative",
          }}
        >
          {!current && (
            <Typography color="text.secondary" sx={{ mt: 4, textAlign: "center" }}>
              从左侧选择文档
            </Typography>
          )}
          {current && (
            <div
              key={current}
              className="atlas-fade"
              style={{
                maxWidth: 880,
                width: "100%",
                marginLeft: "auto",
                marginRight: "auto",
              }}
            >
              <Stack direction="row" spacing={1} alignItems="center" sx={{ mb: 2 }}>
                <Chip size="small" color="primary" label={current} />
                <Tooltip title="复制本页链接">
                  <IconButton
                    size="small"
                    onClick={() => {
                      const url = `${window.location.origin}/reader?p=${encodeURIComponent(current)}`;
                      void navigator.clipboard?.writeText(url).then(() => setCopied(true));
                    }}
                  >
                    <LinkOutlinedIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              </Stack>
              <MarkdownView source={content} />
            </div>
          )}
          {showTop && (
            <Fab
              size="small"
              color="primary"
              onClick={() => docRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
              sx={{
                position: "sticky",
                bottom: 16,
                float: "right",
                mr: 1,
                boxShadow: "0 8px 20px rgba(26,111,181,0.35)",
              }}
            >
              <KeyboardArrowUpIcon />
            </Fab>
          )}
        </Box>

        {/* outline card, pinned right */}
        {headings.length > 0 && (
          <Box
            sx={{
              display: { xs: "none", lg: "flex" },
              width: 240,
              flexShrink: 0,
              height: "100%",
              minHeight: 0,
              flexDirection: "column",
              borderRadius: 1.5,
              bgcolor: "rgba(255,255,255,0.92)",
              border: "1px solid rgba(228,233,240,0.9)",
              boxShadow:
                "0 1px 2px rgba(16,24,40,0.04), 0 12px 32px rgba(16,24,40,0.08)",
              backdropFilter: "blur(12px)",
              overflow: "hidden",
            }}
          >
            <Box
              sx={{
                px: 1.5,
                py: 1,
                borderBottom: "1px solid",
                borderColor: "divider",
                display: "flex",
                alignItems: "center",
                gap: 0.75,
                flexShrink: 0,
              }}
            >
              <SubjectIcon fontSize="small" color="primary" />
              <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                本页目录
              </Typography>
            </Box>
            <Box sx={{ flexGrow: 1, minHeight: 0, overflow: "auto", px: 1, py: 1 }}>
              {headings.map((h) => {
                const active = activeHeading === h.id;
                return (
                  <ListItemButton
                    key={h.id}
                    dense
                    onClick={() => scrollToHeading(h.id)}
                    sx={{
                      borderRadius: 1,
                      pl: 1 + (h.level - 1) * 1.25,
                      py: 0.25,
                      bgcolor: active ? alpha("#1A6FB5", 0.1) : "transparent",
                      color: active ? "primary.dark" : "text.primary",
                      borderLeft: active ? "2px solid" : "2px solid transparent",
                      borderColor: active ? "primary.main" : "transparent",
                      "&:hover": { bgcolor: alpha("#1A6FB5", 0.06) },
                    }}
                  >
                    <ListItemText
                      primary={h.text}
                      primaryTypographyProps={{
                        fontSize: h.level <= 2 ? 13 : 12.5,
                        fontWeight: h.level <= 2 ? 700 : 500,
                        color: h.level >= 3 ? "text.secondary" : "inherit",
                        noWrap: true,
                      }}
                    />
                  </ListItemButton>
                );
              })}
            </Box>
          </Box>
        )}
      </Box>
      <Snackbar
        open={copied}
        autoHideDuration={2000}
        onClose={() => setCopied(false)}
        message="已复制页面链接"
      />
    </Box>
  );
}

// Percent-encode each path segment while keeping `/` separators, so Chinese file
// names and spaces survive the trip through `/api/pages/{*path}`.
function encodePath(p: string): string {
  return p
    .split("/")
    .map((seg) => encodeURIComponent(seg))
    .join("/");
}

// Collect the folder ids on the path to `path` so the tree reveals a deep-linked page.
function ancestorsOf(
  node: TreeNode | null,
  path: string,
  chain: string[] = []
): Record<string, boolean> {
  if (!node) return {};
  if (node.path === path) return Object.fromEntries(chain.map((id) => [id, true]));
  const nextChain = node.path ? chain : [...chain, node.id];
  for (const child of node.children || []) {
    const found = ancestorsOf(child, path, nextChain);
    if (Object.keys(found).length > 0) return found;
  }
  return {};
}

function countNodes(n: TreeNode | null): number {
  if (!n) return 0;
  let c = n.path ? 1 : 0;
  for (const ch of n.children || []) c += countNodes(ch);
  return c;
}

function collectFolderIds(n: TreeNode | null, out: string[] = []): string[] {
  if (!n) return out;
  if (!n.path && n.children?.length) out.push(n.id);
  for (const ch of n.children || []) collectFolderIds(ch, out);
  return out;
}

function basename(p: string): string {
  const parts = p.split("/");
  return parts[parts.length - 1] || p;
}

// Depth-first walk in display order, so "the first markdown file" is predictable.
function collectFiles(node: TreeNode | null, out: string[] = []): string[] {
  if (!node) return out;
  if (node.path) out.push(node.path);
  for (const ch of node.children || []) collectFiles(ch, out);
  return out;
}

// quickstart.md → README.md → first markdown file → first file of any kind.
function defaultEntry(tree: TreeNode | null): string | null {
  const files = collectFiles(tree);
  if (files.length === 0) return null;
  const named = (name: string) =>
    files.find((f) => basename(f).toLowerCase() === name.toLowerCase());
  return (
    named("quickstart.md") ||
    named("readme.md") ||
    files.find((f) => /\.(md|markdown)$/i.test(f)) ||
    files[0]
  );
}

function TreeView({
  node,
  expanded,
  onToggle,
  onSelect,
  selected,
  depth = 0,
}: {
  node: TreeNode;
  expanded: Record<string, boolean>;
  onToggle: (id: string) => void;
  onSelect: (path: string) => void;
  selected: string | null;
  depth?: number;
}) {
  if (node.path) {
    return (
      <ListItemButton
        dense
        selected={selected === node.path}
        onClick={() => onSelect(node.path!)}
        sx={{
          pl: 1 + depth * 1.5,
          borderRadius: 1.5,
          "&.Mui-selected": {
            bgcolor: alpha("#1A6FB5", 0.1),
            color: "primary.dark",
            fontWeight: 600,
          },
        }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          <DescriptionOutlinedIcon fontSize="small" color="action" />
        </ListItemIcon>
        <ListItemText
          primary={node.name}
          primaryTypographyProps={{ fontSize: 13.5, fontWeight: selected === node.path ? 700 : 500 }}
        />
      </ListItemButton>
    );
  }

  const open = expanded[node.id] ?? depth < 2;
  return (
    <>
      <ListItemButton
        dense
        onClick={() => onToggle(node.id)}
        sx={{ pl: 1 + depth * 1.5, borderRadius: 1.5 }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          {open ? <ExpandMoreIcon fontSize="small" /> : <ChevronRightIcon fontSize="small" />}
        </ListItemIcon>
        <ListItemIcon sx={{ minWidth: 24 }}>
          <FolderOutlinedIcon fontSize="small" color="primary" />
        </ListItemIcon>
        <ListItemText
          primary={node.name}
          primaryTypographyProps={{ fontSize: 13.5, fontWeight: 700 }}
        />
      </ListItemButton>
      <Collapse in={open} timeout="auto" unmountOnExit>
        <List disablePadding>
          {(node.children || [])
            .slice()
            .sort((a, b) => {
              const af = a.path ? 1 : 0;
              const bf = b.path ? 1 : 0;
              if (af !== bf) return af - bf;
              return a.name.localeCompare(b.name);
            })
            .map((ch) => (
              <TreeView
                key={ch.id}
                node={ch}
                expanded={expanded}
                onToggle={onToggle}
                onSelect={onSelect}
                selected={selected}
                depth={depth + 1}
              />
            ))}
        </List>
      </Collapse>
    </>
  );
}
