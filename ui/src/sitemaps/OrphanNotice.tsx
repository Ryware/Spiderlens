import { useEffect, useState } from "react";
import type { OrphanSummary } from "../generated/OrphanSummary";
import type { Transport } from "../transport";

/** The line above the Sitemaps tab (spec 011 FR-012, FR-013, FR-015): how
 * many orphans the crawl found, and whether that count can be trusted. */
export function orphanText(s: OrphanSummary): string {
  if (s.list_mode || !s.available) return s.reason ?? "Orphan pages are found when the crawl ends.";
  const n = (v: number, one: string, many: string) => `${v.toLocaleString()} ${v === 1 ? one : many}`;
  let text = `${n(s.orphans, "orphan page", "orphan pages")}, ${n(s.nofollow_only, "page", "pages")} linked only through nofollow.`;
  if (s.provisional) text += " The crawl is still running, so these counts can change.";
  else if (s.incomplete)
    text += ` The crawl did not reach every page (${n(s.not_crawled, "URL", "URLs")} not crawled), so some of these may be linked from pages it did not fetch.`;
  return text;
}

export function OrphanNotice({ transport, path, live }: { transport: Transport; path: string; live: boolean }) {
  const [summary, setSummary] = useState<OrphanSummary | null>(null);
  useEffect(() => {
    let gone = false;
    transport.orphanSummary(path).then(
      (s) => !gone && setSummary(s),
      () => !gone && setSummary(null),
    );
    return () => {
      gone = true;
    };
  }, [transport, path, live]);
  if (!summary) return null;
  return (
    <p className="m-0 text-sm text-fg-2" role="status" aria-label="Orphan pages">
      {orphanText(summary)}
    </p>
  );
}
