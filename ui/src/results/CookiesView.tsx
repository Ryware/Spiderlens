// The cookie audit (spec 025 US5): every cookie the crawl saw, without its
// value, and the URLs that set the selected one.

import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, Cookie } from "lucide-react";
import type { CookieFilter } from "../generated/CookieFilter";
import type { CookiePage } from "../generated/CookiePage";
import type { CookieRow } from "../generated/CookieRow";
import type { CookieUrlPage } from "../generated/CookieUrlPage";
import type { Features } from "../generated/Features";
import type { Transport } from "../transport";
import { Badge, Button, Chip, cn, EmptyState } from "../components/ui";
import { needReason } from "./format";

const PAGE = 500;
const URL_PAGE = 100;

export const cookieFilters: { id: CookieFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "session", label: "Session" },
  { id: "persistent", label: "Persistent" },
  { id: "third_party", label: "Third party" },
  { id: "missing_secure", label: "Missing Secure" },
  { id: "missing_http_only", label: "Missing HttpOnly" },
  { id: "same_site_none", label: "SameSite=None" },
];

const columns: { id: keyof CookieRow; label: string; numeric?: boolean }[] = [
  { id: "name", label: "Name" },
  { id: "domain", label: "Domain" },
  { id: "path", label: "Path" },
  { id: "expires", label: "Expires" },
  { id: "same_site", label: "SameSite" },
  { id: "size", label: "Size", numeric: true },
  { id: "urls", label: "URLs", numeric: true },
];

const keyOf = (c: CookieRow) => `${c.name}\n${c.domain}\n${c.path}`;

/** Secure, HttpOnly and party as small badges: what a consent review looks for. */
export function CookieFlags({ c }: { c: CookieRow }) {
  return (
    <span className="inline-flex flex-wrap gap-1">
      <Badge tone={c.secure ? "neutral" : "warn"}>{c.secure ? "Secure" : "Not Secure"}</Badge>
      <Badge tone={c.http_only ? "neutral" : "warn"}>{c.http_only ? "HttpOnly" : "Script readable"}</Badge>
      {c.third_party && <Badge tone="info">Third party</Badge>}
    </span>
  );
}

export const expiresText = (c: CookieRow) => (c.session || c.expires === null ? "Session" : new Date(c.expires).toISOString().slice(0, 10));

function CookieUrls({ transport, path, cookie, onFocusId }: { transport: Transport; path: string; cookie: CookieRow; onFocusId: (id: number) => void }) {
  const [page, setPage] = useState<CookieUrlPage | null>(null);
  useEffect(() => {
    let off = false;
    setPage(null);
    transport.cookieUrls(path, cookie.name, cookie.domain, cookie.path, 0, URL_PAGE).then((p) => !off && setPage(p));
    return () => {
      off = true;
    };
  }, [transport, path, cookie.name, cookie.domain, cookie.path]);
  const more = () => page && transport.cookieUrls(path, cookie.name, cookie.domain, cookie.path, page.rows.length, URL_PAGE).then((p) => setPage({ total: p.total, rows: [...page.rows, ...p.rows] }));

  return (
    <section aria-label={`URLs that set ${cookie.name}`} className="flex min-h-0 flex-col border-t border-line">
      <div className="flex items-center gap-2 px-3 py-2">
        <h3 className="m-0 text-xs font-semibold text-fg-3">
          URLs that set <span className="font-mono text-fg-2">{cookie.name}</span>
          {page && ` (${page.total.toLocaleString()})`}
        </h3>
        <span className="ml-auto">
          <CookieFlags c={cookie} />
        </span>
      </div>
      <ul className="m-0 min-h-0 flex-1 list-none overflow-auto px-1 pb-2" data-selectable>
        {page?.rows.map((u) => (
          <li key={u.id}>
            <button type="button" className="flex w-full gap-2 rounded-sm px-2 py-0.5 text-left text-sm hover:bg-surface-2" onClick={() => onFocusId(u.id)}>
              <span className="tabular w-8 shrink-0 text-fg-3">{u.status ?? "–"}</span>
              <span className="min-w-0 truncate font-mono text-fg-2">{u.url}</span>
            </button>
          </li>
        ))}
      </ul>
      {page && page.rows.length < page.total && (
        <div className="px-3 pb-2">
          <Button size="sm" onClick={() => void more()}>
            Show more
          </Button>
        </div>
      )}
    </section>
  );
}

export function CookiesView({
  transport,
  path,
  features,
  live,
  onFocusId,
}: {
  transport: Transport;
  path: string;
  features: Features | null;
  live: boolean;
  onFocusId: (id: number) => void;
}) {
  const [filter, setFilter] = useState<CookieFilter>("all");
  const [sort, setSort] = useState<{ column: keyof CookieRow; desc: boolean } | null>(null);
  const [page, setPage] = useState<CookiePage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const available = features?.available.includes("cookies") ?? false;

  useEffect(() => {
    if (!available) return;
    let off = false;
    const load = () =>
      transport.cookiesList(path, filter, sort?.column ?? null, sort?.desc ?? true, 0, PAGE).then(
        (p) => !off && (setPage(p), setError(null)),
        (e) => !off && setError(String(e)),
      );
    void load();
    // A running crawl keeps finding cookies.
    const timer = live ? setInterval(load, 2000) : undefined;
    return () => {
      off = true;
      clearInterval(timer);
    };
  }, [transport, path, filter, sort, available, live]);

  if (!features) return null;
  if (!available)
    return (
      <EmptyState icon={<Cookie className="size-6" />} title="No cookie data">
        {needReason.cookies}
      </EmptyState>
    );

  const more = () =>
    page && transport.cookiesList(path, filter, sort?.column ?? null, sort?.desc ?? true, page.rows.length, PAGE).then((p) => setPage({ ...p, rows: [...page.rows, ...p.rows] }));
  const chosen = page?.rows.find((c) => keyOf(c) === selected) ?? null;
  const sortBy = (column: keyof CookieRow) => setSort((s) => (s?.column === column ? { column, desc: !s.desc } : { column, desc: column === "urls" || column === "size" }));

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <h2 className="m-0 shrink-0 text-base font-semibold text-fg">Cookies</h2>
        <div role="toolbar" aria-label="Cookie filters" className="flex min-w-0 flex-wrap items-center gap-0.5">
          {cookieFilters.map((f) => (
            <Chip key={f.id} label={f.label} count={page?.counts[f.id] ?? null} active={f.id === filter} onClick={() => setFilter(f.id)} data-filter={f.id} />
          ))}
        </div>
      </div>
      {error ? (
        <EmptyState title="Could not read the cookies">{error}</EmptyState>
      ) : page && page.total === 0 ? (
        <EmptyState icon={<Cookie className="size-6" />} title={filter === "all" ? "No cookies were set" : `No ${cookieFilters.find((f) => f.id === filter)!.label} cookies`}>
          {live ? "They appear here as the crawl finds them." : filter === "all" ? "No page in this crawl set a cookie." : "Try another filter."}
        </EmptyState>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-sm" aria-label={`Cookies: ${cookieFilters.find((f) => f.id === filter)!.label}`}>
            <thead className="sticky top-0 bg-chrome">
              <tr>
                {columns.map((c) => {
                  const on = sort?.column === c.id;
                  return (
                    <th key={c.id} scope="col" aria-sort={on ? (sort!.desc ? "descending" : "ascending") : undefined} className={cn("border-b border-line-strong px-2 py-1 font-medium text-fg-2", c.numeric ? "text-right" : "text-left")}>
                      <button type="button" className="inline-flex items-center gap-1" onClick={() => sortBy(c.id)}>
                        {c.label}
                        {on && (sort!.desc ? <ArrowDown aria-hidden className="size-3 text-accent-text" /> : <ArrowUp aria-hidden className="size-3 text-accent-text" />)}
                      </button>
                    </th>
                  );
                })}
                <th scope="col" className="border-b border-line-strong px-2 py-1 text-left font-medium text-fg-2">
                  Flags
                </th>
              </tr>
            </thead>
            <tbody data-selectable>
              {page?.rows.map((c) => {
                const k = keyOf(c);
                return (
                  <tr
                    key={k}
                    aria-selected={k === selected}
                    tabIndex={0}
                    onClick={() => setSelected(k)}
                    onKeyDown={(e) => e.key === "Enter" && setSelected(k)}
                    className={cn("cursor-default border-b border-line", k === selected ? "bg-selection" : "hover:bg-surface-2")}
                  >
                    <td className="px-2 py-1 font-mono text-fg">{c.name}</td>
                    <td className="px-2 py-1 text-fg-2">{c.domain}</td>
                    <td className="px-2 py-1 font-mono text-fg-2">{c.path}</td>
                    <td className="px-2 py-1 text-fg-2">{expiresText(c)}</td>
                    <td className="px-2 py-1 text-fg-2">{c.same_site ?? "–"}</td>
                    <td className="tabular px-2 py-1 text-right text-fg-2">{c.size.toLocaleString()}</td>
                    <td className="tabular px-2 py-1 text-right text-fg-2">{c.urls.toLocaleString()}</td>
                    <td className="px-2 py-1">
                      <CookieFlags c={c} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {page && page.rows.length < page.total && (
            <div className="p-2">
              <Button size="sm" onClick={() => void more()}>
                Show more
              </Button>
            </div>
          )}
        </div>
      )}
      {chosen && (
        <div className="flex h-2/5 min-h-32 flex-col">
          <CookieUrls transport={transport} path={path} cookie={chosen} onFocusId={onFocusId} />
        </div>
      )}
    </div>
  );
}
