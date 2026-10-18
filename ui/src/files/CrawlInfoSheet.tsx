import { useEffect, useState, type ReactNode } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import { Button, Notice, Sheet, SheetClose } from "../components/ui";
import type { CrawlInfo, Transport, VersionRecord } from "../transport";
import { EntryBadges } from "./RecentCrawls";
import { formatDateTime, formatSize } from "./format";

const robots = { respect: "Respected", ignore_report: "Ignored, blocked URLs reported", ignore: "Ignored" } as const;
const versionVerb: Record<VersionRecord["kind"], string> = { created: "Created", resumed: "Resumed", upgraded: "Upgraded" };
const noLimit = (n: number | null) => (n === null ? "No limit" : n.toLocaleString());
const list = (xs: string[]) => (xs.length ? xs.join("\n") : "None");

/** The stored configuration as label and value, for reading (spec 006 US6-1). Secrets never appear. */
export function configRows(c: CrawlConfig): [string, string][] {
  const s = c.speed;
  const ua = c.identity.user_agent;
  const rows: [string, string][] = [
    ["Mode", c.mode === "spider" ? "Spider" : "List"],
    ["URL limit", noLimit(c.limits.max_urls)],
    c.mode === "spider" ? ["Depth limit", noLimit(c.limits.max_depth)] : ["Follow links from listed URLs", c.limits.list_depth === 0 ? "No" : `${c.limits.list_depth} clicks`],
    ["Include", list(c.scope.include)],
    ["Exclude", list(c.scope.exclude)],
    ["Subdomains", c.scope.subdomains === "this_host" ? "This host only" : "All subdomains"],
    ["Check external links", c.scope.check_external ? "Yes" : "No"],
    ["Speed", `${s.max_concurrent} at once, ${s.max_per_host} per host, ${s.max_urls_per_sec === null ? "no rate limit" : `up to ${s.max_urls_per_sec} URLs/s`}${s.adaptive ? ", slows down when the site does" : ""}`],
    ["robots.txt", `${robots[c.robots.mode]}${c.robots.honour_crawl_delay ? ", crawl-delay honoured" : ""}`],
    ["User agent", ua.preset === "custom" ? (ua.custom_string ?? "Custom") : ua.preset],
    ["Retries", `${c.retry.attempts} attempts, up to ${c.retry.max_wait_secs} s apart`],
    ["Timeouts", `Connect ${c.network.connect_timeout_secs} s, read ${c.network.read_timeout_secs} s`],
  ];
  const l = c.limits;
  if (l.max_urls_per_subdomain !== null) rows.push(["URLs per subdomain", noLimit(l.max_urls_per_subdomain)]);
  if (l.max_urls_per_depth !== null) rows.push(["URLs per depth", noLimit(l.max_urls_per_depth)]);
  if (l.max_folder_depth !== null) rows.push(["Folder depth limit", noLimit(l.max_folder_depth)]);
  if (l.max_query_params !== null) rows.push(["Query parameter limit", noLimit(l.max_query_params)]);
  if (c.scope.rewrite.length) rows.push(["URL rewriting", c.scope.rewrite.map((r) => `${r.pattern} => ${r.replace}`).join("\n")]);
  if (c.robots.custom.length) rows.push(["Custom robots.txt", c.robots.custom.map((r) => r.host).join("\n")]);
  for (const h of c.identity.headers) rows.push([`Header ${h.name}`, "plain" in h.value ? h.value.plain : "secret, not stored"]);
  return rows;
}

/**
 * Crawl information (spec 006 US6): what the file is, how it was made and
 * with which settings, so results can be explained and repeated.
 */
export function CrawlInfoSheet({
  open,
  onOpenChange,
  transport,
  path,
  onNewCrawl,
  onScheduleAgain,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  path: string | null;
  onNewCrawl: (info: CrawlInfo) => void;
  /** A new schedule with this crawl's settings and input (spec 021 FR-002). */
  onScheduleAgain?: (info: CrawlInfo) => void;
}) {
  const [info, setInfo] = useState<CrawlInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open || !path) return;
    let live = true;
    setError(null);
    transport.crawlInfo(path).then(
      (i) => live && setInfo(i),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [open, path, transport]);
  const i = info?.path === path ? info : null;

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Crawl information" className="w-160 sheet-max-w">
      <header className="flex items-center gap-2 border-b border-line px-5 py-3">
        <h2 className="m-0 min-w-0 truncate text-md font-semibold">{i?.name ?? "Crawl information"}</h2>
        {i && <EntryBadges entry={i} />}
      </header>
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-4" data-selectable>
        {error && (
          <Notice kind="bad" role="alert">
            Could not read the crawl information: {error}
          </Notice>
        )}
        {!i && !error && <p className="m-0 text-sm text-fg-3">Reading…</p>}
        {i && (
          <>
            <Section title="File">
              <Pairs
                rows={[
                  ["Location", <span className="font-code break-all">{i.path}</span>],
                  ["Size", formatSize(i.size_bytes)],
                  ["Format", `Version ${i.format_version}`],
                  ["Status", i.pause_reason ? `${statusText(i)} (${i.pause_reason})` : statusText(i)],
                  ["Mode", i.mode === "spider" ? "Spider" : "List"],
                  ...(i.start_url ? ([["Start URL", <span className="font-code break-all">{i.start_url}</span>]] as [string, ReactNode][]) : []),
                  ["Started", formatDateTime(i.started_at)],
                  ["Ended", formatDateTime(i.ended_at)],
                ]}
              />
            </Section>
            <Section title="URLs">
              <Pairs
                rows={[
                  ["Discovered", i.counts.discovered.toLocaleString()],
                  ["Crawled", i.counts.crawled.toLocaleString()],
                  ["Failed", i.counts.failed.toLocaleString()],
                  ["Not crawled", i.counts.not_crawled.toLocaleString()],
                  ["Queued", i.counts.queued.toLocaleString()],
                ]}
              />
            </Section>
            <Section title="Versions">
              <ul aria-label="Versions" className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                {i.versions.map((v, n) => (
                  <li key={n} className="flex gap-2">
                    <span className="text-fg">
                      {versionVerb[v.kind]} with SpiderLens {v.app_version}
                    </span>
                    <span className="text-fg-3">on {v.os}</span>
                    <span className="ml-auto text-fg-3 tabular">{formatDateTime(v.at)}</span>
                  </li>
                ))}
              </ul>
            </Section>
            {i.events.length > 0 && (
              <Section title="History">
                <ul aria-label="History" className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                  {i.events.map((e, n) => (
                    <li key={n} className="flex gap-2">
                      <span className="w-36 shrink-0 text-fg-3 tabular">{formatDateTime(e.at)}</span>
                      <span className="min-w-0 flex-1 text-fg">
                        {e.kind[0].toUpperCase() + e.kind.slice(1).replace(/_/g, " ")}
                        {e.detail ? <span className="text-fg-2">: {e.detail}</span> : null}
                      </span>
                      <span className="shrink-0 text-fg-3">{e.app_version}</span>
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            <Section title="Configuration">
              {i.config ? <Pairs rows={configRows(i.config)} label="Configuration" /> : <p className="m-0 text-sm text-fg-3">This file has no configuration SpiderLens can read.</p>}
            </Section>
          </>
        )}
      </div>
      <footer className="flex justify-end gap-2 border-t border-line px-5 py-3">
        {onScheduleAgain && (
          <Button disabled={!i?.config} onClick={() => i && onScheduleAgain(i)}>
            Schedule again…
          </Button>
        )}
        <Button disabled={!i?.config} onClick={() => i && onNewCrawl(i)}>
          New crawl with these settings
        </Button>
        <SheetClose render={<Button variant="primary" />}>Close</SheetClose>
      </footer>
    </Sheet>
  );
}

const statusText = (i: CrawlInfo) => ({ running: "Running", paused: "Paused", stopped: "Stopped (incomplete)", completed: "Completed" })[i.status];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="m-0 text-xs font-semibold text-fg-3">{title}</h3>
      {children}
    </section>
  );
}

function Pairs({ rows, label }: { rows: [string, ReactNode][]; label?: string }) {
  return (
    <dl aria-label={label} className="m-0 flex flex-col gap-1 text-sm">
      {rows.map(([k, v]) => (
        <div key={k} className="flex gap-4">
          <dt className="w-40 shrink-0 text-fg-3">{k}</dt>
          <dd className="m-0 min-w-0 flex-1 whitespace-pre-line text-fg">{v}</dd>
        </div>
      ))}
    </dl>
  );
}
