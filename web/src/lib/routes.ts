import { readStoredProjectId } from "./projectContext";

// Deep link into the reader for a Wiki page (path relative to the atlas root).
export function readerHref(pagePath: string, projectId?: string): string {
  const q = new URLSearchParams({ p: pagePath });
  const project = projectId || currentProjectId();
  if (project) q.set("project", project);
  return `/reader?${q.toString()}`;
}

/** Active project: URL → localStorage → null (launch). */
export function currentProjectId(): string | null {
  const fromUrl = new URLSearchParams(window.location.search).get("project");
  if (fromUrl && fromUrl.trim()) return fromUrl.trim();
  return readStoredProjectId();
}

/** Append `project` query param when a project context is active. */
export function withProject(path: string, projectId?: string | null): string {
  const p = projectId ?? currentProjectId();
  if (!p) return path;
  const sep = path.includes("?") ? "&" : "?";
  return `${path}${sep}project=${encodeURIComponent(p)}`;
}
