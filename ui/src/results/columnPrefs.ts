// Which columns each tab shows, their order and widths (FR-014), kept per
// machine like the rest of the layout. Storage can be missing or throw.

import type { TabDef } from "../generated/TabDef";

export interface ColumnPrefs {
  /** Visible column ids in display order. */
  order: string[];
  widths: Record<string, number>;
}

const KEY = "spiderlens.columns.v1";

function load(): Record<string, ColumnPrefs> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, ColumnPrefs>;
  } catch {
    return {};
  }
}

export function defaultPrefs(tab: TabDef): ColumnPrefs {
  return { order: tab.columns.filter((c) => c.default_visible).map((c) => c.id), widths: {} };
}

/** Saved prefs for a tab, dropping columns the tab no longer has. */
export function loadPrefs(tab: TabDef): ColumnPrefs {
  const saved = load()[tab.id];
  if (!saved) return defaultPrefs(tab);
  const ids = new Set(tab.columns.map((c) => c.id));
  const order = saved.order.filter((id) => ids.has(id));
  return { order: order.length ? order : defaultPrefs(tab).order, widths: saved.widths ?? {} };
}

export function savePrefs(tab: string, prefs: ColumnPrefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...load(), [tab]: prefs }));
  } catch {
    // Not fatal: the columns just aren't remembered.
  }
}
