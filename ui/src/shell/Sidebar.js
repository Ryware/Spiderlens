import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { Activity, CircleHelp, ExternalLink, FileText, Globe, Heading1, Heading2, Image, Link2, ListOrdered, Bot, Type, Code, Ban, Sparkles } from "lucide-react";
import { cn, Tooltip } from "../components/ui";
export const resultTabs = [
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
const later = [
    { id: "ai-crawlers", title: "AI crawlers", icon: Bot, later: "M4" },
    { id: "llms-txt", title: "llms.txt", icon: Sparkles, later: "M4" },
];
/** Where you are (FR-003). Result tabs open once a crawl exists (spec 007). */
export function Sidebar({ active, onSelect, collapsed, onAbout, results }) {
    const item = (it, available, run = () => onSelect(it.id)) => {
        const on = it.id === active;
        const Icon = it.icon;
        const btn = (_jsxs("button", { type: "button", "aria-current": on ? "page" : undefined, "aria-disabled": !available || undefined, "aria-label": collapsed ? it.title : undefined, onClick: () => available && run(), className: cn("flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base", on ? "bg-selection text-fg" : available ? "text-fg-2 hover:bg-surface-2" : "text-fg-3", collapsed && "justify-center px-0"), children: [_jsx(Icon, { "aria-hidden": true, className: cn("size-4 shrink-0", on ? "text-accent-text" : "opacity-80") }), !collapsed && _jsx("span", { className: "min-w-0 flex-1 truncate", children: it.title }), !collapsed && it.later && _jsx("span", { className: "rounded-sm border border-line px-1 text-2xs text-fg-3", children: it.later })] }));
        return _jsx("li", { children: collapsed ? _jsx(Tooltip, { content: it.title, children: btn }) : btn }, it.id);
    };
    const heading = (t) => (collapsed ? _jsx("div", { className: "mx-2 my-2 h-px bg-line" }) : _jsx("div", { className: "px-2 pt-3 pb-1 text-xs font-semibold text-fg-3", children: t }));
    return (_jsxs("nav", { "aria-label": "Sections", "data-material-region": true, className: "flex h-full flex-col overflow-y-auto bg-sidebar px-2 py-2", children: [_jsx("ul", { className: "m-0 list-none p-0", children: item({ id: "overview", title: "Overview", icon: Activity }, true) }), heading("Crawl results"), _jsx("ul", { className: "m-0 flex list-none flex-col gap-px p-0", children: resultTabs.map((t) => item(t, results)) }), heading("AI visibility"), _jsx("ul", { className: "m-0 flex list-none flex-col gap-px p-0", children: later.map((t) => item(t, false)) }), _jsx("div", { className: "mt-auto pt-2", children: _jsx("ul", { className: "m-0 list-none p-0", children: item({ id: "about", title: "About SpiderLens", icon: CircleHelp }, true, onAbout) }) })] }));
}
