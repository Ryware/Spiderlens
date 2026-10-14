// The detail pane for the selected URL (US3, FR-019..FR-025).

import { useEffect, useState, type ReactNode } from "react";
import { CornerDownRight, MousePointerClick } from "lucide-react";
import type { ListKind } from "../generated/ListKind";
import type { ListPage } from "../generated/ListPage";
import type { AgentField } from "../generated/AgentField";
import type { PageAgentFindings } from "../generated/PageAgentFindings";
import type { UrlDetail } from "../generated/UrlDetail";
import type { Transport } from "../transport";
import { Badge, Button, cn, EmptyState, Notice, Segmented, type Tone } from "../components/ui";
import { cellTone, formatBytes, toneText } from "./format";

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

function LinkList({ transport, path, id, kind, total }: { transport: Transport; path: string; id: number; kind: ListKind; total: number }) {
  const [page, setPage] = useState<ListPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let off = false;
    setPage(null);
    setError(null);
    transport.urlList(path, id, kind, 0, LIST_PAGE).then(
      (p) => !off && setPage(p),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, kind]);
  const more = () =>
    page &&
    transport.urlList(path, id, kind, page.rows.length, LIST_PAGE).then(
      (p) => setPage({ ...p, from: 0, rows: [...page.rows, ...p.rows], headings: [...page.headings, ...p.headings] }),
      (e) => setError(String(e)),
    );
  if (error) return <Notice kind="bad">{error}</Notice>;
  if (!page) return <div className="text-sm text-fg-3">Loading…</div>;
  if (page.unavailable) return <div className="text-sm text-fg-3">{page.unavailable}</div>;
  const shown = kind === "headings" ? page.headings.length : page.rows.length;
  if (shown === 0) return <div className="text-sm text-fg-3">None.</div>;
  return (
    <div className="flex flex-col">
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
            <li key={`${r.id}-${i}`} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className={cn("w-8 shrink-0 text-right tabular", tone ? toneText[tone] : "text-fg-3")}>{r.status ?? "–"}</span>
              <span className="min-w-0 flex-1 truncate" title={r.url}>
                {r.url}
              </span>
              <LinkText kind={kind} text={r.text} />
              {(r.nofollow || !r.followed || r.sponsored || r.ugc) && (
                <span className="shrink-0 text-xs text-fg-3">
                  {[r.nofollow ? "nofollow" : !r.followed ? "not followed" : null, r.sponsored && "sponsored", r.ugc && "ugc"].filter(Boolean).join(", ")}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      )}
      {shown < total && (
        <Button size="sm" variant="quiet" className="mt-1 self-start" onClick={() => void more()}>
          Show more ({(total - shown).toLocaleString()} left)
        </Button>
      )}
    </div>
  );
}

const actionKinds: Record<string, string> = {
  anchor_no_href: "Link without an address",
  anchor_hash: "Link to #",
  anchor_js: "javascript: link",
  onclick: "Click handler",
  role_button: "Button role",
  role_link: "Link role",
};

const linkKinds: Record<string, string> = {
  markdown_alternate: "Markdown version",
  service_desc: "API description",
  service_doc: "API documentation",
  api_catalog: "API catalog",
};

/** A field's accessible name, or why an agent cannot tell what it is for. */
function FieldName({ f }: { f: AgentField }) {
  if (f.name_source === "none") return <span className="text-warn">No label</span>;
  return (
    <span className="min-w-0 truncate text-fg" title={f.accessible_name ?? undefined}>
      {f.accessible_name}
      {f.name_source === "placeholder" && <span className="ml-1.5 text-xs text-warn">placeholder only</span>}
    </span>
  );
}

/** How an AI agent would see this page's forms and actions (spec 016 FR-019). */
function AgentsSection({ transport, path, id }: { transport: Transport; path: string; id: number }) {
  const [f, setF] = useState<PageAgentFindings | null>(null);
  useEffect(() => {
    let off = false;
    setF(null);
    transport.agentsPage(path, id).then(
      (x) => !off && setF(x),
      () => !off && setF(null),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id]);
  if (!f || f.id !== id || !f.available || !f.checked) return null;
  const empty = f.forms.length === 0 && f.actions_total === 0 && f.nameless_buttons === 0 && f.links.length === 0;
  const count = (n: number) => <span className={n > 0 ? "text-warn" : undefined}>{n.toLocaleString()}</span>;
  return (
    <Section title="AI agents">
      {empty ? (
        <p className="m-0 text-sm text-fg-3">No forms or actions on this page.</p>
      ) : (
        <>
          <Facts
            rows={[
              ["Forms", f.forms.length.toLocaleString()],
              ["Fields without a label", count(f.unlabelled_fields)],
              ["Buttons without a name", count(f.nameless_buttons)],
              ["Actions without a link", count(f.actions_total)],
            ]}
          />
          {f.forms.map((form, i) => (
            <div key={i} aria-label={`Form ${i + 1}`} role="group" className="mt-1 flex flex-col gap-1 rounded-md border border-line p-2">
              <div className="flex flex-wrap items-center gap-1.5 text-sm">
                <span className="font-medium text-fg uppercase">{form.method}</span>
                <span className="min-w-0 flex-1 truncate text-fg-2" title={form.action ?? undefined} data-selectable>
                  {form.action ?? "this page"}
                </span>
                {form.source === "rendered" && <Badge tone="info">After JavaScript</Badge>}
                {form.is_search && <Badge>Search</Badge>}
                {form.tool_name && <Badge tone="ok">Agent tool: {form.tool_name}</Badge>}
                {form.challenge && <Badge tone="warn">CAPTCHA: {form.challenge}</Badge>}
                {!form.has_submit && <Badge tone="warn">No submit button</Badge>}
              </div>
              {form.fields.length > 0 && (
                <ul className="m-0 flex list-none flex-col p-0" data-selectable>
                  {form.fields.map((fl, j) => (
                    <li key={j} className="flex items-baseline gap-2 border-t border-line py-0.5 text-sm">
                      <span className="w-20 shrink-0 truncate font-mono text-xs text-fg-3" title={fl.name ?? undefined}>
                        {fl.name ?? "–"}
                      </span>
                      <span className="w-16 shrink-0 text-xs text-fg-3">{fl.kind}</span>
                      <FieldName f={fl} />
                      {fl.required && <span className="ml-auto shrink-0 text-xs text-fg-3">required</span>}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ))}
          {f.actions.length > 0 && (
            <ul aria-label="Actions without a link" className="m-0 mt-1 flex list-none flex-col p-0" data-selectable>
              {f.actions.map((a, i) => (
                <li key={i} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
                  <span className="min-w-0 flex-1 truncate text-fg">{a.text || <span className="text-fg-3">(no text)</span>}</span>
                  <span className="shrink-0 text-xs text-fg-3">{actionKinds[a.kind] ?? a.kind}</span>
                </li>
              ))}
            </ul>
          )}
          {f.links.length > 0 && (
            <Facts rows={f.links.map((l) => [linkKinds[l.kind] ?? l.kind, <span className="break-all">{l.href}</span>] as [string, ReactNode])} />
          )}
        </>
      )}
    </Section>
  );
}

export function Inspector({ transport, path, id }: { transport: Transport; path: string | null; id: number | null }) {
  const [detail, setDetail] = useState<UrlDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [list, setList] = useState<ListKind>("inlinks");

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
  if (d.indexability) {
    const non = d.indexability.startsWith("Non-indexable");
    facts.push(["Indexability", <span className={non ? "text-warn" : undefined}>{d.indexability}</span>]);
  }
  if (d.fetched_at !== null) facts.push(["Fetched", new Date(d.fetched_at).toLocaleString()]);
  const counts: Record<ListKind, number> = { inlinks: d.inlinks, outlinks: d.outlinks, images: d.images, headings: d.headings };
  // Fields the file has no data for, grouped by why.
  const missing = new Map<string, string[]>();
  for (const [k, why] of Object.entries(d.unavailable)) missing.set(why, [...(missing.get(why) ?? []), fieldLabels[k] ?? k]);

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

      {d.redirects.length > 0 && (
        <Section title={`Redirects (${d.redirects.length - 1} ${d.redirects.length === 2 ? "hop" : "hops"})`}>
          {d.redirect_loop && <Notice kind="bad">The chain loops back on itself.</Notice>}
          {d.redirect_over_limit && <Notice kind="warn">The chain is longer than the crawl's redirect limit.</Notice>}
          <ol className="m-0 flex list-none flex-col gap-1 p-0 text-sm" data-selectable>
            {d.redirects.map((h, i) => {
              const t = cellTone("status", h.status);
              return (
                <li key={`${h.id}-${i}`} className="flex items-baseline gap-1.5">
                  {i > 0 ? <CornerDownRight aria-hidden className="size-3 shrink-0 text-fg-3" /> : <span className="w-3 shrink-0" />}
                  <span className={cn("w-8 shrink-0 tabular", t ? toneText[t] : "text-fg-3")}>{h.status ?? "–"}</span>
                  <span className="min-w-0 break-all">{h.url}</span>
                </li>
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
        <LinkList transport={transport} path={path} id={d.id} kind={list} total={counts[list]} />
      </Section>

      <AgentsSection transport={transport} path={path} id={d.id} />

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

      {missing.size > 0 && (
        <Section title="Not collected">
          {[...missing].map(([why, fields]) => (
            <p key={why} className="m-0 text-sm text-fg-3">
              <span className="text-fg-2">{fields.join(", ")}.</span> {why}
            </p>
          ))}
        </Section>
      )}
    </aside>
  );
}
