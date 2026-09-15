// Deep link into the reader for a Wiki page (path relative to the atlas root).
export function readerHref(pagePath: string): string {
  return `/reader?p=${encodeURIComponent(pagePath)}`;
}
