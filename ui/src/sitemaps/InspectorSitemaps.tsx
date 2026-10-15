import { useEffect, useState } from "react";
import type { UrlSitemapEntry } from "../generated/UrlSitemapEntry";
import type { Transport } from "../transport";

/** The inspector's Sitemaps section (spec 011 FR-026): every sitemap that
 * lists the selected URL, with the lastmod it gives. */
export function useSitemapEntries(transport: Transport, path: string, id: number | null) {
  const [entries, setEntries] = useState<UrlSitemapEntry[]>([]);
  useEffect(() => {
    if (id === null) return setEntries([]);
    let gone = false;
    transport.sitemapEntriesForUrl(path, id).then(
      (e) => !gone && setEntries(e),
      () => !gone && setEntries([]),
    );
    return () => {
      gone = true;
    };
  }, [transport, path, id]);
  return entries;
}

export function SitemapEntryList({ entries }: { entries: UrlSitemapEntry[] }) {
  if (entries.length === 0) return <p className="m-0 text-sm text-fg-3">Not listed in any sitemap the crawl read.</p>;
  return (
    <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm" aria-label="Sitemaps listing this URL">
      {entries.map((e, i) => (
        <li key={`${e.file_id}-${i}`} className="flex min-w-0 items-baseline gap-2">
          <span className="min-w-0 truncate" title={e.location}>
            {e.location}
          </span>
          <span className="shrink-0 tabular text-fg-3">{e.lastmod_raw ? `lastmod ${e.lastmod_raw}${e.lastmod === null ? " (not a valid date)" : ""}` : "no lastmod"}</span>
        </li>
      ))}
    </ul>
  );
}
