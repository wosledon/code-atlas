import { useEffect, useMemo, useState } from "react";
import mermaid from "mermaid";
import {
  cleanupMermaidArtifacts,
  mermaidErrorBrief,
  repairMermaid,
} from "../../lib/mermaid";

export function useDiagramRender(code: string) {
  const [svg, setSvg] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [repaired, setRepaired] = useState(false);
  const id = useMemo(() => `mmd-${Math.random().toString(36).slice(2, 9)}`, []);

  useEffect(() => {
    let cancelled = false;
    const show = (markup: string, wasRepaired: boolean) => {
      if (cancelled) return;
      setSvg(markup);
      setErr(null);
      setRepaired(wasRepaired);
    };
    (async () => {
      let failure: unknown;
      try {
        show((await mermaid.render(id, code)).svg, false);
        return;
      } catch (e) {
        failure = e;
      }
      const fix = repairMermaid(code);
      if (fix.edits > 0) {
        try {
          show((await mermaid.render(`${id}r`, fix.code)).svg, true);
          cleanupMermaidArtifacts(id);
          return;
        } catch {
          cleanupMermaidArtifacts(`${id}r`);
        }
      }
      cleanupMermaidArtifacts(id);
      if (cancelled) return;
      setSvg("");
      setRepaired(false);
      setErr(mermaidErrorBrief(failure));
    })();
    return () => {
      cancelled = true;
    };
  }, [code, id]);

  return { svg, err, repaired };
}
