import { Box, IconButton, ListItemButton, ListItemText, Tooltip, Typography, alpha } from "@mui/material";
import FolderOutlinedIcon from "@mui/icons-material/FolderOutlined";
import UnfoldLessIcon from "@mui/icons-material/UnfoldLess";
import UnfoldMoreIcon from "@mui/icons-material/UnfoldMore";
import SubjectIcon from "@mui/icons-material/Subject";
import type { TreeNode } from "./treeUtils";
import { TreeView } from "./TreeView";

export function DocTreePanel({
  tree,
  count,
  folderIds,
  expanded,
  allOpen,
  current,
  onToggle,
  onSelect,
  onExpandAll,
  onCollapseAll,
}: {
  tree: TreeNode | null;
  count: number;
  folderIds: string[];
  expanded: Record<string, boolean>;
  allOpen: boolean;
  current: string | null;
  onToggle: (id: string) => void;
  onSelect: (path: string) => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
}) {
  return (
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
                onClick={() => (allOpen ? onCollapseAll() : onExpandAll())}
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
            onToggle={onToggle}
            onSelect={onSelect}
            selected={current}
          />
        ) : (
          <Typography color="text.secondary" sx={{ px: 1.5, py: 1 }}>
            加载目录…
          </Typography>
        )}
      </Box>
    </Box>
  );
}

export function DocOutline({
  headings,
  activeHeading,
  onSelect,
}: {
  headings: { id: string; text: string; level: number }[];
  activeHeading: string | null;
  onSelect: (id: string) => void;
}) {
  return (
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
        boxShadow: "0 1px 2px rgba(16,24,40,0.04), 0 12px 32px rgba(16,24,40,0.08)",
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
              onClick={() => onSelect(h.id)}
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
  );
}
