import {
  IconButton,
  Stack,
  Tooltip,
  Typography,
} from "@mui/material";
import ZoomInIcon from "@mui/icons-material/ZoomIn";
import ZoomOutIcon from "@mui/icons-material/ZoomOut";
import CenterFocusStrongIcon from "@mui/icons-material/CenterFocusStrong";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import CheckIcon from "@mui/icons-material/Check";
import OpenInFullIcon from "@mui/icons-material/OpenInFull";
import CloseIcon from "@mui/icons-material/Close";

/** Icon-only toolbar for the diagram canvas. */
export function DiagramToolbar({
  zoom,
  onZoomIn,
  onZoomOut,
  onFit,
  onCopy,
  onFullscreen,
  onCloseFullscreen,
  fullscreen,
  copied,
}: {
  zoom: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onCopy: () => void;
  onFullscreen: () => void;
  onCloseFullscreen?: () => void;
  fullscreen?: boolean;
  copied: boolean;
}) {
  return (
    <Stack
      direction="row"
      alignItems="center"
      spacing={0.25}
      sx={{
        px: 0.75,
        py: 0.5,
        borderBottom: "1px solid",
        borderColor: "divider",
        bgcolor: "rgba(255,255,255,0.85)",
        backdropFilter: "blur(8px)",
        flexShrink: 0,
        userSelect: "none",
      }}
    >
      <Tooltip title="缩小">
        <IconButton size="small" onClick={onZoomOut}>
          <ZoomOutIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Typography
        variant="caption"
        sx={{
          minWidth: 44,
          textAlign: "center",
          color: "text.secondary",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {Math.round(zoom * 100)}%
      </Typography>
      <Tooltip title="放大">
        <IconButton size="small" onClick={onZoomIn}>
          <ZoomInIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Tooltip title="适应视图">
        <IconButton size="small" onClick={onFit}>
          <CenterFocusStrongIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Tooltip title={copied ? "已复制" : "复制源码"}>
        <IconButton size="small" onClick={onCopy}>
          {copied ? (
            <CheckIcon fontSize="small" color="success" />
          ) : (
            <ContentCopyIcon fontSize="small" />
          )}
        </IconButton>
      </Tooltip>
      {fullscreen ? (
        <Tooltip title="关闭 (Esc)">
          <IconButton size="small" onClick={onCloseFullscreen}>
            <CloseIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      ) : (
        <Tooltip title="全屏">
          <IconButton size="small" onClick={onFullscreen}>
            <OpenInFullIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      )}
    </Stack>
  );
}
