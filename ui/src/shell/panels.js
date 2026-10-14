import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { Copy, Info, MousePointerClick } from "lucide-react";
import { formatElapsed } from "../crawl/useCrawlRun";
import { Badge, Bar, Button, cn, EmptyState, Kbd, Modal, Notice, Tooltip } from "../components/ui";
import { IssueList } from "../settings/fields";
import { PausedSites, siteName } from "./PausedSites";
import { CrawlSpeed } from "./CrawlSpeed";
import { classLabel } from "../crawl/errorClasses";
import { LocalErrorNotice } from "../files/LocalErrorNotice";
import { RecentCrawls, StorageWarning, UnsavedCrawls } from "../files/RecentCrawls";
/** Detail panel (FR-001): explains the selected URL once results exist (spec 007). */
export function DetailPanel() {
    return (_jsx("aside", { "aria-label": "Details", className: "h-full overflow-y-auto bg-surface", children: _jsx(EmptyState, { icon: _jsx(MousePointerClick, { className: "size-6" }), title: "Nothing selected", children: "Select a URL in the results to see its response, links, headings and issues here." }) }));
}
const robotsLabel = { respect: "robots.txt respected", ignore_report: "robots.txt ignored, reported", ignore: "robots.txt ignored" };
/** How each phase reads in the status bar and the overview. */
export const phaseInfo = {
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
const dots = { accent: "bg-accent", ok: "bg-ok", info: "bg-info", warn: "bg-warn", bad: "bg-bad", neutral: "bg-fg-3" };
const count = (n) => (n ?? 0).toLocaleString();
/** Why the crawl paused itself (spec 004 FR-015, FR-029), in plain words. */
const pauseReasons = {
    network: "SpiderLens paused this crawl because your network connection seemed to be down. No URL was marked as failed.",
    local: "SpiderLens paused this crawl because of a problem on this computer. No URL was marked as failed.",
};
/** Live crawl figures (FR-006), from the engine's progress events. */
export function StatusBar({ setup, run }) {
    const p = run.progress;
    const s = run.report?.summary;
    const wait = run.waitingForHost;
    // Every remaining URL's site is paused: say so, with the soonest test, so
    // the crawl does not look frozen (spec 004 US4.4).
    const info = wait ? { label: "Waiting for site", tone: "warn" } : phaseInfo[run.phase];
    // Value, label and, when the label is long, a shorter test hook.
    const figures = [
        [count(p?.crawled ?? s?.crawled), "crawled"],
        [count(run.running ? p?.queued : (s?.queued ?? p?.queued)), "queued"],
        [count(p?.errors), p?.errors === 1 ? "error" : "errors"],
        [run.rate === null ? "–" : run.rate.toFixed(run.rate < 10 ? 1 : 0), "URLs/s"],
        [formatElapsed(run.elapsedMs), "elapsed"],
    ];
    // Spider mode keeps finding URLs, so this is for the ones found so far.
    if (run.running && p?.eta_secs != null && !wait)
        figures.push([`~${formatElapsed(p.eta_secs * 1000)}`, "left"]);
    if (run.running && p?.waiting)
        figures.splice(3, 0, [count(p.waiting), "waiting to retry", "waiting"]);
    const speed = setup.speedPreset === "polite" ? "Polite" : setup.speedPreset === "fast" ? "Fast" : "Custom speed";
    return (_jsxs("footer", { "aria-label": "Crawl status", "data-region": "chrome", className: "flex h-statusbar shrink-0 items-center gap-4 overflow-hidden border-t border-line bg-chrome px-3 text-xs whitespace-nowrap text-fg-3 tabular", children: [_jsxs("span", { className: "flex min-w-0 items-center gap-1.5 text-fg-2", role: "status", "data-phase": run.phase, children: [_jsx("span", { "aria-hidden": true, className: cn("size-1.5 shrink-0 rounded-full", dots[info.tone], run.running && "motion-safe:animate-pulse") }), _jsxs("span", { className: "truncate", children: [info.label, run.phase !== "idle" && run.target ? ` ${run.target}` : "", wait && (wait.probing || wait.secondsLeft === null ? " · Testing…" : ` · ${formatElapsed(wait.secondsLeft * 1000)}`)] })] }), figures.map(([n, label, key]) => (_jsxs("span", { "data-figure": key ?? label, children: [_jsx("span", { className: "font-medium text-fg-2", children: n }), " ", label] }, label))), _jsx(PausedSites, { run: run }), _jsx(CrawlSpeed, { run: run }), _jsxs("span", { className: "ml-auto", children: [setup.loaded ? `${setup.loaded.name} · ` : "", speed, setup.config ? ` · ${robotsLabel[setup.config.robots.mode]}` : ""] })] }));
}
const robotsOutcome = { ok: "Read", not_found: "None found", forbidden: "Forbidden (treated as none)", unavailable: "Unavailable", ignored: "Ignored", pending: "Not read yet" };
/** One large figure in the overview. */
function Figure({ label, value, tone }) {
    return (_jsxs("div", { className: "flex flex-col gap-0.5 px-4 py-3", children: [_jsx("dt", { className: "text-xs text-fg-3", children: label }), _jsx("dd", { className: cn("m-0 text-xl font-semibold tracking-tight tabular", tone === "bad" ? "text-bad" : "text-fg"), children: value })] }));
}
/**
 * The Overview section once a crawl exists: live figures while it runs, then
 * why URLs were not crawled, robots.txt per site and sitemaps (spec 003 US2).
 * The URLs themselves are in the result tabs (spec 007).
 */
export function CrawlOverview({ run, onInfo, onResume, onSaveAs }) {
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
    return (_jsx("section", { "aria-label": "Overview", className: "h-full overflow-y-auto", children: _jsxs("div", { className: "mx-auto flex max-w-3xl flex-col gap-5 p-6", children: [_jsxs("header", { className: "flex flex-col gap-2", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx("h1", { className: "m-0 min-w-0 truncate text-lg font-semibold tracking-tight", children: run.target || "Crawl" }), info.tone === "accent" ? _jsx(Badge, { tone: "info", children: info.label }) : _jsx(Badge, { tone: info.tone, children: info.label }), _jsx("span", { className: "ml-auto text-sm text-fg-3 tabular", children: formatElapsed(run.elapsedMs) }), run.path && onInfo && (_jsx(Tooltip, { content: "Crawl information", children: _jsx(Button, { variant: "quiet", size: "sm", icon: true, "aria-label": "Crawl information", onClick: onInfo, children: _jsx(Info, { className: "size-3.5" }) }) }))] }), _jsx(Bar, { value: done, label: "URLs handled", tone: run.running ? "accent" : info.tone === "accent" ? "neutral" : info.tone, className: "w-full" }), run.path && (_jsxs("p", { className: "m-0 truncate text-sm text-fg-3", title: run.path, children: ["Saved to ", _jsx("span", { className: "font-code text-fg-2", "data-selectable": true, children: run.path })] }))] }), run.local && (_jsx(LocalErrorNotice, { error: run.local, onResume: () => onResume?.(), onSaveAs: () => onSaveAs?.(), onKeepPaused: run.dismissLocal })), run.error && (_jsx(Notice, { kind: "bad", role: "alert", children: run.error })), s?.pause_reason && _jsx(Notice, { kind: "warn", children: pauseReasons[s.pause_reason] ?? `SpiderLens paused this crawl (${s.pause_reason}).` }), s?.url_limit_reached && _jsx(Notice, { kind: "info", children: "The URL limit was reached: the remaining URLs are listed as \"Over URL limit\"." }), warnings.map((w) => (_jsx(Notice, { kind: "warn", children: w }, w))), notices.length > 0 && (_jsx("ul", { "aria-label": "Crawl notices", className: "m-0 flex list-none flex-col gap-1 p-0", children: notices.map((n) => (_jsx("li", { children: _jsx(Notice, { kind: "info", children: n }) }, n))) })), _jsxs("dl", { className: "m-0 grid grid-cols-5 divide-x divide-line overflow-hidden rounded-lg border border-line bg-surface", children: [_jsx(Figure, { label: "Discovered", value: count(discovered) }), _jsx(Figure, { label: "Crawled", value: count(crawled) }), _jsx(Figure, { label: "Not crawled", value: count(notCrawled) }), _jsx(Figure, { label: "Queued", value: count(queued) }), _jsx(Figure, { label: "Errors", value: count(errors), tone: errors ? "bad" : undefined })] }), run.report && run.report.reasons.length > 0 && (_jsx(Table, { title: "Not crawled", head: ["Reason", "URLs"], children: run.report.reasons.map((r) => (_jsxs("tr", { "data-reason": r.reason, children: [_jsx("td", { className: "px-3 py-1.5", children: r.label }), _jsx("td", { className: "px-3 py-1.5 text-right tabular", children: count(r.count) })] }, r.reason))) })), byClass.length > 0 && (_jsx(Table, { title: "Errors", head: ["Error", "URLs"], aside: `${count(s?.transient_errors)} temporary · ${count(s?.permanent_errors)} permanent`, children: byClass
                        .sort((a, b) => b[1] - a[1])
                        .map(([cls, n]) => (_jsxs("tr", { "data-class": cls, children: [_jsx("td", { className: "px-3 py-1.5", children: classLabel(cls, run.report) }), _jsx("td", { className: "px-3 py-1.5 text-right text-bad tabular", children: count(n) })] }, cls))) })), byClass.length === 0 && run.report?.failures && run.report.failures.length > 0 && (_jsx(Table, { title: "Failed without a response", head: ["Error", "URLs"], children: [...run.report.failures]
                        .sort((a, b) => b.count - a.count)
                        .map((f) => (_jsxs("tr", { "data-failure": f.class, children: [_jsx("td", { className: "px-3 py-1.5", children: f.label }), _jsx("td", { className: "px-3 py-1.5 text-right text-bad tabular", children: count(f.count) })] }, f.class))) })), s && retried && (_jsx(Table, { title: "Retries", head: ["What", "Count"], children: [
                        ["Automatic retries", s.retries],
                        ["URLs that worked after a retry", s.recovered],
                        ["Sites paused", s.hosts_paused],
                        ["Times you retried failed URLs", s.manual_retries],
                    ].map(([label, n]) => (_jsxs("tr", { children: [_jsx("td", { className: "px-3 py-1.5", children: label }), _jsx("td", { className: "px-3 py-1.5 text-right tabular", children: count(n) })] }, label))) })), s?.speed_events && s.speed_events.length > 0 && (_jsx(Table, { title: "Slowdowns", head: ["Site", "What slowed it down"], children: s.speed_events.map((e, i) => (_jsxs("tr", { "data-speed-host": e.host, children: [_jsx("td", { className: "px-3 py-1.5 font-code", title: e.host, children: siteName(e.host) }), _jsx("td", { className: "px-3 py-1.5 text-right", children: e.summary.replace(`${e.host}: `, "") })] }, `${e.host}-${i}`))) })), s && s.hosts.length > 0 && (_jsx(Table, { title: "robots.txt", head: ["Site", "robots.txt"], children: s.hosts.map((h) => (_jsxs("tr", { children: [_jsx("td", { className: "px-3 py-1.5 font-code", children: h.host }), _jsxs("td", { className: "px-3 py-1.5 text-right", children: [robotsOutcome[h.robots], h.crawl_delay_secs !== null ? `, crawl-delay ${h.crawl_delay_secs} s` : "", h.error ? ` (${h.error})` : ""] })] }, h.host))) })), s && s.sitemaps.length > 0 && (_jsx(Table, { title: "Sitemaps", head: ["Sitemap", "URLs"], children: s.sitemaps.map((m) => (_jsxs("tr", { children: [_jsx("td", { className: "max-w-0 truncate px-3 py-1.5 font-code", title: m.url, children: m.url }), _jsx("td", { className: cn("px-3 py-1.5 text-right tabular", m.error && "text-bad"), children: m.error ?? count(m.urls) })] }, m.url))) }))] }) }));
}
function Table({ title, head, aside, children }) {
    return (_jsxs("section", { "aria-label": title, className: "flex flex-col gap-1.5", children: [_jsxs("div", { className: "flex items-baseline gap-2 px-1", children: [_jsx("h2", { className: "m-0 text-xs font-semibold text-fg-3", children: title }), aside && (_jsx("span", { className: "ml-auto text-xs text-fg-3 tabular", "data-aside": true, children: aside }))] }), _jsx("div", { className: "overflow-hidden rounded-lg border border-line bg-surface", children: _jsxs("table", { className: "w-full table-fixed border-collapse text-sm", children: [_jsx("thead", { className: "sr-only", children: _jsxs("tr", { children: [_jsx("th", { children: head[0] }), _jsx("th", { children: head[1] })] }) }), _jsx("tbody", { className: "divide-y divide-line", children: children })] }) })] }));
}
/** Main area before any crawl (FR-008): what to do next, and anything to be careful about. */
export function StartState({ setup, error, onFocusUrl, onOpenSettings, library, onOpenRecent, }) {
    const mode = setup.config?.mode ?? "spider";
    const urlIssues = setup.issues.filter((i) => i.path === "input.start_url");
    return (_jsx("section", { "aria-label": "Start", className: "h-full overflow-y-auto", children: _jsxs("div", { className: "flex min-h-full flex-col items-center justify-center gap-5 p-8", children: [library && _jsx(StorageWarning, { storage: library.storage }), _jsxs("div", { className: "flex max-w-md flex-col items-center gap-2 text-center", children: [_jsx("h1", { className: "m-0 text-xl font-semibold tracking-tight text-balance", children: mode === "spider" ? "Crawl a website" : "Crawl a list of URLs" }), _jsx("p", { className: "m-0 text-base text-fg-2", children: mode === "spider"
                                ? "Enter a start URL above and press Start. SpiderLens follows links from there and lists every page it finds, with its issues."
                                : "Add your URLs, then press Start. Only the URLs in the list are crawled." })] }), error && (_jsx(Notice, { kind: "bad", role: "alert", className: "w-full max-w-md", children: error })), mode === "spider" && setup.startUrl !== "" && _jsx(IssueList, { issues: urlIssues }), setup.warnings.length > 0 && (_jsxs("div", { "aria-label": "Less respectful settings", className: "flex w-full max-w-md flex-col gap-2 rounded-lg border border-line bg-surface px-4 py-3", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx(Badge, { tone: "warn", children: "Less respectful settings" }), _jsx(Button, { size: "sm", variant: "quiet", className: "ml-auto", onClick: onOpenSettings, children: "Review" })] }), _jsx(IssueList, { issues: setup.warnings })] })), _jsxs("div", { className: "flex flex-wrap items-center justify-center gap-x-4 gap-y-1 text-sm text-fg-3", children: [_jsxs("button", { type: "button", className: "rounded-sm hover:text-fg-2", onClick: onFocusUrl, children: ["Start URL ", _jsx(Kbd, { keys: "Mod+L" })] }), _jsxs("span", { children: ["Settings ", _jsx(Kbd, { keys: "Mod+," })] }), _jsxs("span", { children: ["Commands ", _jsx(Kbd, { keys: "Mod+K" })] })] }), library && onOpenRecent && _jsx(RecentCrawls, { library: library, onOpen: onOpenRecent }), library && _jsx(UnsavedCrawls, { library: library })] }) }));
}
/** About (FR-007): version, engine and build, copyable. */
export function AboutDialog({ open, onOpenChange, transport }) {
    const [info, setInfo] = useState(null);
    const [error, setError] = useState(null);
    const [copied, setCopied] = useState(false);
    useEffect(() => {
        transport.buildInfo().then(setInfo, (e) => setError(String(e)));
    }, [transport]);
    const text = info ? `SpiderLens ${info.product_version} (engine ${info.engine_version}, build ${info.build_id})` : "";
    return (_jsxs(Modal, { open: open, onOpenChange: onOpenChange, title: "SpiderLens", description: "SEO crawler for Windows, macOS and Linux.", children: [error && (_jsxs(Notice, { kind: "bad", role: "alert", children: ["Could not read build information: ", error] })), info && (_jsxs("dl", { className: "m-0 grid grid-cols-2 gap-x-4 gap-y-1 text-sm", "data-selectable": true, children: [_jsx("dt", { className: "text-fg-3", children: "Version" }), _jsx("dd", { className: "m-0 tabular", "data-testid": "product-version", children: info.product_version }), _jsx("dt", { className: "text-fg-3", children: "Engine" }), _jsx("dd", { className: "m-0 tabular", "data-testid": "engine-version", children: info.engine_version }), _jsx("dt", { className: "text-fg-3", children: "Build" }), _jsx("dd", { className: "m-0 font-code", "data-testid": "build-id", children: info.build_id })] })), _jsxs("div", { className: "flex justify-end gap-2", children: [info && (_jsxs(Button, { onClick: () => navigator.clipboard?.writeText(text).then(() => setCopied(true), () => setCopied(false)), children: [_jsx(Copy, { className: "size-3.5" }), " ", copied ? "Copied" : "Copy"] })), _jsx(Button, { variant: "primary", onClick: () => onOpenChange(false), children: "Close" })] })] }));
}
