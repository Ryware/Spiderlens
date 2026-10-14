import { useEffect, useState, type ReactNode } from "react";
import { Copy, Info, MousePointerClick } from "lucide-react";
import { formatElapsed, type CrawlRun, type Phase } from "../crawl/useCrawlRun";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { BuildInfo, Transport } from "../transport";
import { Badge, Bar, Button, cn, EmptyState, Kbd, Modal, Notice, Tooltip, type Tone } from "../components/ui";
import { IssueList } from "../settings/fields";
import { PausedSites } from "./PausedSites";
import { classLabel } from "../crawl/errorClasses";
import { LocalErrorNotice } from "../files/LocalErrorNotice";
import { RecentCrawls, StorageWarning, UnsavedCrawls } from "../files/RecentCrawls";
import type { CrawlLibrary } from "../files/useCrawlLibrary";

/** Detail panel (FR-001): explains the selected URL once results exist (spec 007). */
export function DetailPanel() {
  return (
    <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
      <EmptyState icon={<MousePointerClick className="size-6" />} title="Nothing selected">
        Select a URL in the results to see its response, links, headings and issues here.
      </EmptyState>
    </aside>
  );
}

const robotsLabel = { respect: "robots.txt respected", ignore_report: "robots.txt ignored, reported", ignore: "robots.txt ignored" } as const;

/** How each phase reads in the status bar and the overview. */
export const phaseInfo: Record<Phase, { label: string; tone: Tone | "accent" }> = {
  idle: { label: "Idle", tone: "neutral" },
  starting: { label: "Starting…", tone: "accent" },
  running: { label: "Crawling", tone: "accent" },
  pausing: { label: "Pausing…", tone: "warn" },
  stopping: { label: "Stopping…", tone: "warn" },
  paused: { label: "Paused", tone: "warn" },
  stopped: { label: "Stopped", tone: "neutral" },
  completed: { label: "Completed", tone: "ok" },
  failed: { label: "Failed", tone: "bad" },
};

const dots: Record<Tone | "accent", string> = { accent: "bg-accent", ok: "bg-ok", info: "bg-info", warn: "bg-warn", bad: "bg-bad", neutral: "bg-fg-3" };

const count = (n: number | undefined) => (n ?? 0).toLocaleString();

/** Why the crawl paused itself (spec 004 FR-015, FR-029), in plain words. */
const pauseReasons: Record<string, string> = {
  network: "SpiderLens paused this crawl because your network connection seemed to be down. No URL was marked as failed.",
  local: "SpiderLens paused this crawl because of a problem on this computer. No URL was marked as failed.",
};

/** Live crawl figures (FR-006), from the engine's progress events. */
export function StatusBar({ setup, run }: { setup: CrawlSetup; run: CrawlRun }) {
  const p = run.progress;
  const s = run.report?.summary;
  const wait = run.waitingForHost;
  // Every remaining URL's site is paused: say so, with the soonest test, so
  // the crawl does not look frozen (spec 004 US4.4).
  const info = wait ? { label: "Waiting for site", tone: "warn" as const } : phaseInfo[run.phase];
  // Value, label and, when the label is long, a shorter test hook.
  const figures: [string, string, string?][] = [
    [count(p?.crawled ?? s?.crawled), "crawled"],
    [count(run.running ? p?.queued : (s?.queued ?? p?.queued)), "queued"],
    [count(p?.errors), p?.errors === 1 ? "error" : "errors"],
    [run.rate === null ? "–" : run.rate.toFixed(run.rate < 10 ? 1 : 0), "URLs/s"],
    [formatElapsed(run.elapsedMs), "elapsed"],
  ];
  // Spider mode keeps finding URLs, so this is for the ones found so far.
  if (run.running && p?.eta_secs != null && !wait) figures.push([`~${formatElapsed(p.eta_secs * 1000)}`, "left"]);
  if (run.running && p?.waiting) figures.splice(3, 0, [count(p.waiting), "waiting to retry", "waiting"]);
  const speed = setup.speedPreset === "polite" ? "Polite" : setup.speedPreset === "fast" ? "Fast" : "Custom speed";
  return (
    <footer aria-label="Crawl status" data-region="chrome" className="flex h-statusbar shrink-0 items-center gap-4 overflow-hidden border-t border-line bg-chrome px-3 text-xs whitespace-nowrap text-fg-3 tabular">
      <span className="flex min-w-0 items-center gap-1.5 text-fg-2" role="status" data-phase={run.phase}>
        <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", dots[info.tone], run.running && "motion-safe:animate-pulse")} />
        <span className="truncate">
          {info.label}
          {run.phase !== "idle" && run.target ? ` ${run.target}` : ""}
          {wait && (wait.probing || wait.secondsLeft === null ? " · Testing…" : ` · ${formatElapsed(wait.secondsLeft * 1000)}`)}
        </span>
      </span>
      {figures.map(([n, label, key]) => (
        <span key={label} data-figure={key ?? label}>
          <span className="font-medium text-fg-2">{n}</span> {label}
        </span>
      ))}
      <PausedSites run={run} />
      <span className="ml-auto">
        {setup.loaded ? `${setup.loaded.name} · ` : ""}
        {speed}
        {setup.config ? ` · ${robotsLabel[setup.config.robots.mode]}` : ""}
      </span>
    </footer>
  );
}

const robotsOutcome = { ok: "Read", not_found: "None found", forbidden: "Forbidden (treated as none)", unavailable: "Unavailable", ignored: "Ignored", pending: "Not read yet" } as const;

/** One large figure in the overview. */
function Figure({ label, value, tone }: { label: string; value: string; tone?: "bad" }) {
  return (
    <div className="flex flex-col gap-0.5 px-4 py-3">
      <dt className="text-xs text-fg-3">{label}</dt>
      <dd className={cn("m-0 text-xl font-semibold tracking-tight tabular", tone === "bad" ? "text-bad" : "text-fg")}>{value}</dd>
    </div>
  );
}

/**
 * The Overview section once a crawl exists: live figures while it runs, then
 * why URLs were not crawled, robots.txt per site and sitemaps (spec 003 US2).
 * The URLs themselves are in the result tabs (spec 007).
 */
export function CrawlOverview({ run, onInfo, onResume, onSaveAs }: { run: CrawlRun; onInfo?: () => void; onResume?: () => void; onSaveAs?: () => void }) {
  const p = run.progress;
  const s = run.report?.summary;
  const info = phaseInfo[run.phase];
  const discovered = s?.discovered ?? p?.discovered ?? 0;
  const crawled = s?.crawled ?? p?.crawled ?? 0;
  const notCrawled = s?.not_crawled ?? p?.not_crawled ?? 0;
  const queued = s?.queued ?? p?.queued ?? 0;
  const errors = p?.errors ?? (s?.transient_errors !== undefined ? s.transient_errors + s.permanent_errors : 0);
  const done = discovered === 0 ? 0 : (crawled + notCrawled) / discovered;
  const warnings = s?.warnings.length ? s.warnings : run.warnings;
  // A notice that also came as a warning is shown once.
  const notices = run.notices.filter((n) => !warnings.includes(n));
  const byClass = s?.by_class ? Object.entries(s.by_class).filter(([, n]) => n > 0) : [];
  const retried = s ? [s.retries, s.recovered, s.hosts_paused, s.manual_retries].some((n) => (n ?? 0) > 0) : false;
  return (
    <section aria-label="Overview" className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-5 p-6">
        <header className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <h1 className="m-0 min-w-0 truncate text-lg font-semibold tracking-tight">{run.target || "Crawl"}</h1>
            {info.tone === "accent" ? <Badge tone="info">{info.label}</Badge> : <Badge tone={info.tone}>{info.label}</Badge>}
            <span className="ml-auto text-sm text-fg-3 tabular">{formatElapsed(run.elapsedMs)}</span>
            {run.path && onInfo && (
              <Tooltip content="Crawl information">
                <Button variant="quiet" size="sm" icon aria-label="Crawl information" onClick={onInfo}>
                  <Info className="size-3.5" />
                </Button>
              </Tooltip>
            )}
          </div>
          <Bar value={done} label="URLs handled" tone={run.running ? "accent" : info.tone === "accent" ? "neutral" : info.tone} className="w-full" />
          {run.path && (
            <p className="m-0 truncate text-sm text-fg-3" title={run.path}>
              Saved to <span className="font-code text-fg-2" data-selectable>{run.path}</span>
            </p>
          )}
        </header>

        {run.local && (
          <LocalErrorNotice error={run.local} onResume={() => onResume?.()} onSaveAs={() => onSaveAs?.()} onKeepPaused={run.dismissLocal} />
        )}
        {run.error && (
          <Notice kind="bad" role="alert">
            {run.error}
          </Notice>
        )}
        {s?.pause_reason && <Notice kind="warn">{pauseReasons[s.pause_reason] ?? `SpiderLens paused this crawl (${s.pause_reason}).`}</Notice>}
        {s?.url_limit_reached && <Notice kind="info">The URL limit was reached: the remaining URLs are listed as "Over URL limit".</Notice>}
        {warnings.map((w) => (
          <Notice key={w} kind="warn">
            {w}
          </Notice>
        ))}
        {notices.length > 0 && (
          <ul aria-label="Crawl notices" className="m-0 flex list-none flex-col gap-1 p-0">
            {notices.map((n) => (
              <li key={n}>
                <Notice kind="info">{n}</Notice>
              </li>
            ))}
          </ul>
        )}

        <dl className="m-0 grid grid-cols-5 divide-x divide-line overflow-hidden rounded-lg border border-line bg-surface">
          <Figure label="Discovered" value={count(discovered)} />
          <Figure label="Crawled" value={count(crawled)} />
          <Figure label="Not crawled" value={count(notCrawled)} />
          <Figure label="Queued" value={count(queued)} />
          <Figure label="Errors" value={count(errors)} tone={errors ? "bad" : undefined} />
        </dl>

        {run.report && run.report.reasons.length > 0 && (
          <Table title="Not crawled" head={["Reason", "URLs"]}>
            {run.report.reasons.map((r) => (
              <tr key={r.reason} data-reason={r.reason}>
                <td className="px-3 py-1.5">{r.label}</td>
                <td className="px-3 py-1.5 text-right tabular">{count(r.count)}</td>
              </tr>
            ))}
          </Table>
        )}
        {byClass.length > 0 && (
          <Table title="Errors" head={["Error", "URLs"]} aside={`${count(s?.transient_errors)} temporary · ${count(s?.permanent_errors)} permanent`}>
            {byClass
              .sort((a, b) => b[1] - a[1])
              .map(([cls, n]) => (
                <tr key={cls} data-class={cls}>
                  <td className="px-3 py-1.5">{classLabel(cls, run.report)}</td>
                  <td className="px-3 py-1.5 text-right text-bad tabular">{count(n)}</td>
                </tr>
              ))}
          </Table>
        )}
        {/* Without counts by class, only the failures without a response are known. */}
        {byClass.length === 0 && run.report?.failures && run.report.failures.length > 0 && (
          <Table title="Failed without a response" head={["Error", "URLs"]}>
            {[...run.report.failures]
              .sort((a, b) => b.count - a.count)
              .map((f) => (
                <tr key={f.class} data-failure={f.class}>
                  <td className="px-3 py-1.5">{f.label}</td>
                  <td className="px-3 py-1.5 text-right text-bad tabular">{count(f.count)}</td>
                </tr>
              ))}
          </Table>
        )}
        {s && retried && (
          <Table title="Retries" head={["What", "Count"]}>
            {(
              [
                ["Automatic retries", s.retries],
                ["URLs that worked after a retry", s.recovered],
                ["Sites paused", s.hosts_paused],
                ["Times you retried failed URLs", s.manual_retries],
              ] as const
            ).map(([label, n]) => (
              <tr key={label}>
                <td className="px-3 py-1.5">{label}</td>
                <td className="px-3 py-1.5 text-right tabular">{count(n)}</td>
              </tr>
            ))}
          </Table>
        )}
        {s && s.hosts.length > 0 && (
          <Table title="robots.txt" head={["Site", "robots.txt"]}>
            {s.hosts.map((h) => (
              <tr key={h.host}>
                <td className="px-3 py-1.5 font-code">{h.host}</td>
                <td className="px-3 py-1.5 text-right">
                  {robotsOutcome[h.robots]}
                  {h.crawl_delay_secs !== null ? `, crawl-delay ${h.crawl_delay_secs} s` : ""}
                  {h.error ? ` (${h.error})` : ""}
                </td>
              </tr>
            ))}
          </Table>
        )}
        {s && s.sitemaps.length > 0 && (
          <Table title="Sitemaps" head={["Sitemap", "URLs"]}>
            {s.sitemaps.map((m) => (
              <tr key={m.url}>
                <td className="max-w-0 truncate px-3 py-1.5 font-code" title={m.url}>
                  {m.url}
                </td>
                <td className={cn("px-3 py-1.5 text-right tabular", m.error && "text-bad")}>{m.error ?? count(m.urls)}</td>
              </tr>
            ))}
          </Table>
        )}
      </div>
    </section>
  );
}

function Table({ title, head, aside, children }: { title: string; head: [string, string]; aside?: ReactNode; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2 px-1">
        <h2 className="m-0 text-xs font-semibold text-fg-3">{title}</h2>
        {aside && (
          <span className="ml-auto text-xs text-fg-3 tabular" data-aside>
            {aside}
          </span>
        )}
      </div>
      <div className="overflow-hidden rounded-lg border border-line bg-surface">
        <table className="w-full table-fixed border-collapse text-sm">
          <thead className="sr-only">
            <tr>
              <th>{head[0]}</th>
              <th>{head[1]}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">{children}</tbody>
        </table>
      </div>
    </section>
  );
}

/** Main area before any crawl (FR-008): what to do next, and anything to be careful about. */
export function StartState({
  setup,
  error,
  onFocusUrl,
  onOpenSettings,
  library,
  onOpenRecent,
  onCompare,
  onManageCrawls,
}: {
  setup: CrawlSetup;
  error?: string | null;
  onFocusUrl: () => void;
  onOpenSettings: () => void;
  /** Recent and unsaved crawls (spec 006 US4). */
  library?: CrawlLibrary;
  onOpenRecent?: (path: string) => void;
  /** Opens the comparison setup (spec 012). */
  onCompare?: () => void;
  /** Opens Manage crawls (spec 019 US1, FR-023). */
  onManageCrawls?: () => void;
}) {
  const mode = setup.config?.mode ?? "spider";
  const urlIssues = setup.issues.filter((i) => i.path === "input.start_url");
  return (
    <section aria-label="Start" className="h-full overflow-y-auto">
      <div className="flex min-h-full flex-col items-center justify-center gap-5 p-8">
        {library && <StorageWarning storage={library.storage} onManage={onManageCrawls} />}
        <div className="flex max-w-md flex-col items-center gap-2 text-center">
          <h1 className="m-0 text-xl font-semibold tracking-tight text-balance">{mode === "spider" ? "Crawl a website" : "Crawl a list of URLs"}</h1>
          <p className="m-0 text-base text-fg-2">
            {mode === "spider"
              ? "Enter a start URL above and press Start. SpiderLens follows links from there and lists every page it finds, with its issues."
              : "Add your URLs, then press Start. Only the URLs in the list are crawled."}
          </p>
        </div>

        {/* A crawl file that could not be opened (Open crawl…). */}
        {error && (
          <Notice kind="bad" role="alert" className="w-full max-w-md">
            {error}
          </Notice>
        )}

        {mode === "spider" && setup.startUrl !== "" && <IssueList issues={urlIssues} />}

        {setup.warnings.length > 0 && (
          <div aria-label="Less respectful settings" className="flex w-full max-w-md flex-col gap-2 rounded-lg border border-line bg-surface px-4 py-3">
            <div className="flex items-center gap-2">
              <Badge tone="warn">Less respectful settings</Badge>
              <Button size="sm" variant="quiet" className="ml-auto" onClick={onOpenSettings}>
                Review
              </Button>
            </div>
            <IssueList issues={setup.warnings} />
          </div>
        )}

        <div className="flex flex-wrap items-center justify-center gap-x-4 gap-y-1 text-sm text-fg-3">
          <button type="button" className="rounded-sm hover:text-fg-2" onClick={onFocusUrl}>
            Start URL <Kbd keys="Mod+L" />
          </button>
          <span>
            Settings <Kbd keys="Mod+," />
          </span>
          <span>
            Commands <Kbd keys="Mod+K" />
          </span>
          {onCompare && (
            <button type="button" className="rounded-sm hover:text-fg-2" onClick={onCompare}>
              Compare crawls…
            </button>
          )}
        </div>

        {library && onOpenRecent && <RecentCrawls library={library} onOpen={onOpenRecent} />}
        {library && <UnsavedCrawls library={library} onManage={onManageCrawls} />}
      </div>
    </section>
  );
}

/** About (FR-007): version, engine and build, copyable. */
export function AboutDialog({ open, onOpenChange, transport }: { open: boolean; onOpenChange: (o: boolean) => void; transport: Transport }) {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    transport.buildInfo().then(setInfo, (e) => setError(String(e)));
  }, [transport]);
  const text = info ? `SpiderLens ${info.product_version} (engine ${info.engine_version}, build ${info.build_id})` : "";
  return (
    <Modal open={open} onOpenChange={onOpenChange} title="SpiderLens" description="SEO crawler for Windows, macOS and Linux.">
      {error && (
        <Notice kind="bad" role="alert">
          Could not read build information: {error}
        </Notice>
      )}
      {info && (
        <dl className="m-0 grid grid-cols-2 gap-x-4 gap-y-1 text-sm" data-selectable>
          <dt className="text-fg-3">Version</dt>
          <dd className="m-0 tabular" data-testid="product-version">
            {info.product_version}
          </dd>
          <dt className="text-fg-3">Engine</dt>
          <dd className="m-0 tabular" data-testid="engine-version">
            {info.engine_version}
          </dd>
          <dt className="text-fg-3">Build</dt>
          <dd className="m-0 font-code" data-testid="build-id">
            {info.build_id}
          </dd>
        </dl>
      )}
      <div className="flex justify-end gap-2">
        {info && (
          <Button
            onClick={() =>
              navigator.clipboard?.writeText(text).then(
                () => setCopied(true),
                () => setCopied(false),
              )
            }
          >
            <Copy className="size-3.5" /> {copied ? "Copied" : "Copy"}
          </Button>
        )}
        <Button variant="primary" onClick={() => onOpenChange(false)}>
          Close
        </Button>
      </div>
    </Modal>
  );
}
