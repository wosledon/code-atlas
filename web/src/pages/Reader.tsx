import { useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Box,
  Chip,
  Fab,
  IconButton,
  Snackbar,
  Stack,
  Tooltip,
  Typography,
} from "@mui/material";
import LinkOutlinedIcon from "@mui/icons-material/LinkOutlined";
import KeyboardArrowUpIcon from "@mui/icons-material/KeyboardArrowUp";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { PageResponse, TreeResponse } from "../lib/api";
import { useProject } from "../lib/projectContext";
import { withProject } from "../lib/routes";
import { MarkdownView, extractHeadings } from "../components/MarkdownView";
import { DocOutline, DocTreePanel } from "../components/reader/DocTreePanel";
import {
  ancestorsOf,
  collectFolderIds,
  countNodes,
  defaultEntry,
  encodePath,
  type TreeNode,
} from "../components/reader/treeUtils";

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
      const res = await api<PageResponse>(
        withProject(`/api/pages/${encodePath(p)}`, projectRef.current)
      );
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

  useEffect(() => {
    const el = docRef.current;
    if (!el) return;
    const onScroll = () => {
      setShowTop(el.scrollTop > 280);
      const tops = headings
        .map((h) => {
          const node = el.querySelector(`#${CSS.escape(h.id)}`) as HTMLElement | null;
          return node
            ? { id: h.id, top: node.getBoundingClientRect().top - el.getBoundingClientRect().top }
            : null;
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
    const top =
      el.getBoundingClientRect().top - docRef.current.getBoundingClientRect().top + docRef.current.scrollTop - 16;
    docRef.current.scrollTo({ top, behavior: "smooth" });
    setActiveHeading(id);
  };

  const count = useMemo(() => countNodes(tree), [tree]);

  return (
    <Box
      sx={{
        height: { md: "100%" },
        minHeight: 0,
        flexGrow: { md: 1 },
        display: "flex",
        flexDirection: "column",
      }}
    >
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
        <DocTreePanel
          tree={tree}
          count={count}
          folderIds={folderIds}
          expanded={expanded}
          allOpen={allOpen}
          current={current}
          onToggle={(id) => setExpanded((e) => ({ ...e, [id]: !e[id] }))}
          onSelect={loadPage}
          onExpandAll={() => setExpanded(Object.fromEntries(folderIds.map((id) => [id, true])))}
          onCollapseAll={() => setExpanded({})}
        />

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

        {headings.length > 0 && (
          <DocOutline headings={headings} activeHeading={activeHeading} onSelect={scrollToHeading} />
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
