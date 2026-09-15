import { createTheme, alpha } from "@mui/material/styles";

export const atlasTheme = createTheme({
  palette: {
    mode: "light",
    primary: { main: "#1A6FB5", light: "#5B9FE0", dark: "#0F4A80", contrastText: "#FFFFFF" },
    secondary: { main: "#3D8B6E", light: "#6FB99A", dark: "#1F5A44" },
    error: { main: "#B54545" },
    warning: { main: "#B58A1A" },
    background: {
      default: "#F4F6FA",
      paper: "#FFFFFF",
    },
    text: { primary: "#141A22", secondary: "#5A6678" },
    divider: "#E4E9F0",
  },
  shape: { borderRadius: 14 },
  typography: {
    fontFamily: [
      "IBM Plex Sans",
      "Segoe UI",
      "system-ui",
      "PingFang SC",
      "Microsoft YaHei",
      "sans-serif",
    ].join(","),
    h4: { fontWeight: 700, letterSpacing: "-0.02em" },
    h5: { fontWeight: 700, letterSpacing: "-0.01em" },
    h6: { fontWeight: 600 },
    subtitle1: { fontWeight: 600 },
  },
  shadows: [
    "none",
    "0 1px 2px rgba(20,26,34,0.04), 0 1px 3px rgba(20,26,34,0.06)",
    "0 2px 4px rgba(20,26,34,0.04), 0 4px 12px rgba(20,26,34,0.06)",
    "0 4px 8px rgba(20,26,34,0.05), 0 8px 24px rgba(20,26,34,0.08)",
    "0 8px 16px rgba(20,26,34,0.06), 0 16px 40px rgba(20,26,34,0.1)",
    ...Array(20).fill("0 8px 16px rgba(20,26,34,0.06), 0 16px 40px rgba(20,26,34,0.1)"),
  ] as never,
  components: {
    MuiCard: {
      defaultProps: { elevation: 0 },
      styleOverrides: {
        root: {
          borderRadius: 16,
          border: "1px solid #E4E9F0",
          background: "#FFFFFF",
          transition: "transform 0.2s ease, box-shadow 0.2s ease, border-color 0.2s ease",
          "&:hover": {
            transform: "translateY(-2px)",
            boxShadow: "0 10px 28px rgba(26,111,181,0.12)",
            borderColor: alpha("#1A6FB5", 0.25),
          },
        },
      },
    },
    MuiButton: {
      styleOverrides: {
        root: {
          borderRadius: 10,
          textTransform: "none",
          fontWeight: 600,
          boxShadow: "none",
          paddingInline: 16,
        },
        contained: {
          boxShadow: "0 4px 14px rgba(26,111,181,0.28)",
          "&:hover": { boxShadow: "0 6px 18px rgba(26,111,181,0.36)" },
        },
      },
    },
    MuiChip: {
      styleOverrides: {
        root: { borderRadius: 8, fontWeight: 500 },
      },
    },
    MuiPaper: {
      styleOverrides: {
        outlined: { borderRadius: 16, borderColor: "#E4E9F0" },
      },
    },
    MuiTextField: {
      defaultProps: { size: "small" },
    },
    MuiAppBar: {
      styleOverrides: {
        root: {
          backgroundImage: "none",
          backdropFilter: "blur(12px)",
          backgroundColor: "rgba(255,255,255,0.82)",
        },
      },
    },
  },
});

export const kindColor = (kind: string): string => {
  switch (kind) {
    case "page":
      return "#1A6FB5";
    case "module":
      return "#3D8B6E";
    case "symbol":
      return "#8B6B9E";
    case "api":
      return "#B58A1A";
    case "data":
      return "#B54545";
    case "config":
      return "#5A6678";
    case "external":
      return "#0F4A80";
    default:
      return "#5A6678";
  }
};
