import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight, FileCode2, FolderTree } from "lucide-react";
import type { SitemapFileRow } from "../generated/SitemapFileRow";
import type { SitemapFiles as Files } from "../generated/SitemapFiles";
import type { Transport } from "../transport";
import { Badge, cn, EmptyState, Notice, type Tone } from "../components/ui";

const SOURCES: Record<string, string> = {
  robots: "robots.txt",
  default: "Default location",
  user: "Added in settings",
  local: "On this computer",
  list: "URL list",
};

const READING: Record<string, string> = { invalid_xml: "Invalid XML", not_a_sitemap: "Not a sitemap", compression: "Bad compression" };

/** The file's state as a label and tone (never colour alone). */
export function fileStatus(f: SitemapFileRow): { label: string; tone: Tone } {
  if (f.error_class === "too_deep") return { label: "Nested too deep", tone: "warn" };
  if (f.error_class && READING[f.error_class]) return { label: READING[f.error_class], tone: "bad" };
  if (f.error_class || f.error) {
    const http = f.http_status && f.http_status >= 400 ? ` ${f.http_status}` : "";
    return { label: `Not read${http}`, tone: "bad" };
  }
  if (f.over_limit) return { label: "Over the limit", tone: "warn" };
  if (f.invalid_entries > 0) return { label: "Invalid entries", tone: "warn" };
  return { label: f.http_status ? `OK ${f.http_status}` : "OK", tone: "ok" };
}

const size = (b: number | null) =>
  b === null ? "" : b < 1024 ? `${b} B` : b < 1024 * 1024 ? `${(b / 1024).toFixed(1)} KB` : `${(b / 1024 / 1024).toFixed(1)} MB`;

/** Visible rows: a collapsed index hides everything under it. */
function visible(files: SitemapFileRow[], collapsed: Set<number>) {
  const out: SitemapFileRow[] = [];
  let hideBelow: number | null = null;
  for (const f of files) {
    if (hideBelow !== null) {
      if (f.level > hideBelow) continue;
      hideBelow = null;
    }
    out.push(f);
    if (f.children > 0 && collapsed.has(f.id)) hideBelow = f.level;
  }
  return out;
}

/**
 * Sitemap files (spec 011 US3): every sitemap the crawl read, indexes with
 * the files they list, and what reading each one found. Refreshes while the
 * crawl runs.
 */
export function SitemapFilesView({ transport, path, live }: { transport: Transport; path: string; live: boolean }) {
  const [data, setData] = useState<Files | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [focus, setFocus] = useState<number | null>(null);
  const treeRef = useRef<HTMLUListElement>(null);

  const load = useCallback(() => {
    transport.sitemapFiles(path).then(
      (d) => (setData(d), setError(null)),
      (e) => setError(typeof e === "string" ? e : String(e)),
    );
  }, [transport, path]);
  useEffect(() => {
    load();
    if (!live) return;
    const t = setInterval(load, 2000);
    return () => clearInterval(t);
  }, [load, live]);

  const rows = useMemo(() => (data ? visible(data.files, collapsed) : []), [data, collapsed]);
  const current = rows.find((r) => r.id === focus) ?? rows[0] ?? null;
  const toggle = (id: number, open?: boolean) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (open ?? n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  useEffect(() => {
    if (current && treeRef.current?.contains(document.activeElement)) {
      treeRef.current.querySelector<HTMLElement>(`[data-id="${current.id}"]`)?.focus();
    }
  }, [current]);

  const onKey = (e: React.KeyboardEvent) => {
    if (!current) return;
    const i = rows.indexOf(current);
    const go = (r: SitemapFileRow | undefined) => r && (e.preventDefault(), setFocus(r.id));
    switch (e.key) {
      case "ArrowDown":
        return go(rows[i + 1]);
      case "ArrowUp":
        return go(rows[i - 1]);
      case "Home":
        return go(rows[0]);
      case "End":
        return go(rows[rows.length - 1]);
      case "ArrowRight":
        if (current.children > 0) {
          e.preventDefault();
          if (collapsed.has(current.id)) toggle(current.id, true);
          else go(rows[i + 1]);
        }
        return;
      case "ArrowLeft":
        e.preventDefault();
        if (current.children > 0 && !collapsed.has(current.id)) toggle(current.id, false);
        else go(rows.find((r) => r.id === current.parent_id));
        return;
      case "Enter":
      case " ":
        if (current.children > 0) (e.preventDefault(), toggle(current.id));
    }
  };

  if (error) return <EmptyState title="Could not read the sitemap files">{error}</EmptyState>;
  if (!data) return null;
  if (data.files.length === 0) {
    return (
      <EmptyState icon={<FolderTree className="size-6" />} title="No sitemap files">
        {live ? "Sitemaps are read while the crawl runs." : "This crawl read no sitemaps. Turn on Read sitemaps in Settings › Scope, or add one there."}
      </EmptyState>
    );
  }
  return (
    <section aria-label="Sitemap files" className="flex h-full min-h-0 flex-col">
      <header className="flex items-center gap-3 border-b border-line px-4 py-2">
        <h2 className="m-0 text-base font-semibold">Sitemap files</h2>
        <span className="text-sm text-fg-3 tabular">{data.files.length.toLocaleString()} files</span>
      </header>
      {!data.available && (
        <Notice className="px-4 py-2">This crawl file was made before SpiderLens kept sitemap entries, so only the summary of each file is shown.</Notice>
      )}
      <ul ref={treeRef} role="tree" aria-label="Sitemap files" className="m-0 min-h-0 flex-1 list-none overflow-y-auto p-1" onKeyDown={onKey}>
        {rows.map((f) => {
          const st = fileStatus(f);
          const on = current?.id === f.id;
          const open = f.children > 0 ? !collapsed.has(f.id) : undefined;
          return (
            <li
              key={f.id}
              data-id={f.id}
              role="treeitem"
              aria-level={f.level + 1}
              aria-expanded={open}
              aria-selected={on}
              tabIndex={on ? 0 : -1}
              onClick={() => setFocus(f.id)}
              onDoubleClick={() => f.children > 0 && toggle(f.id)}
              className={cn("flex h-7 cursor-default items-center gap-2 rounded-md pr-2 text-base outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset", on ? "bg-selection" : "hover:bg-surface-2")}
              style={{ paddingLeft: 8 + f.level * 16 }}
            >
              {f.children > 0 ? (
                <button
                  type="button"
                  tabIndex={-1}
                  aria-label={open ? "Collapse" : "Expand"}
                  onClick={(e) => (e.stopPropagation(), toggle(f.id))}
                  className="flex size-4 items-center justify-center text-fg-3"
                >
                  <ChevronRight aria-hidden className={cn("size-3.5 transition-transform duration-(--dur-fast)", open && "rotate-90")} />
                </button>
              ) : (
                <FileCode2 aria-hidden className="size-4 shrink-0 text-fg-3" />
              )}
              <span className="min-w-0 flex-1 truncate font-mono text-sm" title={f.location}>
                {f.location}
              </span>
              {f.source && <span className="shrink-0 text-xs text-fg-3">{SOURCES[f.source] ?? f.source}</span>}
              <span className="w-24 shrink-0 text-right text-sm text-fg-2 tabular">{f.entries.toLocaleString()} {f.kind === "index" ? "files" : "URLs"}</span>
              <Badge tone={st.tone} className="shrink-0">
                {st.label}
              </Badge>
            </li>
          );
        })}
      </ul>
      {current && <FileFacts f={current} />}
    </section>
  );
}

function FileFacts({ f }: { f: SitemapFileRow }) {
  const facts: [string, string][] = [];
  if (f.source) facts.push(["Found via", SOURCES[f.source] ?? f.source]);
  if (f.base_url) facts.push(["Published at", f.base_url]);
  if (f.final_url && f.final_url !== f.location) facts.push([f.redirected ? "Redirected to" : "Read from", f.final_url]);
  if (f.kind) facts.push(["Type", f.kind === "index" ? "Sitemap index" : "URL sitemap"]);
  if (f.bytes !== null) facts.push(["Size", `${size(f.bytes)}${f.compressed ? ", gzip" : ""}`]);
  if (f.invalid_entries > 0) facts.push(["Invalid entries", f.invalid_entries.toLocaleString()]);
  return (
    <div aria-label="Selected sitemap file" role="region" className="flex flex-col gap-1.5 border-t border-line px-4 py-3 text-sm">
      <dl className="m-0 flex flex-col gap-1">
        {facts.map(([k, v]) => (
          <div key={k} className="flex gap-4">
            <dt className="w-28 shrink-0 text-fg-3">{k}</dt>
            <dd className="m-0 min-w-0 truncate" title={v}>
              {v}
            </dd>
          </div>
        ))}
      </dl>
      {f.over_limit && <Notice kind="warn">More than 50,000 entries or 50 MB: only the entries up to the limit were read.</Notice>}
      {f.error && (
        <Notice kind="bad">
          {f.error}
          {f.error_line !== null && ` (line ${f.error_line})`}
        </Notice>
      )}
    </div>
  );
}
