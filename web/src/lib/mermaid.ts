// Render-time repair for mermaid diagrams written by the LLM.
//
// Generated pages regularly contain syntax the mermaid parser rejects, even
// though the surrounding markdown is fine:
//   * a reserved word used as node id      graph["graph.rs"]  -> lexical error
//   * an unquoted label starting with `/`  U[/api/pages]      -> parallelogram
//   * an unquoted label containing `(`     C[serve()]         -> parse error
//   * flowchart shapes in a sequence diagram, which has none:
//     cli["atlas-cli<br/>main.rs:112"]     -> `participant cli as atlas-cli`
// The writer is told to quote labels, but pages already on disk (and any future
// slip) should still render, so the reader repairs the source *after* the
// original failed to parse. A diagram that already renders is never rewritten.

/** Words mermaid lexes as keywords; using one as node id breaks the parse. */
const RESERVED_IDS = new Set([
  "graph",
  "flowchart",
  "subgraph",
  "end",
  "class",
  "classDef",
  "style",
  "click",
  "linkStyle",
  "direction",
  "default",
  "interpolate",
  "accTitle",
  "accDescr",
]);

/** Lines declaring styling, links or subgraph boundaries instead of nodes. */
const DIRECTIVE =
  /^\s*(?:%%|style\b|classDef\b|class\b|click\b|linkStyle\b|direction\b|subgraph\b|accTitle\b|accDescr\b|end\s*;?\s*$)/;

/** The grammars this repairs; every other diagram kind passes through. */
const FLOW_HEADER = /^\s*(?:flowchart|graph)\b/;
const SEQUENCE_HEADER = /^\s*sequenceDiagram\b/;

/** Characters mermaid reads as syntax when a label is left unquoted. */
const NEEDS_QUOTE = /[(),"]|^\//;

/** Shape delimiters, longest first so `[[` is not read as a nested `[`. */
const SHAPES: ReadonlyArray<readonly [string, string]> = [
  ["([", "])"],
  ["[(", ")]"],
  ["[[", "]]"],
  ["((", "))"],
  ["{{", "}}"],
  ["[", "]"],
  ["(", ")"],
  ["{", "}"],
];

const ID_START = /[A-Za-z_]/;
const ID_BODY = /[A-Za-z0-9_-]/;
/** An arrow on either side of an identifier puts it in node-id position. */
const EDGE_AHEAD = /^\s*(?:[-=.<]|\.{1,2}|~{3})/;
const EDGE_BEHIND = /[-|>.<>=~]\s*$/;

export interface MermaidRepair {
  code: string;
  /** Number of syntax edits applied; `0` means the source is unchanged. */
  edits: number;
}

interface Shape {
  /** Index of the opening delimiter. */
  start: number;
  /** Index of the first content character. */
  contentStart: number;
  /** Index of the closing delimiter. */
  end: number;
  open: string;
  close: string;
  content: string;
}

/**
 * Repair the source of a diagram that failed to parse. Flowchart labels and
 * reserved ids are quoted/renamed; in a sequence diagram a participant declared
 * with a flowchart shape is rewritten as a real declaration. Returns the input
 * untouched (with `edits: 0`) when there is nothing safe to fix.
 */
export function repairMermaid(code: string): MermaidRepair {
  const lines = code.split("\n");
  const header = lines.findIndex(
    (line) =>
      !line.trimStart().startsWith("%%") &&
      (FLOW_HEADER.test(line) || SEQUENCE_HEADER.test(line)),
  );
  if (header === -1) return { code, edits: 0 };
  const repair = SEQUENCE_HEADER.test(lines[header]) ? repairSequenceLine : repairFlowchartLine;
  let edits = 0;
  for (let i = 0; i < lines.length; i += 1) {
    if (i === header) continue;
    const fixed = repair(lines[i]);
    edits += fixed.edits;
    lines[i] = fixed.line;
  }
  return edits === 0 ? { code, edits: 0 } : { code: lines.join("\n"), edits };
}

/** Styling and subgraph boundaries are structure, not nodes. */
function repairFlowchartLine(line: string): { line: string; edits: number } {
  if (DIRECTIVE.test(line)) return { line, edits: 0 };
  return repairLine(line);
}

/**
 * `cli["atlas-cli<br/>main.rs:112"]` inside a `sequenceDiagram` is a flowchart
 * node declaration: the sequence grammar has no shapes, so the parse dies on the
 * first one. Mermaid spells the same thing `participant cli as atlas-cli…` (the
 * label runs to end of line, so parentheses and slashes need no quoting). The
 * whole line must be the declaration — a message line carries `->>` or `:` and
 * is left alone.
 */
function repairSequenceLine(line: string): { line: string; edits: number } {
  const unchanged = { line, edits: 0 };
  const trimmed = line.trimStart();
  if (trimmed === "" || trimmed.startsWith("%%")) return unchanged;
  const indent = line.slice(0, line.length - trimmed.length);
  const withKeyword = /^(actor|participant)\s+/.exec(trimmed);
  const after = withKeyword ? trimmed.slice(withKeyword[0].length) : trimmed;
  const id = /^[A-Za-z_][A-Za-z0-9_-]*/.exec(after)?.[0];
  if (id === undefined) return unchanged;
  const shape = readShape(after, id.length);
  if (shape === null || !/^\s*$/.test(after.slice(shape.end + shape.close.length))) {
    return unchanged;
  }
  const raw = shape.content.trim();
  const label = isQuoted(raw) ? raw.slice(1, -1).replace(/#quot;/g, '"') : raw;
  if (label === "") return unchanged;
  const keyword = withKeyword ? withKeyword[1] : "participant";
  return { line: `${indent}${keyword} ${id} as ${label}`, edits: 1 };
}

function repairLine(line: string): { line: string; edits: number } {
  let out = "";
  let edits = 0;
  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (ch === '"') {
      const stop = quotedEnd(line, i);
      out += line.slice(i, stop);
      i = stop;
      continue;
    }
    if (ch === "|") {
      const end = line.indexOf("|", i + 1);
      if (end === -1) {
        out += ch;
        i += 1;
        continue;
      }
      const label = line.slice(i + 1, end).trim();
      if (label !== "" && !isQuoted(label) && NEEDS_QUOTE.test(label)) {
        out += `|"${escapeLabel(line.slice(i + 1, end))}"|`;
        edits += 1;
      } else {
        out += line.slice(i, end + 1);
      }
      i = end + 1;
      continue;
    }
    if (ID_START.test(ch)) {
      let j = i;
      while (j < line.length && ID_BODY.test(line[j])) j += 1;
      const token = line.slice(i, j);
      const shape = readShape(line, j);
      const isId =
        shape !== null || EDGE_AHEAD.test(line.slice(j)) || EDGE_BEHIND.test(out);
      if (isId && RESERVED_IDS.has(token)) {
        out += `${token}_n`;
        edits += 1;
      } else {
        out += token;
      }
      i = j;
      if (shape) {
        const fixed = quoteShape(line, shape);
        out += fixed.text;
        if (fixed.quoted) edits += 1;
        i = shape.end + shape.close.length;
      }
      continue;
    }
    out += ch;
    i += 1;
  }
  return { line: out, edits };
}

/** Emit a node's `label` part, quoting it when mermaid would misread it. */
function quoteShape(line: string, shape: Shape): { text: string; quoted: boolean } {
  const raw = line.slice(shape.start, shape.end + shape.close.length);
  const inner = shape.content.trim();
  if (inner === "" || isQuoted(inner) || !NEEDS_QUOTE.test(inner)) {
    return { text: raw, quoted: false };
  }
  const open = line.slice(shape.start, shape.contentStart);
  return { text: `${open}"${escapeLabel(shape.content)}"${shape.close}`, quoted: true };
}

/** Read the node shape starting at `from`, if the line has one there. */
function readShape(line: string, from: number): Shape | null {
  for (const [open, close] of SHAPES) {
    if (!line.startsWith(open, from)) continue;
    const contentStart = from + open.length;
    const end = findClose(line, contentStart, open, close);
    if (end === -1) continue;
    return {
      start: from,
      contentStart,
      end,
      open,
      close,
      content: line.slice(contentStart, end),
    };
  }
  return null;
}

/** Index of the closing delimiter, honouring nesting for single-char shapes. */
function findClose(line: string, from: number, open: string, close: string): number {
  if (open.length !== 1 || close.length !== 1) return line.indexOf(close, from);
  let depth = 1;
  for (let i = from; i < line.length; i += 1) {
    if (line[i] === '"') {
      i = quotedEnd(line, i) - 1;
      continue;
    }
    if (line[i] === open) depth += 1;
    else if (line[i] === close) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/** Index just past the quoted span starting at `i` (== line.length if unterminated). */
function quotedEnd(line: string, i: number): number {
  const end = line.indexOf('"', i + 1);
  return end === -1 ? line.length : end + 1;
}

function isQuoted(text: string): boolean {
  return /^"[^"]*"$/.test(text);
}

/** `#quot;` is the entity mermaid renders as `"` inside a quoted label. */
function escapeLabel(text: string): string {
  return text.replace(/"/g, "#quot;");
}

/** First lines of a mermaid parse error, for the fallback panel. */
export function mermaidErrorBrief(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  const lines = text
    .split("\n")
    .map((part) => part.trim())
    .filter((part) => part !== "")
    .slice(0, 3);
  return (lines.length > 0 ? lines.join(" ") : text.trim()).slice(0, 260);
}

/** mermaid leaves the failed render's scratch nodes in the DOM; drop them. */
export function cleanupMermaidArtifacts(...ids: string[]): void {
  for (const id of ids) {
    for (const domId of [id, `d${id}`]) document.getElementById(domId)?.remove();
  }
}
