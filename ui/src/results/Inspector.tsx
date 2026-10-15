// The detail pane for the selected URL (US3, FR-019..FR-025).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Copy, CornerDownRight, FileCode, MousePointerClick, TableProperties } from "lucide-react";
import type { LinkMetrics } from "../generated/LinkMetrics";
import type { ListKind } from "../generated/ListKind";
import type { ListPage } from "../generated/ListPage";
import type { UrlDetail } from "../generated/UrlDetail";
import type { Transport } from "../transport";
import { Badge, Bar, Button, cn, ContextMenu, EmptyState, Notice, Segmented, Select, type Tone } from "../components/ui";
import { AiPageSection } from "./AiPageSection";
import { CookieFlags, expiresText } from "./CookiesView";
import { cellTone, formatBytes, toneText } from "./format";
import { HtmlViewer } from "./HtmlViewer";

const LIST_PAGE = 100;

const stateTone = (d: UrlDetail): Tone => {
  if (d.error_class || d.final_state.startsWith("Failed")) return "bad";
  const t = cellTone("status", d.status);
  if (t) return t;
  return d.not_crawled_reason ? "warn" : "neutral";
};

const fieldLabels: Record<string, string> = {
  indexability: "Indexability",
  headers: "Response headers",
  headings: "Headings",
  anchor_text: "Anchor and alt text",
  size: "Size",
  response_time: "Response time",
  attempts: "Retry attempts",
  raw_html: "Page HTML",
  link_metrics: "Link Score",
  cookies: "Cookies",
};

function Section({ title, children, aside }: { title: string; children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5 border-b border-line px-3 py-3">
      <div className="flex items-center gap-2">
        <h3 className="m-0 text-xs font-semibold text-fg-3">{title}</h3>
        {aside && <div className="ml-auto">{aside}</div>}
      </div>
      {children}
    </section>
  );
}

/** Link Score and link counts (spec 023 FR-008). */
function LinkScoreSection({ m }: { m: LinkMetrics }) {
  const n = (v: number) => v.toLocaleString();
  return (
    <Section title="Link Score">
      <div className="flex items-center gap-3">
        <span className="text-xl font-semibold tabular text-fg">{m.score.toFixed(1)}</span>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <Bar value={m.score / 100} tone="accent" label="Link Score" className="w-full" />
          <span className="text-xs text-fg-3">
            Rank {n(m.rank)} of {n(m.of)} internal URLs
          </span>
        </div>
      </div>
      <Facts
        rows={[
          ["Unique inlinks", n(m.unique_inlinks)],
          ["Followed inlinks", n(m.followed_inlinks)],
          ["All inlinks", n(m.all_inlinks)],
          ["Unique outlinks", n(m.unique_outlinks)],
          ["External outlinks", n(m.external_outlinks)],
          ["Pages linking", `${m.pct_linking.toFixed(1)}%`],
        ]}
      />
    </Section>
  );
}

function Facts({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="m-0 grid grid-cols-3 gap-x-3 gap-y-1 text-sm" data-selectable>
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-fg-3">{k}</dt>
          <dd className="col-span-2 m-0 min-w-0 break-words text-fg">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/** What a URL row in the inspector's lists can do (T027). */
interface RowActions {
  copy: (url: string) => void;
  /** Absent when there is no table to show the URL in. */
  show?: (id: number) => void;
}

/**
 * A list row naming one URL, with Copy URL and Show in table: buttons that
 * appear on hover or keyboard focus, and the same actions on right-click.
 */
function UrlRow({ actions, id, url, className, children }: { actions: RowActions; id: number; url: string; className?: string; children: ReactNode }) {
  const { copy, show } = actions;
  return (
    <ContextMenu
      items={[
        { label: "Copy URL", onSelect: () => copy(url) },
        ...(show ? [{ label: "Show in table", onSelect: () => show(id) }] : []),
      ]}
    >
      <li className={cn("group", className)}>
        {children}
        <span className="-my-1 flex shrink-0 gap-0.5 self-center opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
          <Button size="sm" variant="quiet" icon aria-label={`Copy ${url}`} title="Copy URL" onClick={() => copy(url)}>
            <Copy aria-hidden className="size-3.5" />
          </Button>
          {show && (
            <Button size="sm" variant="quiet" icon aria-label={`Show ${url} in table`} title="Show in table" onClick={() => show(id)}>
              <TableProperties aria-hidden className="size-3.5" />
            </Button>
          )}
        </span>
      </li>
    </ContextMenu>
  );
}

/** Anchor text, or for images whether the alt text is there. */
function LinkText({ kind, text }: { kind: ListKind; text: string | null }) {
  if (kind === "images") {
    if (text === null) return <span className="shrink-0 text-xs text-warn">No alt attribute</span>;
    if (text === "") return <span className="shrink-0 text-xs text-warn">Empty alt</span>;
  }
  if (!text) return null;
  return (
    <span className="max-w-2/5 shrink-0 truncate text-fg-3" title={text}>
      {text}
    </span>
  );
}

/** Link positions (spec 023 US4), as the lists show them. */
const positionLabel: Record<string, string> = {
  navigation: "Navigation",
  header: "Header",
  content: "Content",
  sidebar: "Sidebar",
  footer: "Footer",
  unknown: "Position unknown",
};

/** Filter by position on the Inlinks and Outlinks lists (FR-024). */
function PositionFilter({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <Select aria-label="Link position" className="mb-1 w-44 self-start" value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">All positions</option>
      {Object.entries(positionLabel).map(([v, label]) => (
        <option key={v} value={v}>
          {v === "unknown" ? "Unknown" : label}
        </option>
      ))}
    </Select>
  );
}

function LinkList({ transport, path, id, kind, total, actions }: { transport: Transport; path: string; id: number; kind: ListKind; total: number; actions: RowActions }) {
  const [page, setPage] = useState<ListPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [position, setPosition] = useState("");
  // Positions exist once Crawl Analysis has worked them out; the filter
  // shows from then on, even when it leaves the list empty.
  // Keyed by URL and list, so both reset when either changes.
  const [positioned, setPositioned] = useState(false);
  useEffect(() => {
    let off = false;
    setPage(null);
    setError(null);
    transport.urlList(path, id, kind, 0, LIST_PAGE, position || null).then(
      (p) => {
        if (off) return;
        setPage(p);
        if (p.rows.some((r) => r.position !== null)) setPositioned(true);
      },
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, kind, position]);
  const more = () =>
    page &&
    transport.urlList(path, id, kind, page.rows.length, LIST_PAGE, position || null).then(
      (p) => setPage({ ...p, from: 0, rows: [...page.rows, ...p.rows], headings: [...page.headings, ...p.headings] }),
      (e) => setError(String(e)),
    );
  const filter = positioned && (kind === "inlinks" || kind === "outlinks") && <PositionFilter value={position} onChange={setPosition} />;
  if (error) return <Notice kind="bad">{error}</Notice>;
  if (!page) return <div className="text-sm text-fg-3">Loading…</div>;
  if (page.unavailable) return <div className="text-sm text-fg-3">{page.unavailable}</div>;
  const shown = kind === "headings" ? page.headings.length : page.rows.length;
  // With a position chosen, the list's own total is the filtered count.
  const all = position ? page.total : total;
  if (shown === 0)
    return (
      <div className="flex flex-col">
        {filter}
        <div className="text-sm text-fg-3">{position ? "No links at this position." : "None."}</div>
      </div>
    );
  return (
    <div className="flex flex-col">
      {filter}
      {kind === "headings" ? (
        <ol className="m-0 flex list-none flex-col p-0" data-selectable>
          {page.headings.map((h, i) => (
            <li key={i} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className="w-6 shrink-0 text-xs font-medium text-fg-3">H{h.level}</span>
              <span className={cn("min-w-0 flex-1 break-words", h.level === 1 ? "text-fg" : "text-fg-2")}>{h.text || <span className="text-fg-3">(empty)</span>}</span>
            </li>
          ))}
        </ol>
      ) : (
      <ul className="m-0 flex list-none flex-col p-0" data-selectable>
        {page.rows.map((r, i) => {
          const tone = cellTone("status", r.status);
          return (
            <UrlRow key={`${r.id}-${i}`} actions={actions} id={r.id} url={r.url} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className={cn("w-8 shrink-0 text-right tabular", tone ? toneText[tone] : "text-fg-3")}>{r.status ?? "–"}</span>
              <span className="min-w-0 flex-1 truncate" title={r.url}>
                {r.url}
              </span>
              <LinkText kind={kind} text={r.text} />
              {r.position && (
                <span className="shrink-0 text-xs text-fg-3" title="Where the link sits on its page">
                  {positionLabel[r.position] ?? r.position}
                </span>
              )}
              {(r.nofollow || !r.followed || r.sponsored || r.ugc) && (
                <span className="shrink-0 text-xs text-fg-3">
                  {[r.nofollow ? "nofollow" : !r.followed ? "not followed" : null, r.sponsored && "sponsored", r.ugc && "ugc"].filter(Boolean).join(", ")}
                </span>
              )}
            </UrlRow>
          );
        })}
      </ul>
      )}
      {shown < all && (
        <Button size="sm" variant="quiet" className="mt-1 self-start" onClick={() => void more()}>
          Show more ({(all - shown).toLocaleString()} left)
        </Button>
      )}
    </div>
  );
}

export function Inspector({
  transport,
  path,
  id,
  onAnchorCloud,
  onShowInTable,
  aiChecks = false,
}: {
  transport: Transport;
  path: string | null;
  id: number | null;
  /** Opens the anchor text cloud of this URL's inlinks (spec 022 US4). */
  onAnchorCloud?: () => void;
  /** Selects the URL in a results tab; false when no tab lists it. */
  onShowInTable?: (id: number) => Promise<boolean>;
  aiChecks?: boolean;
}) {
  const [detail, setDetail] = useState<UrlDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [list, setList] = useState<ListKind>("inlinks");
  const [htmlOpen, setHtmlOpen] = useState(false);
  const [status, setStatus] = useState<{ kind: "info" | "warn" | "bad"; text: string } | null>(null);
  const statusTimer = useRef<ReturnType<typeof setTimeout>>();
  const say = (kind: "info" | "warn" | "bad", text: string) => {
    clearTimeout(statusTimer.current);
    setStatus({ kind, text });
    statusTimer.current = setTimeout(() => setStatus(null), 4000);
  };
  useEffect(() => () => clearTimeout(statusTimer.current), []);
  const actions: RowActions = {
    copy: (url) =>
      void transport.copyText(url).then(
        () => say("info", "Copied."),
        (e) => say("bad", String(e)),
      ),
    show: onShowInTable
      ? (target) =>
          void onShowInTable(target).then(
            (found) => !found && say("warn", "That URL is not in any results table of this crawl."),
            (e) => say("bad", String(e)),
          )
      : undefined,
  };

  useEffect(() => {
    if (!path || id === null) {
      setDetail(null);
      return;
    }
    let off = false;
    setError(null);
    transport.urlDetail(path, id).then(
      (d) => !off && setDetail(d),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id]);

  if (!path || id === null) {
    return (
      <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
        <EmptyState icon={<MousePointerClick className="size-6" />} title="Nothing selected">
          Select a URL in the results to see its response, redirects and links here.
        </EmptyState>
      </aside>
    );
  }
  if (error) {
    return (
      <aside aria-label="Details" className="h-full overflow-y-auto bg-surface p-3">
        <Notice kind="bad">{error}</Notice>
      </aside>
    );
  }
  if (!detail || detail.id !== id) return <aside aria-label="Details" className="h-full bg-surface" aria-busy />;

  const d = detail;
  const tone = stateTone(d);
  const facts: [string, ReactNode][] = [
    ["Status", d.status !== null ? `${d.status}${d.status_text ? ` ${d.status_text}` : ""}` : "No response"],
    ["Content type", d.content_type ?? "–"],
    ["Depth", d.depth.toLocaleString()],
    ["Found on", d.found_on ? <span className="break-all">{d.found_on}</span> : "Start URL"],
    ["Discovered via", d.discovered_via],
    ["In sitemap", d.in_sitemap ? "Yes" : "No"],
  ];
  if (d.size !== null) facts.push(["Size", formatBytes(d.size)]);
  if (d.response_ms !== null) facts.push(["Response time", `${d.response_ms.toLocaleString()} ms`]);
  if (d.attempts !== null && (d.attempts > 1 || d.manual_retry)) {
    facts.push(["Attempts", `${d.attempts}${d.manual_retry ? ", last by Retry failed URLs" : ""}`]);
  }
  if (d.indexability) {
    const non = d.indexability.startsWith("Non-indexable");
    facts.push(["Indexability", <span className={non ? "text-warn" : undefined}>{d.indexability}</span>]);
  }
  if (d.fetched_at !== null) facts.push(["Fetched", new Date(d.fetched_at).toLocaleString()]);
  const counts: Record<ListKind, number> = { inlinks: d.inlinks, outlinks: d.outlinks, images: d.images, headings: d.headings };
  // Fields the file has no data for, grouped by why. Page HTML has its own
  // section, which says how to get it.
  const missing = new Map<string, string[]>();
  for (const [k, why] of Object.entries(d.unavailable)) if (k !== "raw_html") missing.set(why, [...(missing.get(why) ?? []), fieldLabels[k] ?? k]);

  return (
    <aside aria-label="Details" className="flex h-full flex-col overflow-y-auto bg-surface">
      <header className="flex flex-col gap-2 border-b border-line px-3 py-3">
        <div className="flex items-center gap-2">
          <Badge tone={tone}>{d.final_state}</Badge>
          {!d.internal && <Badge tone="neutral">External</Badge>}
        </div>
        <h2 className="m-0 text-base font-medium break-all text-fg" data-selectable title={d.url}>
          {d.url_readable}
        </h2>
      </header>

      {(d.error_label || d.not_crawled_reason) && (
        <Section title={d.error_label ? "Error" : "Not crawled"}>
          <Notice kind={d.error_label ? "bad" : "warn"}>
            <span data-selectable>
              {d.error_label ?? d.not_crawled_reason}
              {d.error_message ? `: ${d.error_message}` : d.reason_detail ? `: ${d.reason_detail}` : ""}
              {d.robots_rule ? ` (rule ${d.robots_rule})` : ""}
            </span>
          </Notice>
        </Section>
      )}

      <Section title="Response">
        <Facts rows={facts} />
      </Section>

      {d.link_metrics && <LinkScoreSection m={d.link_metrics} />}

      {aiChecks && d.internal && (
        <AiPageSection
          transport={transport}
          path={path}
          id={d.id}
          section={(title, body) => <Section title={title}>{body}</Section>}
        />
      )}

      {d.redirects.length > 0 && (
        <Section title={`Redirects (${d.redirects.length - 1} ${d.redirects.length === 2 ? "hop" : "hops"})`}>
          {d.redirect_loop && <Notice kind="bad">The chain loops back on itself.</Notice>}
          {d.redirect_over_limit && <Notice kind="warn">The chain is longer than the crawl's redirect limit.</Notice>}
          <ol className="m-0 flex list-none flex-col gap-1 p-0 text-sm" data-selectable>
            {d.redirects.map((h, i) => {
              const t = cellTone("status", h.status);
              return (
                <UrlRow key={`${h.id}-${i}`} actions={actions} id={h.id} url={h.url} className="flex items-baseline gap-1.5">
                  {i > 0 ? <CornerDownRight aria-hidden className="size-3 shrink-0 text-fg-3" /> : <span className="w-3 shrink-0" />}
                  <span className={cn("w-8 shrink-0 tabular", t ? toneText[t] : "text-fg-3")}>{h.status ?? "–"}</span>
                  <span className="min-w-0 flex-1 break-all">{h.url}</span>
                </UrlRow>
              );
            })}
          </ol>
        </Section>
      )}

      <Section
        title="Links"
        aside={
          <Segmented<ListKind>
            size="sm"
            label="Link list"
            value={list}
            onChange={setList}
            options={[
              { value: "inlinks", label: `In ${d.inlinks.toLocaleString()}` },
              { value: "outlinks", label: `Out ${d.outlinks.toLocaleString()}` },
              { value: "images", label: `Images ${d.images.toLocaleString()}` },
              ...(d.unavailable.headings ? [] : [{ value: "headings" as const, label: `Headings ${d.headings.toLocaleString()}` }]),
            ]}
          />
        }
      >
        <LinkList key={`${path}|${d.id}|${list}`} transport={transport} path={path} id={d.id} kind={list} total={counts[list]} actions={actions} />
        {list === "inlinks" && d.inlinks > 0 && onAnchorCloud && (
          <Button size="sm" variant="quiet" className="mt-1 self-start" onClick={onAnchorCloud}>
            Anchor text cloud
          </Button>
        )}
      </Section>

      {d.headers.length > 0 && (
        <Section title={`Response headers (${d.headers.length})`}>
          <dl className="m-0 flex flex-col gap-0.5 font-mono text-xs" data-selectable>
            {d.headers.map(([k, v], i) => (
              <div key={`${k}-${i}`} className="flex gap-2">
                <dt className="shrink-0 text-fg-3">{k}</dt>
                <dd className="m-0 min-w-0 break-all text-fg-2">{v}</dd>
              </div>
            ))}
          </dl>
        </Section>
      )}

      <Section
        title="Page HTML"
        aside={
          d.html_stored && (
            <Button size="sm" onClick={() => setHtmlOpen(true)}>
              <FileCode className="size-3.5" /> View source
            </Button>
          )
        }
      >
        <p className="m-0 text-sm text-fg-3">
          {d.html_stored
            ? "The page's source as received, searchable and ready to copy."
            : d.unavailable.raw_html
              ? "Not stored in this crawl. Turn on Settings › Page data › Store page HTML and crawl again to keep each page's source."
              : "None stored for this URL: it is not an HTML page or no body was received."}
        </p>
        <HtmlViewer transport={transport} path={path} id={d.id} url={d.url} open={htmlOpen} onOpenChange={setHtmlOpen} />
      </Section>
      {d.cookies.length > 0 && (
        <Section title={`Cookies set (${d.cookies.length})`}>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label="Cookies set" data-selectable>
            {d.cookies.map((c) => (
              <li key={`${c.name}\n${c.domain}\n${c.path}`} className="flex flex-col gap-0.5">
                <div className="flex min-w-0 items-baseline gap-2 text-sm">
                  <span className="min-w-0 truncate font-mono text-fg">{c.name}</span>
                  <span className="min-w-0 truncate text-fg-3">
                    {c.domain}
                    {c.path !== "/" && c.path}
                  </span>
                  <span className="ml-auto shrink-0 text-xs text-fg-3">{expiresText(c)}</span>
                </div>
                <CookieFlags c={c} />
              </li>
            ))}
          </ul>
        </Section>
      )}

      {missing.size > 0 && (
        <Section title="Not collected">
          {[...missing].map(([why, fields]) => (
            <p key={why} className="m-0 text-sm text-fg-3">
              <span className="text-fg-2">{fields.join(", ")}.</span> {why}
            </p>
          ))}
        </Section>
      )}

      {/* Copy and show-in-table results, announced as they happen. */}
      <div role="status" aria-live="polite" className="sticky bottom-0 mt-auto">
        {status && (
          <Notice kind={status.kind} className="m-2 rounded-md border border-line bg-surface px-2 py-1.5 shadow-popover">
            {status.text}
          </Notice>
        )}
      </div>
    </aside>
  );
}
