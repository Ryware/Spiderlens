// Per-machine UI state (research R7): theme and layout, one versioned key.
// Storage can be missing or throw; defaults apply then.

export type ThemePref = "system" | "light" | "dark";

export interface UiState {
  v: 1;
  theme: ThemePref;
  sidebarWidth: number;
  detailWidth: number;
  sidebarCollapsed: boolean;
  detailCollapsed: boolean;
  settingsCategory: string;
  recentCommands: string[];
}

export const defaults: UiState = {
  v: 1,
  theme: "system",
  sidebarWidth: 220,
  detailWidth: 300,
  sidebarCollapsed: false,
  detailCollapsed: false,
  settingsCategory: "speed",
  recentCommands: [],
};

const KEY = "spiderlens.ui";

export function loadUiState(): UiState {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...defaults };
    const parsed = JSON.parse(raw) as Partial<UiState>;
    return parsed.v === 1 ? { ...defaults, ...parsed } : { ...defaults };
  } catch {
    return { ...defaults };
  }
}

export function saveUiState(s: UiState): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // Not fatal: the layout just isn't remembered.
  }
}
