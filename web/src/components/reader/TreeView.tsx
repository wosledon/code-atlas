import {
  Collapse,
  List,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  alpha,
} from "@mui/material";
import FolderOutlinedIcon from "@mui/icons-material/FolderOutlined";
import DescriptionOutlinedIcon from "@mui/icons-material/DescriptionOutlined";
import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import ChevronRightIcon from "@mui/icons-material/ChevronRight";
import type { TreeNode } from "./treeUtils";

export function TreeView({
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
          primaryTypographyProps={{
            fontSize: 13.5,
            fontWeight: selected === node.path ? 700 : 500,
          }}
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
