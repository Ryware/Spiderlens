import type { FilterCounts } from "../generated/FilterCounts";
import type { TabDef } from "../generated/TabDef";
import { Chip, Tooltip } from "../components/ui";

/**
 * The tab's filters with live counts (FR-010, FR-011). A filter the crawl
 * file has no data for stays visible, disabled, with the reason.
 */
export function FilterStrip({ tab, active, counts, onSelect }: { tab: TabDef; active: string; counts: FilterCounts | null; onSelect: (id: string) => void }) {
  const mine = counts?.tab === tab.id ? counts : null;
  return (
    <div role="toolbar" aria-label="Filters" className="flex min-w-0 items-center gap-0.5 overflow-x-auto">
      {tab.filters.map((f) => {
        const reason = mine?.unavailable[f.id];
        const chip = (
          <Chip
            key={f.id}
            label={f.label}
            count={reason ? null : (mine?.counts[f.id] ?? null)}
            active={f.id === active}
            disabled={!!reason}
            aria-disabled={reason ? true : undefined}
            onClick={() => onSelect(f.id)}
            className={reason ? "cursor-default text-fg-3 hover:bg-transparent" : undefined}
            data-filter={f.id}
          />
        );
        return reason ? (
          <Tooltip key={f.id} content={reason}>
            <span>{chip}</span>
          </Tooltip>
        ) : (
          chip
        );
      })}
    </div>
  );
}
