import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useState } from "react";
import { FileQuestion } from "lucide-react";
import { Badge, Button, Modal, Notice, Tooltip } from "../components/ui";
import { formatSize, formatWhen } from "./format";
export const statusInfo = {
    running: { label: "Running", tone: "info" },
    paused: { label: "Paused", tone: "warn" },
    stopped: { label: "Stopped", tone: "neutral" },
    completed: { label: "Completed", tone: "ok" },
};
const title = (e) => e.name || e.start_url || e.path;
/** The badges an entry carries: its status, then Recovered and Unsaved. */
export function EntryBadges({ entry }) {
    const s = statusInfo[entry.status];
    return (_jsxs("span", { className: "flex shrink-0 items-center gap-1", children: [_jsx(Badge, { tone: s.tone, children: s.label }), entry.recovered && _jsx(Badge, { tone: "info", children: "Recovered" }), entry.unsaved && _jsx(Badge, { children: "Unsaved" })] }));
}
/** Recent crawls on the start screen (spec 006 US4): one click opens one. */
export function RecentCrawls({ library, onOpen }) {
    const [error, setError] = useState(null);
    if (library.recent.length === 0)
        return null;
    const act = (f) => () => {
        setError(null);
        f().catch((e) => setError(String(e)));
    };
    return (_jsxs("section", { "aria-labelledby": "recent-crawls", className: "flex w-full max-w-xl flex-col gap-1.5", children: [_jsx("h2", { id: "recent-crawls", className: "m-0 px-1 text-xs font-semibold text-fg-3", children: "Recent crawls" }), error && (_jsx(Notice, { kind: "bad", role: "alert", children: error })), _jsx("ul", { "aria-label": "Recent crawls", className: "m-0 flex list-none flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0", children: library.recent.map((e) => (_jsx("li", { "data-path": e.path, "data-available": e.available, children: e.available ? (_jsxs("button", { type: "button", onClick: () => onOpen(e.path), title: e.path, className: "flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-surface-2 focus-visible:bg-surface-2 focus-visible:outline-none", children: [_jsx(EntryText, { entry: e }), _jsx(EntryBadges, { entry: e })] })) : (_jsxs("div", { className: "flex items-center gap-3 px-3 py-2", children: [_jsx("div", { className: "flex min-w-0 flex-1 items-center gap-3 opacity-55", children: _jsx(EntryText, { entry: e }) }), _jsxs("span", { className: "flex shrink-0 items-center gap-1 text-xs text-fg-3", children: [_jsx(FileQuestion, { "aria-hidden": true, className: "size-3.5" }), " File not found"] }), _jsx(Button, { size: "sm", onClick: act(() => library.locate(e.path)), children: "Locate\u2026" }), _jsx(Tooltip, { content: "Removes it from this list. The file itself is not deleted.", children: _jsx(Button, { size: "sm", variant: "quiet", "aria-description": "Removes it from this list. The file itself is not deleted.", onClick: act(() => library.remove(e.path)), children: "Remove" }) })] })) }, e.path))) })] }));
}
function EntryText({ entry: e }) {
    return (_jsxs(_Fragment, { children: [_jsxs("span", { className: "flex min-w-0 flex-1 flex-col", children: [_jsx("span", { className: "truncate text-base font-medium text-fg", children: title(e) }), e.start_url && e.start_url !== title(e) && _jsx("span", { className: "truncate font-code text-xs text-fg-3", children: e.start_url })] }), _jsxs("span", { className: "flex shrink-0 flex-col items-end text-xs text-fg-3 tabular", children: [_jsx("span", { children: formatWhen(e.last_used) }), _jsxs("span", { children: [e.urls.toLocaleString(), " URLs"] })] })] }));
}
/** Crawls kept in the app's own folder until saved somewhere, by size, so space can be freed (spec 006 edge cases). */
export function UnsavedCrawls({ library, openPath = null }) {
    const [confirm, setConfirm] = useState(null);
    const [error, setError] = useState(null);
    const u = library.unsaved;
    if (!u || u.entries.length === 0)
        return null;
    const remove = async (e) => {
        setConfirm(null);
        try {
            await library.deleteUnsaved(e.path);
        }
        catch (err) {
            setError(String(err));
        }
    };
    return (_jsxs("section", { "aria-labelledby": "unsaved-crawls", className: "flex w-full max-w-xl flex-col gap-1.5", children: [_jsxs("h2", { id: "unsaved-crawls", className: "m-0 flex items-baseline gap-2 px-1 text-xs font-semibold text-fg-3", children: ["Unsaved crawls", _jsxs("span", { className: "ml-auto font-normal tabular", "data-testid": "unsaved-total", children: ["Unsaved crawls use ", formatSize(u.total_bytes)] })] }), error && (_jsx(Notice, { kind: "bad", role: "alert", children: error })), _jsx("ul", { "aria-label": "Unsaved crawls", className: "m-0 flex list-none flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0", children: u.entries.map((e) => (_jsxs("li", { "data-path": e.path, className: "flex items-center gap-3 px-3 py-1.5 text-sm", children: [_jsx("span", { className: "min-w-0 flex-1 truncate", title: e.path, children: title(e) }), _jsx("span", { className: "shrink-0 text-fg-3 tabular", children: formatSize(u.size_bytes[e.path] ?? 0) }), _jsx(Button, { size: "sm", variant: "destructive", disabled: e.path === openPath, onClick: () => setConfirm(e), children: "Delete\u2026" })] }, e.path))) }), confirm && (_jsx(Modal, { open: true, onOpenChange: (o) => !o && setConfirm(null), title: `Delete ${title(confirm)}? This can't be undone.`, description: "Its results are removed from this computer.", children: _jsxs("div", { className: "flex justify-end gap-2", children: [_jsx(Button, { onClick: () => setConfirm(null), children: "Cancel" }), _jsx(Button, { variant: "destructive", onClick: () => void remove(confirm), children: "Delete" })] }) }))] }));
}
/** Little space left where unsaved crawls are kept. */
export function StorageWarning({ storage }) {
    if (!storage?.low)
        return null;
    return (_jsxs(Notice, { kind: "warn", role: "alert", className: "w-full max-w-xl rounded-lg border border-line bg-surface px-3 py-2", children: [storage.free_bytes !== null
                ? `Only ${formatSize(storage.free_bytes)} free on the drive for unsaved crawls (keep at least ${formatSize(storage.reserve_bytes)}).`
                : `Little space is free on the drive for unsaved crawls (keep at least ${formatSize(storage.reserve_bytes)}).`, " ", "Save crawls elsewhere or delete ones you no longer need."] }));
}
