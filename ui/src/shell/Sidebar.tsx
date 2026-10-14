import { Activity, CircleHelp, ExternalLink, FileText, Globe, Heading1, Heading2, Image, Link2, ListOrdered, Bot, Type, Code, Ban, Sparkles, type LucideIcon } from "lucide-react";
import { cn, Tooltip } from "../components/ui";
import type { IssuesSummary } from "../generated/IssuesSummary";
import { IssuesNav } from "../issues/IssuesNav";

export interface NavItem {
  id: string;
  title: string;
  icon: LucideIcon;
  /** Milestone it arrives in, when not available yet. */
  later?: string;
}

export const resultTabs: NavItem[] = [
  { id: "internal", title: "Internal", icon: Globe },
  { id: "external", title: "External", icon: ExternalLink },
  { id: "response-codes", title: "Response codes", icon: ListOrdered },
  { id: "url", title: "URL", icon: Link2 },
  { id: "page-titles", title: "Page titles", icon: Type },
  { id: "meta-description", title: "Meta description", icon: FileText },
  { id: "h1", title: "H1", icon: Heading1 },
  { id: "h2", title: "H2", icon: Heading2 },
  { id: "images", title: "Images", icon: Image },
  { id: "canonicals", title: "Canonicals", icon: Code },
  { id: "directives", title: "Directives", icon: Ban },
];

const later: NavItem[] = [
  { id: "ai-crawlers", title: "AI crawlers", icon: Bot, later: "M4" },
  { id: "llms-txt", title: "llms.txt", icon: Sparkles, later: "M4" },
];

/** Where you are (FR-003). Result tabs open once a crawl exists (spec 007). */
export function Sidebar({
  active,
  onSelect,
  collapsed,
  onAbout,
  results,
  issues = null,
}: {
  active: string;
  onSelect: (id: string) => void;
  collapsed: boolean;
  onAbout: () => void;
  results: boolean;
  /** The open crawl's issues (spec 009); the group shows once a crawl exists. */
  issues?: IssuesSummary | null;
}) {
  const item = (it: NavItem, available: boolean, run = () => onSelect(it.id)) => {
    const on = it.id === active;
    const Icon = it.icon;
    const btn = (
      <button
        type="button"
        aria-current={on ? "page" : undefined}
        aria-disabled={!available || undefined}
        aria-label={collapsed ? it.title : undefined}
        onClick={() => available && run()}
        className={cn(
          "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base",
          on ? "bg-selection text-fg" : available ? "text-fg-2 hover:bg-surface-2" : "text-fg-3",
          collapsed && "justify-center px-0",
        )}
      >
        <Icon aria-hidden className={cn("size-4 shrink-0", on ? "text-accent-text" : "opacity-80")} />
        {!collapsed && <span className="min-w-0 flex-1 truncate">{it.title}</span>}
        {!collapsed && it.later && <span className="rounded-sm border border-line px-1 text-2xs text-fg-3">{it.later}</span>}
      </button>
    );
    return <li key={it.id}>{collapsed ? <Tooltip content={it.title}>{btn}</Tooltip> : btn}</li>;
  };
  const heading = (t: string) => (collapsed ? <div className="mx-2 my-2 h-px bg-line" /> : <div className="px-2 pt-3 pb-1 text-xs font-semibold text-fg-3">{t}</div>);

  return (
    <nav aria-label="Sections" data-material-region className="flex h-full flex-col overflow-y-auto bg-sidebar px-2 py-2">
      <ul className="m-0 list-none p-0">{item({ id: "overview", title: "Overview", icon: Activity }, true)}</ul>
      {results && (
        <>
          {heading("Issues")}
          <IssuesNav summary={issues} active={active} collapsed={collapsed} onSelect={onSelect} />
        </>
      )}
      {heading("Crawl results")}
      <ul className="m-0 flex list-none flex-col gap-px p-0">{resultTabs.map((t) => item(t, results))}</ul>
      {heading("AI visibility")}
      <ul className="m-0 flex list-none flex-col gap-px p-0">{later.map((t) => item(t, false))}</ul>
      <div className="mt-auto pt-2">
        <ul className="m-0 list-none p-0">{item({ id: "about", title: "About SpiderLens", icon: CircleHelp }, true, onAbout)}</ul>
      </div>
    </nav>
  );
}
