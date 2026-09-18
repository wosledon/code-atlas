import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import mermaid from "mermaid";
import { MermaidBlock } from "./MermaidBlock";

mermaid.initialize({
  startOnLoad: false,
  theme: "base",
  securityLevel: "strict",
  htmlLabels: false,
  fontFamily: "IBM Plex Sans, Segoe UI, PingFang SC, Microsoft YaHei, sans-serif",
  themeVariables: {
    primaryColor: "#EAF3FB",
    primaryTextColor: "#141A22",
    primaryBorderColor: "#1A6FB5",
    secondaryColor: "#EEF6F3",
    secondaryTextColor: "#141A22",
    secondaryBorderColor: "#3D8B6E",
    tertiaryColor: "#F7F8FA",
    tertiaryTextColor: "#4A5563",
    tertiaryBorderColor: "#C9D1DB",
    lineColor: "#7A8699",
    textColor: "#141A22",
    mainBkg: "#EAF3FB",
    nodeBorder: "#1A6FB5",
    clusterBkg: "#F7F8FA",
    clusterBorder: "#D5DCE6",
    edgeLabelBackground: "#FFFFFF",
    fontSize: "13px",
  },
  flowchart: {
    curve: "basis",
    padding: 12,
    nodeSpacing: 40,
    rankSpacing: 48,
    useMaxWidth: true,
  },
  sequence: {
    useMaxWidth: true,
    actorMargin: 50,
    messageMargin: 40,
    mirrorActors: false,
    bottomMarginAdj: 1,
  },
  gantt: {
    useMaxWidth: true,
    barHeight: 22,
    fontSize: 12,
  },
});

export function MarkdownView({
  source,
  streaming = false,
}: {
  source: string;
  /** Source is still arriving: a fence without its closing ``` is normal. */
  streaming?: boolean;
}) {
  const nodes = useMemo(() => parseBlocks(source, streaming), [source, streaming]);
  return (
    <div className="md-view" style={{ lineHeight: 1.65, fontSize: 15, color: "#141A22" }}>
      <style>{mdCss}</style>
      {nodes}
    </div>
  );
}

const mdCss = `
.md-view table { border-collapse: collapse; width: 100%; margin: 12px 0 18px; font-size: 13.5px; }
.md-view th, .md-view td { border: 1px solid #E4E9F0; padding: 8px 10px; text-align: left; vertical-align: top; }
.md-view th { background: #F4F6FA; font-weight: 600; }
.md-view tr:nth-child(even) td { background: #FAFBFD; }
.md-view code { background: #EEF1F5; padding: 1px 5px; border-radius: 4px; font-family: "IBM Plex Mono", ui-monospace, monospace; font-size: 0.92em; }
.md-view a { color: #1A6FB5; }
.md-view h1, .md-view h2, .md-view h3, .md-view h4 { font-weight: 700; letter-spacing: -0.01em; scroll-margin-top: 16px; }
.md-view h1 { font-size: 28px; margin: 8px 0 12px; }
.md-view h2 { font-size: 21px; margin: 22px 0 10px; padding-bottom: 6px; border-bottom: 1px solid #EEF1F5; }
.md-view h3 { font-size: 17px; margin: 18px 0 8px; }
.md-view ul, .md-view ol { padding-left: 22px; margin: 8px 0 14px; }
.md-view li { margin: 4px 0; }
.md-view blockquote { margin: 12px 0; padding: 8px 14px; border-left: 3px solid #1A6FB5; background: #F4F8FC; color: #4A5563; border-radius: 0 8px 8px 0; }
.md-view hr { border: none; border-top: 1px solid #E4E9F0; margin: 20px 0; }
`;

export function headingId(text: string, level: number, index: number): string {
  const slug = text
    .toLowerCase()
    .replace(/[^\w一-鿿]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  return `h-${level}-${index}-${slug || "sec"}`;
}

/** Flatten markdown headings (skips fenced code) for the outline rail. */
export function extractHeadings(md: string): { level: number; text: string; id: string }[] {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  let inFence = false;
  let idx = 0;
  let skippedFm = false;
  const out: { level: number; text: string; id: string }[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (i === 0 && line.trim() === "---") {
      // front matter
      let j = 1;
      while (j < lines.length && lines[j].trim() !== "---") j++;
      i = j;
      skippedFm = true;
      continue;
    }
    if (line.trimStart().startsWith("```")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (!h) continue;
    const level = h[1].length;
    const text = h[2].trim();
    out.push({ level, text, id: headingId(text, level, idx++) });
  }
  void skippedFm;
  return out;
}

function parseBlocks(md: string, streaming: boolean): ReactNode[] {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;
  let headingIdx = 0;

  if (lines[0]?.trim() === "---") {
    i = 1;
    while (i < lines.length && lines[i].trim() !== "---") i++;
    i++;
  }

  while (i < lines.length) {
    const line = lines[i];

    // fenced code / mermaid
    if (line.trimStart().startsWith("```")) {
      const lang = line.trim().slice(3).trim().toLowerCase();
      const buf: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        buf.push(lines[i]);
        i++;
      }
      // No closing fence: while the answer is still streaming the block is
      // half-written, and a diagram parsed from it fails every time.
      const closed = i < lines.length;
      i++;
      const code = buf.join("\n");
      if (lang === "mermaid") {
        out.push(<MermaidBlock key={key++} code={code} pending={streaming && !closed} />);
      } else {
        out.push(
          <pre
            key={key++}
            style={{
              background: "#F4F6FA",
              border: "1px solid #E4E9F0",
              padding: 14,
              borderRadius: 12,
              overflow: "auto",
              fontSize: 12.5,
              lineHeight: 1.55,
              margin: "12px 0 18px",
              fontFamily: "IBM Plex Mono, Cascadia Code, ui-monospace, monospace",
            }}
          >
            {lang ? (
              <div style={{ color: "#8B95A5", marginBottom: 8, fontSize: 11, textTransform: "uppercase" }}>
                {lang}
              </div>
            ) : null}
            {code}
          </pre>
        );
      }
      continue;
    }

    // table
    if (line.includes("|") && i + 1 < lines.length && /^\s*\|?[\s:-]+\|/.test(lines[i + 1])) {
      const header = splitRow(line);
      i += 2; // skip separator
      const rows: string[][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        rows.push(splitRow(lines[i]));
        i++;
      }
      out.push(
        <div key={key++} style={{ overflowX: "auto", margin: "12px 0 18px" }}>
          <table>
            <thead>
              <tr>
                {header.map((h, hi) => (
                  <th key={hi}>{renderInline(h)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r, ri) => (
                <tr key={ri}>
                  {header.map((_, ci) => (
                    <td key={ci}>{renderInline(r[ci] ?? "")}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
      continue;
    }

    // heading
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      const level = h[1].length;
      const text = h[2].trim();
      const Tag = (`h${Math.min(4, level)}` as unknown) as "h1" | "h2" | "h3" | "h4";
      out.push(
        <Tag key={key++} id={headingId(text, Math.min(4, level), headingIdx++)}>
          {renderInline(text)}
        </Tag>
      );
      i++;
      continue;
    }

    if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const items: string[] = [];
      while (i < lines.length && /^\s*([-*+]|\d+\.)\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*([-*+]|\d+\.)\s+/, ""));
        i++;
      }
      const ListTag = ordered ? "ol" : "ul";
      out.push(
        <ListTag key={key++}>
          {items.map((t, idx) => (
            <li key={idx}>{renderInline(t)}</li>
          ))}
        </ListTag>
      );
      continue;
    }

    if (line.trim() === "---" || line.trim() === "***") {
      out.push(<hr key={key++} />);
      i++;
      continue;
    }

    if (line.trimStart().startsWith(">")) {
      const buf: string[] = [];
      while (i < lines.length && lines[i].trimStart().startsWith(">")) {
        buf.push(lines[i].replace(/^\s*>\s?/, ""));
        i++;
      }
      out.push(<blockquote key={key++}>{renderInline(buf.join(" "))}</blockquote>);
      continue;
    }

    if (line.trim() === "") {
      i++;
      continue;
    }

    // paragraph
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !lines[i].startsWith("#") &&
      !lines[i].trimStart().startsWith("```") &&
      !/^\s*([-*+]|\d+\.)\s+/.test(lines[i]) &&
      !lines[i].trimStart().startsWith(">") &&
      !(lines[i].includes("|") && i + 1 < lines.length && /^\s*\|?[\s:-]+\|/.test(lines[i + 1] || ""))
    ) {
      para.push(lines[i]);
      i++;
    }
    out.push(<p key={key++} style={{ margin: "8px 0 14px" }}>{renderInline(para.join(" "))}</p>);
  }
  return out;
}

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  return s.split("|").map((c) => c.trim());
}

function renderInline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*]+\*)|(\[[^\]]+\]\([^)]+\))/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    const tok = m[0];
    if (tok.startsWith("`")) {
      parts.push(<code key={k++}>{tok.slice(1, -1)}</code>);
    } else if (tok.startsWith("**")) {
      parts.push(<strong key={k++}>{tok.slice(2, -2)}</strong>);
    } else if (tok.startsWith("*")) {
      parts.push(<em key={k++}>{tok.slice(1, -1)}</em>);
    } else {
      const lm = /\[([^\]]+)\]\(([^)]+)\)/.exec(tok);
      if (lm) {
        parts.push(
          <a key={k++} href={lm[2]} target="_blank" rel="noreferrer">
            {lm[1]}
          </a>
        );
      }
    }
    last = m.index + tok.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}
