const TOKEN_KEY = "atlas_token";
const AUTH_SCHEME = "Bearer";

export function getToken(): string {
  const url = new URLSearchParams(window.location.search).get("t");
  if (url) {
    localStorage.setItem(TOKEN_KEY, url);
    return url;
  }
  return localStorage.getItem(TOKEN_KEY) || "";
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      Authorization: `${AUTH_SCHEME} ${getToken()}`,
      ...(init?.headers || {}),
    },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(body || res.statusText);
  }
  return res.json() as Promise<T>;
}

export type Run = {
  id: string;
  mode: string;
  status: string;
  git_head?: string | null;
  provider?: string | null;
  model?: string | null;
  language?: string | null;
  prompt_tokens: number;
  completion_tokens: number;
  created_at: string;
  finished_at?: string | null;
};

export type Entity = {
  id: string;
  kind: string;
  name: string;
  canonical_key: string;
  status: string;
};

export type GraphData = {
  nodes: { id: string; kind: string; name: string; canonical_key: string }[];
  edges: { source: string; target: string; rel: string }[];
};

export type SearchHit = {
  kind: string;
  id: string;
  title: string;
  summary: string;
  page_path?: string | null;
  score: number;
};

export type ProjectEntry = { title: string; path: string };

export type Project = {
  id: string;
  name: string;
  root: string;
  language: string;
  provider: string;
  model: string;
  pages: number;
  chunks: number;
  entities: number;
  runs: number;
  gitHead?: string | null;
  updatedAt?: string | null;
  lastStatus?: string | null;
  entryPath: string;
  highlights: ProjectEntry[];
};

export type ProjectsResponse = { projects: Project[] };

export type ChatSource = {
  kind: string;
  title: string;
  summary?: string;
  page_path?: string | null;
};

export type ChatContext = {
  id: string;
  title: string;
  page_path?: string | null;
  summary: string;
  body: string;
  score: number;
  start_line?: number | null;
  end_line?: number | null;
  kind: string;
};

export type ChatUsage = {
  prompt_tokens?: number;
  completion_tokens?: number;
  total_tokens?: number;
};

export type ChatResponse = {
  answer?: string;
  mode?: "llm" | "retrieval";
  sources?: ChatSource[];
  contexts?: ChatContext[];
  model?: string;
  usage?: ChatUsage;
  error?: string;
};

export type ChatStreamEvent =
  | { type: "meta"; mode?: "llm" | "retrieval"; sources?: ChatSource[]; contexts?: ChatContext[] }
  | { type: "delta"; text: string }
  | {
      type: "done";
      mode?: "llm" | "retrieval";
      answer?: string;
      sources?: ChatSource[];
      contexts?: ChatContext[];
      model?: string;
      usage?: ChatUsage;
      error?: string;
    };

/** POST and consume an SSE stream of `data: {json}` frames. */
export async function streamApi(
  path: string,
  init: RequestInit,
  onEvent: (ev: ChatStreamEvent) => void
): Promise<void> {
  const res = await fetch(path, {
    ...init,
    headers: {
      Authorization: `${AUTH_SCHEME} ${getToken()}`,
      Accept: "text/event-stream",
      ...(init?.headers || {}),
    },
  });
  if (!res.ok || !res.body) {
    const body = await res.text().catch(() => "");
    throw new Error(body || res.statusText);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let sep: number;
    while ((sep = buf.indexOf("\n\n")) >= 0) {
      const frame = buf.slice(0, sep);
      buf = buf.slice(sep + 2);
      for (const line of frame.split("\n")) {
        if (!line.startsWith("data:")) continue;
        const raw = line.slice(5).trim();
        if (!raw || raw === "[DONE]") continue;
        try {
          onEvent(JSON.parse(raw) as ChatStreamEvent);
        } catch {
          /* ignore malformed frame */
        }
      }
    }
  }
}
