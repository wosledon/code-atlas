export type TreeNode = {
  id: string;
  name: string;
  path?: string | null;
  children: TreeNode[];
};

/** Percent-encode each path segment while keeping `/` separators. */
export function encodePath(p: string): string {
  return p
    .split("/")
    .map((seg) => encodeURIComponent(seg))
    .join("/");
}

/** Collect folder ids on the path to `path` so the tree reveals a deep-linked page. */
export function ancestorsOf(
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

export function countNodes(n: TreeNode | null): number {
  if (!n) return 0;
  let c = n.path ? 1 : 0;
  for (const ch of n.children || []) c += countNodes(ch);
  return c;
}

export function collectFolderIds(n: TreeNode | null, out: string[] = []): string[] {
  if (!n) return out;
  if (!n.path && n.children?.length) out.push(n.id);
  for (const ch of n.children || []) collectFolderIds(ch, out);
  return out;
}

/**
 * Folders open before the reader touches anything: the top `maxDepth` levels, so
 * the shape of the wiki is visible at a glance.
 *
 * This has to be state, not a fallback in the renderer: a `expanded[id] ?? depth
 * < 2` default cannot be told apart from an explicit `false`, so the first click
 * on a default-open folder computed `!undefined` = `true` and appeared dead, and
 * "collapse all" (which cleared the map) fell straight back to open.
 */
export function defaultExpanded(
  node: TreeNode | null,
  maxDepth = 2,
  depth = 0,
  out: Record<string, boolean> = {}
): Record<string, boolean> {
  if (!node) return out;
  if (!node.path && node.children?.length && depth < maxDepth) out[node.id] = true;
  for (const ch of node.children || []) defaultExpanded(ch, maxDepth, depth + 1, out);
  return out;
}

export function basename(p: string): string {
  const parts = p.split("/");
  return parts[parts.length - 1] || p;
}

/** Depth-first walk in display order, so "the first markdown file" is predictable. */
export function collectFiles(node: TreeNode | null, out: string[] = []): string[] {
  if (!node) return out;
  if (node.path) out.push(node.path);
  for (const ch of node.children || []) collectFiles(ch, out);
  return out;
}

/** quickstart.md → README.md → first markdown file → first file of any kind. */
export function defaultEntry(tree: TreeNode | null): string | null {
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
