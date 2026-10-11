import { useCallback, useEffect, useState } from "react";
import { loadUiState, saveUiState, type ThemePref, type UiState } from "./storage";

/** UI state that survives restarts (FR-004, FR-015). */
export function useUiState() {
  const [state, setState] = useState<UiState>(loadUiState);
  useEffect(() => saveUiState(state), [state]);
  const update = useCallback((patch: Partial<UiState>) => setState((s) => ({ ...s, ...patch })), []);
  return [state, update] as const;
}

const query = "(prefers-color-scheme: dark)";

function systemDark(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia(query).matches;
}

export function resolveTheme(pref: ThemePref, dark = systemDark()): "light" | "dark" {
  return pref === "system" ? (dark ? "dark" : "light") : pref;
}

/** Applies the theme to <html> and follows the system live when set to System. */
export function useTheme(pref: ThemePref): "light" | "dark" {
  const [dark, setDark] = useState(systemDark);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(query);
    const on = () => setDark(mq.matches);
    mq.addEventListener?.("change", on);
    return () => mq.removeEventListener?.("change", on);
  }, []);
  const resolved = resolveTheme(pref, dark);
  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
  }, [resolved]);
  return resolved;
}
