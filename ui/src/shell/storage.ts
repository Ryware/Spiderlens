// Per-machine UI state (research R7): appearance and layout, one versioned key.
// Storage can be missing or throw; defaults apply then.

import { type Appearance, isAccent, isLevel, toneOf } from "./appearance";

export interface UiState extends Appearance {
  v: 2;
  sidebarWidth: number;
  detailWidth: number;
  sidebarCollapsed: boolean;
  detailCollapsed: boolean;
  settingsCategory: string;
  recentCommands: string[];
}

export const defaults: UiState = {
  v: 2,
  followSystem: true,
  level: "dark",
  systemLight: "light",
  systemDark: "dark",
  accent: "teal",
  sidebarWidth: 220,
  detailWidth: 300,
  sidebarCollapsed: false,
  detailCollapsed: false,
  settingsCategory: "speed",
  recentCommands: [],
};

const KEY = "spiderlens.ui";

/** Version 1 stored `theme: "system" | "light" | "dark"`; carry it across. */
function fromV1(old: Record<string, unknown>): Partial<UiState> {
  const { theme, v: _v, ...rest } = old;
  const appearance: Partial<UiState> =
    theme === "light" || theme === "dark" ? { followSystem: false, level: theme } : { followSystem: true };
  return { ...rest, ...appearance };
}

/**
 * Unknown values (a newer build's step or palette, a hand-edited file) fall
 * back to the default for that field only.
 */
function clean(s: UiState): UiState {
  const out = { ...s };
  if (typeof out.followSystem !== "boolean") out.followSystem = defaults.followSystem;
  if (!isLevel(out.level)) out.level = defaults.level;
  if (!isLevel(out.systemLight) || toneOf(out.systemLight) !== "light") out.systemLight = defaults.systemLight;
  if (!isLevel(out.systemDark) || toneOf(out.systemDark) !== "dark") out.systemDark = defaults.systemDark;
  if (!isAccent(out.accent)) out.accent = defaults.accent;
  return out;
}

export function loadUiState(): UiState {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...defaults };
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (parsed.v === 2) return clean({ ...defaults, ...(parsed as Partial<UiState>), v: 2 });
    if (parsed.v === 1) return clean({ ...defaults, ...fromV1(parsed), v: 2 });
    return { ...defaults };
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
