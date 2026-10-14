import { jsx as _jsx } from "react/jsx-runtime";
import { Chip, Tooltip } from "../components/ui";
/**
 * The tab's filters with live counts (FR-010, FR-011). A filter the crawl
 * file has no data for stays visible, disabled, with the reason.
 */
export function FilterStrip({ tab, active, counts, onSelect }) {
    const mine = counts?.tab === tab.id ? counts : null;
    return (_jsx("div", { role: "toolbar", "aria-label": "Filters", className: "flex min-w-0 flex-wrap items-center gap-0.5", children: tab.filters.map((f) => {
            const reason = mine?.unavailable[f.id];
            const chip = (_jsx(Chip, { label: f.label, count: reason ? null : (mine?.counts[f.id] ?? null), active: f.id === active, disabled: !!reason, "aria-disabled": reason ? true : undefined, onClick: () => onSelect(f.id), className: reason ? "cursor-default text-fg-3 hover:bg-transparent" : undefined, "data-filter": f.id }, f.id));
            return reason ? (_jsx(Tooltip, { content: reason, children: _jsx("span", { children: chip }) }, f.id)) : (chip);
        }) }));
}
