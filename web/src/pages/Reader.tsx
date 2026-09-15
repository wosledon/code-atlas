import { useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Box,
  Card,
  CardContent,
  Chip,
  Collapse,
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
import LinkOutlinedIcon from "@mui/icons-material/LinkOutlined";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { MarkdownView } from "../components/MarkdownView";

type TreeNode = {
  id: string;
  name: string;
  path?: string | null;
  children: TreeNode[];
};

export default function ReaderPage() {
  const [params, setParams] = useSearchParams();
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [content, setContent] = useState("");
  const [current, setCurrent] = useState<string | null>(params.get("p"));
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Folders are expanded on demand; nothing is keyed to a hard-coded layout.
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const currentRef = useRef<string | null>(params.get("p"));
  const treeRef = useRef<TreeNode | null>(null);

  const loadPage = async (p: string, syncUrl = true) => {
    setCurrent(p);
    currentRef.current = p;
    if (syncUrl) setParams({ p }, { replace: true });
    // reveal the selected page in the tree
    setExpanded((e) => ({ ...e, ...ancestorsOf(treeRef.current, p) }));
    try {
      const res = await api<{ content: string }>(`/api/pages/${encodePath(p)}`);
      setContent(res.content);
      setErr(null);
    } catch (e) {
      setContent("");
      setErr(`读取 ${p} 失败：${String(e)}`);
    }
  };

  useEffect(() => {
    api<TreeNode>("/api/tree")
      .then((t) => {
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

  // Follow deep links that arrive while the reader stays mounted (same route, new ?p=).
  useEffect(() => {
    const urlPath = params.get("p");
    if (!urlPath || urlPath === currentRef.current) return;
    void loadPage(urlPath, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);

  const count = useMemo(() => countNodes(tree), [tree]);

  return (
    <Stack
      spacing={2.5}
      sx={{ height: { md: "100%" }, minHeight: 0, flexGrow: { md: 1 }, alignItems: "stretch" }}
    >
      <Box className="atlas-fade" sx={{ flexShrink: 0 }}>
        <Typography variant="h4">项目文档</Typography>
        <Typography color="text.secondary" sx={{ mt: 0.5 }}>
          树状目录 · 共 {count} 页 · 点击文件阅读；折叠/展开文件夹
        </Typography>
      </Box>
      {err && <Alert severity="error">{err}</Alert>}
      <Stack
        direction={{ xs: "column", md: "row" }}
        spacing={2.5}
        sx={{ flexGrow: 1, minHeight: 0, alignItems: "stretch" }}
      >
        <Card
          sx={{
            width: { md: 320 },
            flexShrink: 0,
            height: { md: "100%" },
            maxHeight: { xs: 320, md: "none" },
            overflow: "auto",
          }}
        >
          <CardContent sx={{ py: 1.5 }}>
            {tree ? (
              <TreeView
                node={tree}
                expanded={expanded}
                onToggle={(id) => setExpanded((e) => ({ ...e, [id]: !e[id] }))}
                onSelect={loadPage}
                selected={current}
              />
            ) : (
              <Typography color="text.secondary">加载目录…</Typography>
            )}
          </CardContent>
        </Card>
        <Card
          sx={{
            flexGrow: 1,
            width: "100%",
            maxWidth: 860,
            minWidth: 0,
            height: { md: "100%" },
            overflow: "auto",
          }}
        >
          <CardContent sx={{ p: { xs: 2, md: 3.5 } }}>
            {!current && <Typography color="text.secondary">从左侧选择文档</Typography>}
            {current && (
              <div key={current} className="atlas-fade">
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
          </CardContent>
        </Card>
      </Stack>
      <Snackbar
        open={copied}
        autoHideDuration={2000}
        onClose={() => setCopied(false)}
        message="已复制页面链接"
      />
    </Stack>
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
