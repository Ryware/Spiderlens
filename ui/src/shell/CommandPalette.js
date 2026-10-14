import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useRef, useState } from "react";
import { Command as Cmdk } from "cmdk";
import { Dialog } from "@base-ui/react/dialog";
import { Search } from "lucide-react";
import { cn, Kbd } from "../components/ui";
/** ⌘K / Ctrl+K palette: every destination and action, recent ones first (US4). */
export function CommandPalette({ open, onOpenChange, commands, recent, onRun, }) {
    const input = useRef(null);
    // Every opening starts empty: cleared as the palette closes, and again as it
    // opens in case it reopens during the closing animation.
    const [search, setSearch] = useState("");
    useEffect(() => {
        if (open)
            setSearch("");
    }, [open]);
    const setOpen = (o) => {
        if (!o)
            setSearch("");
        onOpenChange(o);
    };
    const available = commands.filter((c) => c.enabled !== false || c.reason);
    const recentCommands = recent.map((id) => available.find((c) => c.id === id)).filter((c) => !!c);
    const groups = ["Crawl", "Go to", "View", "Help"];
    const item = (c, prefix = "") => (_jsxs(Cmdk.Item, { value: prefix + c.id + " " + c.title, keywords: c.keywords, disabled: c.enabled === false, onSelect: () => {
            setOpen(false);
            onRun(c);
        }, className: "flex h-8 items-center gap-2 rounded-md px-2.5 text-base text-fg data-[disabled=true]:text-fg-3 data-[selected=true]:bg-selection", children: [_jsx("span", { className: "truncate", children: c.title }), _jsx("span", { className: "min-w-0 flex-1 truncate text-sm text-fg-3", children: c.enabled === false ? c.reason : null }), c.shortcut && _jsx(Kbd, { keys: c.shortcut, className: "border-current text-current opacity-70" })] }, prefix + c.id));
    return (_jsx(Dialog.Root, { open: open, onOpenChange: setOpen, children: _jsxs(Dialog.Portal, { children: [_jsx(Dialog.Backdrop, { className: "fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur-fast) data-ending-style:opacity-0 data-starting-style:opacity-0" }), _jsxs(Dialog.Popup, { initialFocus: input, "aria-label": "Command palette", className: cn("fixed top-20 left-1/2 z-40 w-140 sheet-max-w -translate-x-1/2 overflow-hidden rounded-xl bg-surface text-fg shadow-sheet outline-none", "transition-[opacity,scale] duration-(--dur-fast) data-ending-style:scale-98 data-ending-style:opacity-0 data-starting-style:scale-98 data-starting-style:opacity-0"), children: [_jsx(Dialog.Title, { className: "sr-only", children: "Command palette" }), _jsxs(Cmdk, { label: "Commands", loop: true, children: [_jsxs("div", { className: "flex items-center gap-2 border-b border-line px-3", children: [_jsx(Search, { "aria-hidden": true, className: "size-4 text-fg-3" }), _jsx(Cmdk.Input, { ref: input, value: search, onValueChange: setSearch, placeholder: "Type a command or a tab name", className: "h-11 flex-1 bg-transparent text-md text-fg outline-none placeholder:text-fg-3" })] }), _jsxs(Cmdk.List, { className: "max-h-90 overflow-y-auto p-1.5", children: [_jsx(Cmdk.Empty, { className: "px-3 py-6 text-center text-sm text-fg-3", children: "No matching command" }), recentCommands.length > 0 && (_jsx(Cmdk.Group, { heading: "Recent", className: "[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-fg-3", children: recentCommands.map((c) => item(c, "recent:")) })), groups.map((g) => {
                                            const cs = available.filter((c) => c.group === g);
                                            return cs.length ? (_jsx(Cmdk.Group, { heading: g, className: "[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-fg-3", children: cs.map((c) => item(c)) }, g)) : null;
                                        })] })] })] })] }) }));
}
