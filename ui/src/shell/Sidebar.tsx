import { Activity, Gauge, CircleHelp, ScrollText, FolderTree, GitFork, ExternalLink, FileText, FileStack, SpellCheck, Zap, Globe, Heading1, Heading2, Image, Link2, ListOrdered, Bot, Type, Code, Ban, Sparkles, Files, GitCompareArrows, Network, Waypoints, TextQuote, CalendarClock, Cookie, ListTree, Map as MapIcon, Search, Cable, Braces, TextSearch, type LucideIcon } from "lucide-react";
import { cn, Tooltip } from "../components/ui";
import type { IssuesSummary } from "../generated/IssuesSummary";
import { IssuesNav } from "../issues/IssuesNav";
import { formatCompact } from "../results/format";

export interface NavItem {
  id: string;
  title: string;
  icon: LucideIcon;
  /** Milestone it arrives in, when not available yet. */
  later?: string;
}

export const resultTabs: NavItem[] = [
  { id: "internal", title: "Internal", icon: Globe },
  { id: "links", title: "Links", icon: Network },
  { id: "external", title: "External", icon: ExternalLink },
  { id: "response-codes", title: "Response codes", icon: ListOrdered },
  { id: "url", title: "URL", icon: Link2 },
  { id: "page-titles", title: "Page titles", icon: Type },
  { id: "meta-description", title: "Meta description", icon: FileText },
  { id: "h1", title: "H1", icon: Heading1 },
  { id: "h2", title: "H2", icon: Heading2 },
  { id: "images", title: "Images", icon: Image },
  { id: "pdfs", title: "PDFs", icon: FileStack },
  { id: "spelling", title: "Spelling", icon: SpellCheck },
  { id: "canonicals", title: "Canonicals", icon: Code },
  { id: "directives", title: "Directives", icon: Ban },
  { id: "content", title: "Content", icon: Files },
  { id: "amp", title: "AMP", icon: Zap },
  { id: "agents", title: "Agents", icon: Bot },
  { id: "search-console", title: "Search Console", icon: Search },
  { id: "sitemaps", title: "Sitemaps", icon: MapIcon },
  { id: "sitemap-files", title: "Sitemap files", icon: ListTree },
];

/** Spec 022: pictures of the crawl, built from the same crawl file. */
export const vizTabs: NavItem[] = [
  { id: "crawl-tree", title: "Crawl tree", icon: GitFork },
  { id: "directory-tree", title: "Directory tree", icon: FolderTree },
  { id: "link-graph", title: "Link graph", icon: Waypoints },
  { id: "anchor-text", title: "Anchor text", icon: TextQuote },
];
/** Reports with their own view rather than a results table (spec 025). */
export const reportTabs: NavItem[] = [{ id: "cookies", title: "Cookies", icon: Cookie }];

const later: NavItem[] = [
  { id: "ai-crawlers", title: "AI crawlers", icon: Bot, later: "M4" },
  { id: "llms-txt", title: "llms.txt", icon: Sparkles, later: "M4" },
];

/** Where you are (FR-003). Result tabs open once a crawl exists (spec 007). */
export const customIcon = (id: string): LucideIcon => (id === "custom_search" ? TextSearch : Braces);
export const reportItem: NavItem = { id: "report", title: "Report", icon: Gauge };

export function Sidebar({
  active,
  onSelect,
  collapsed,
  onAbout,
  results,
  issues = null,
  counting = false,
  custom = [],
  counts,
  comparison = false,
}: {
  active: string;
  onSelect: (id: string) => void;
  collapsed: boolean;
  onAbout: () => void;
  results: boolean;
  /** The open crawl's issues (spec 009); the group shows once a crawl exists. */
  issues?: IssuesSummary | null;
  /** The counts are partial: the issues are still being evaluated. */
  counting?: boolean;
  /** The open crawl's custom extraction and search tabs (spec 013), when it ran rules. */
  custom?: { id: string; title: string }[];
  /** URLs per result tab; absent while unknown (T014). */
  counts?: Record<string, number> | null;
  comparison?: boolean;
}) {
  const item = (it: NavItem, available: boolean, run = () => onSelect(it.id)) => {
    const on = it.id === active;
    const Icon = it.icon;
    const count = available && !collapsed ? counts?.[it.id] : undefined;
    const countId = `sidebar-count-${it.id}`;
    const btn = (
      <button
        type="button"
        aria-current={on ? "page" : undefined}
        aria-disabled={!available || undefined}
        aria-label={collapsed ? it.title : undefined}
        aria-describedby={count !== undefined ? countId : undefined}
        onClick={() => available && run()}
        className={cn(
          "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base",
          on ? "bg-selection text-fg" : available ? "text-fg-2 hover:bg-surface-2" : "text-fg-3",
          collapsed && "justify-center px-0",
        )}
      >
        <Icon aria-hidden className={cn("size-4 shrink-0", on ? "text-accent-text" : "opacity-80")} />
        {!collapsed && <span className="min-w-0 flex-1 truncate">{it.title}</span>}
        {count !== undefined && (
          <span aria-hidden title={count.toLocaleString()} className="shrink-0 text-xs text-fg-3 tabular">
            {formatCompact(count)}
          </span>
        )}
        {!collapsed && it.later && <span className="rounded-sm border border-line px-1 text-2xs text-fg-3">{it.later}</span>}
      </button>
    );
    return (
      <li key={it.id}>
        {collapsed ? <Tooltip content={it.title}>{btn}</Tooltip> : btn}
        {/* Outside the button so the count describes it without renaming it. */}
        {count !== undefined && (
          <span id={countId} className="sr-only">
            {count === 1 ? "1 URL" : `${count.toLocaleString()} URLs`}
          </span>
        )}
      </li>
    );
  };
  const heading = (t: string) => (collapsed ? <div className="mx-2 my-2 h-px bg-line" /> : <div className="px-2 pt-3 pb-1 text-xs font-semibold text-fg-3">{t}</div>);

  return (
    <nav aria-label="Sections" data-material-region className="relative flex h-full flex-col overflow-y-auto bg-sidebar px-2 py-2">
      <ul className="m-0 flex list-none flex-col gap-px p-0">
        {item({ id: "overview", title: "Overview", icon: Activity }, true)}
        {item(reportItem, results)}
        {/* Spec 012: the open comparison, alongside the crawl. */}
        {comparison && item({ id: "comparison", title: "Comparison", icon: GitCompareArrows }, true)}
        {item({ id: "schedules", title: "Schedules", icon: CalendarClock }, true)}
        {item({ id: "log", title: "Crawl log", icon: ScrollText }, results)}
      </ul>
      {results && (
        <>
          {heading("Issues")}
          <IssuesNav summary={issues} counting={counting} active={active} collapsed={collapsed} onSelect={onSelect} />
        </>
      )}
      {heading("Crawl results")}
      <ul className="m-0 flex list-none flex-col gap-px p-0">{[...resultTabs, ...reportTabs].map((t) => item(t, results))}</ul>
      {results && custom.length > 0 && (
        <>
          {heading("Custom")}
          <ul className="m-0 flex list-none flex-col gap-px p-0">{custom.map((t) => item({ id: t.id, title: t.title, icon: customIcon(t.id) }, true))}</ul>
        </>
      )}
      {heading("Visualisations")}
      <ul className="m-0 flex list-none flex-col gap-px p-0">{vizTabs.map((t) => item(t, results))}</ul>
      {heading("External data")}
      <ul className="m-0 list-none p-0">{item({ id: "integrations", title: "Integrations", icon: Cable }, true)}</ul>

      {heading("AI visibility")}
      <ul className="m-0 flex list-none flex-col gap-px p-0">{later.map((t) => item(t, false))}</ul>
      <div className="mt-auto pt-2">
        <ul className="m-0 list-none p-0">{item({ id: "about", title: "About SpiderLens", icon: CircleHelp }, true, onAbout)}</ul>
      </div>
    </nav>
  );
}
