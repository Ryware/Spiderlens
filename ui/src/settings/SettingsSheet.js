import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useRef } from "react";
import { Button, cn, Kbd, Notice, Sheet, SheetClose } from "../components/ui";
import { categories, CategoryBody, issuesFor, liveCategories } from "./categories";
import { IssueList } from "./fields";
import { ProfilePicker } from "./ProfilePicker";
import { AppearanceSettings } from "./AppearanceSettings";
/**
 * Crawl settings as a sheet over the window (US2): profile and categories on
 * the left, the chosen category as grouped rows on the right. Behaviour and
 * validation are spec 002's, unchanged (FR-013).
 *
 * While a crawl is open the settings are locked, except Speed and retries
 * while it runs: those can be changed and sent to it with "Apply to running
 * crawl" (spec 004 D13).
 */
export function SettingsSheet({ open, onOpenChange, setup, run, transport, category, onCategory, appearance, level, systemDark, onAppearance, }) {
    const navRef = useRef(null);
    const mode = setup.config?.mode ?? "spider";
    const visible = categories.filter((c) => !c.listOnly || mode === "list");
    const groups = [visible.filter((c) => !c.app), visible.filter((c) => c.app)];
    const current = visible.find((c) => c.id === category) ?? visible.find((c) => c.id === "speed");
    // Issues on a whole category (for example "speed") rather than one setting.
    const general = setup.issues.filter((i) => current.prefixes.includes(i.path));
    const running = run.phase === "running";
    const live = running && liveCategories.includes(current.id);
    const locked = run.active && !live && !current.app;
    const { config } = setup;
    const changed = !!config && (!run.applied || JSON.stringify([config.speed, config.retry]) !== JSON.stringify([run.applied.speed, run.applied.retry]));
    const liveErrors = setup.errors.some((e) => /^(speed|retry)(\.|$)/.test(e.path));
    const lockNote = running
        ? "Locked while the crawl runs. Speed and retry settings can still be changed."
        : "Locked while a crawl is open. Settings can change again once it is stopped or finished.";
    return (_jsx(Sheet, { open: open, onOpenChange: onOpenChange, title: "Crawl settings", className: "h-160 w-190 sheet-max-w", initialFocus: navRef, children: _jsxs("div", { className: "flex min-h-0 flex-1", children: [_jsxs("nav", { "aria-label": "Settings categories", className: "flex w-56 shrink-0 flex-col gap-3 border-r border-line bg-sidebar p-3", children: [_jsxs("div", { className: "flex flex-col gap-1.5", children: [_jsx("div", { className: "px-1 text-xs font-semibold text-fg-3", children: "Profile" }), _jsx("fieldset", { disabled: run.active, className: "m-0 min-w-0 border-0 p-0", children: _jsx(ProfilePicker, { transport: transport, setup: setup }) })] }), _jsx("div", { role: "tablist", "aria-orientation": "vertical", className: "flex flex-col gap-3", children: groups.map((group, g) => (_jsxs("ul", { className: "m-0 flex list-none flex-col gap-0.5 p-0", role: "presentation", children: [g === 1 && _jsx("li", { role: "presentation", className: "px-2 pb-1 text-xs font-semibold text-fg-3", children: "App" }), group.map((c) => {
                                        const iss = issuesFor(setup, c);
                                        const errors = iss.filter((i) => i.severity === "error").length;
                                        const warnings = iss.length - errors;
                                        const on = c.id === current.id;
                                        return (_jsx("li", { role: "presentation", children: _jsxs("button", { ref: on ? navRef : undefined, type: "button", role: "tab", "aria-selected": on, onClick: () => onCategory(c.id), className: cn("flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base", on ? "bg-selection text-fg" : "text-fg-2 hover:bg-surface-2"), children: [_jsx("span", { className: "min-w-0 flex-1 truncate", children: c.title }), errors > 0 && _jsx("span", { className: "text-xs font-medium text-bad tabular", children: errors === 1 ? "1 error" : `${errors} errors` }), errors === 0 && warnings > 0 && (_jsx("span", { className: "text-xs font-medium text-warn tabular", children: warnings === 1 ? "1 warning" : `${warnings} warnings` }))] }) }, c.id));
                                    })] }, g))) })] }), _jsxs("div", { className: "flex min-w-0 flex-1 flex-col", role: "tabpanel", "aria-label": current.title, children: [_jsxs("header", { className: "flex flex-col gap-0.5 px-6 pt-5 pb-3", children: [_jsx("h2", { className: "m-0 text-md font-semibold", children: current.title }), _jsx("p", { className: "m-0 text-sm text-fg-3", children: current.description })] }), _jsxs("div", { className: "flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-4", children: [locked && _jsx(Notice, { kind: "info", children: lockNote }), live && current.id === "network" && _jsx(Notice, { kind: "info", children: "Retry settings can be applied to the running crawl. Network settings stay as they are until it ends." }), _jsx(IssueList, { issues: general }), current.id === "appearance" ? (_jsx(AppearanceSettings, { appearance: appearance, level: level, systemDark: systemDark, onChange: onAppearance })) : (_jsx("fieldset", { disabled: locked, className: "m-0 flex min-w-0 flex-col gap-4 border-0 p-0", children: _jsx(CategoryBody, { id: current.id, setup: setup, transport: transport, live: live }) }))] }), _jsxs("footer", { className: "flex items-center gap-2 border-t border-line px-6 py-3", children: [_jsx("span", { className: "mr-auto text-sm text-fg-3", role: live && !current.app ? "status" : undefined, children: current.app ? "Saved on this computer" : live ? (changed ? "Not applied to the running crawl yet" : "In use by the running crawl") : "Checked by the engine as you type" }), live && !current.app && config && (_jsx(Button, { disabled: !changed || liveErrors, onClick: () => void run.applySettings(config.speed, config.retry), children: "Apply to running crawl" })), _jsxs(SheetClose, { render: _jsx(Button, { variant: "primary" }), children: ["Done ", _jsx(Kbd, { keys: "Escape", onAccent: true })] })] })] })] }) }));
}
