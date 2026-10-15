// Both crawls' details for the selected URL, side by side (spec 012 FR-021).

import { useEffect, useState } from "react";
import { MousePointerClick } from "lucide-react";
import type { PairDetail } from "../generated/PairDetail";
import type { UrlDetail } from "../generated/UrlDetail";
import type { Transport } from "../transport";
import { cn, EmptyState, Notice } from "../components/ui";
import { formatBytes } from "../results/format";
import type { Pair } from "./CompareView";

const FIELDS: [string, (d: UrlDetail) => string][] = [
  ["Address", (d) => d.url],
  ["State", (d) => d.final_state],
  ["Status", (d) => (d.status === null ? "" : `${d.status} ${d.status_text ?? ""}`.trim())],
  ["Indexability", (d) => d.indexability ?? ""],
  ["Content type", (d) => d.content_type ?? ""],
  ["Depth", (d) => String(d.depth)],
  ["Inlinks", (d) => d.inlinks.toLocaleString()],
  ["Outlinks", (d) => d.outlinks.toLocaleString()],
  ["Size", (d) => (d.size === null ? "" : formatBytes(d.size))],
  ["Response time", (d) => (d.response_ms === null ? "" : `${d.response_ms.toLocaleString()} ms`)],
  ["Redirects to", (d) => d.redirects.at(-1)?.url ?? ""],
  ["In sitemap", (d) => (d.in_sitemap ? "Yes" : "No")],
];

export function PairInspector({ transport, comparison, pair }: { transport: Transport; comparison: number | null; pair: Pair | null }) {
  const [detail, setDetail] = useState<PairDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setError(null);
    if (comparison === null || !pair) return;
    let off = false;
    transport.compareDetail(comparison, pair.prev, pair.cur).then(
      (d) => !off && setDetail(d),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, comparison, pair]);

  if (!pair) {
    return (
      <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
        <EmptyState icon={<MousePointerClick className="size-6" />} title="Nothing selected">
          Select a URL in a comparison list to see both crawls side by side.
        </EmptyState>
      </aside>
    );
  }
  return (
    <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
      {error && (
        <Notice kind="bad" role="alert" className="p-3">
          {error}
        </Notice>
      )}
      {detail && (
        <table aria-label="Previous and current" className="w-full border-collapse text-sm" data-selectable>
          <thead>
            <tr className="border-b border-line text-left text-xs text-fg-3">
              <th className="px-3 py-1.5 font-semibold" />
              <th className="px-2 py-1.5 font-semibold">Previous</th>
              <th className="px-2 py-1.5 font-semibold">Current</th>
            </tr>
          </thead>
          <tbody>
            {FIELDS.map(([label, get]) => {
              const a = detail.previous ? get(detail.previous) : null;
              const b = detail.current ? get(detail.current) : null;
              const changed = a !== null && b !== null && a !== b;
              return (
                <tr key={label} className="border-b border-line/60 align-top" data-changed={changed || undefined}>
                  <th scope="row" className="px-3 py-1 text-left font-normal text-fg-3">
                    {label}
                  </th>
                  <td className="px-2 py-1 break-all">{a ?? <span className="text-fg-3">Not in this crawl</span>}</td>
                  <td className={cn("px-2 py-1 break-all", changed && "font-semibold text-warn")}>{b ?? <span className="text-fg-3">Not in this crawl</span>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </aside>
  );
}
