import { useCallback, useEffect, useState } from "react";
import { applyAppearance, resolveLevel, toneOf } from "./appearance";
import { materialAvailable } from "./platform";
import { loadUiState, saveUiState } from "./storage";
/** UI state that survives restarts (FR-004, FR-015). */
export function useUiState() {
    const [state, setState] = useState(loadUiState);
    useEffect(() => saveUiState(state), [state]);
    const update = useCallback((patch) => setState((s) => ({ ...s, ...patch })), []);
    return [state, update];
}
const query = "(prefers-color-scheme: dark)";
function systemDark() {
    return typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia(query).matches;
}
/** Sets the step and palette before React's first render, so the first frame is right (spec 018 FR-014). */
export function applyStoredAppearance() {
    const s = loadUiState();
    applyAppearance(document.documentElement, resolveLevel(s, systemDark()), s.accent, s.translucent && materialAvailable());
}
/** The native title bar and window controls follow the step's tone (FR-016). Outside Tauri this does nothing. */
async function setWindowTone(tone) {
    if (!("__TAURI_INTERNALS__" in window))
        return;
    try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().setTheme(tone);
    }
    catch {
        // Cosmetic only; the page itself is already themed.
    }
}
/**
 * Applies the appearance to <html> and follows the system live when set to
 * follow it. Returns the step in use.
 */
export function useAppearance(a) {
    const [dark, setDark] = useState(systemDark);
    useEffect(() => {
        if (typeof window.matchMedia !== "function")
            return;
        const mq = window.matchMedia(query);
        const on = () => setDark(mq.matches);
        mq.addEventListener?.("change", on);
        return () => mq.removeEventListener?.("change", on);
    }, []);
    const level = resolveLevel(a, dark);
    const material = a.translucent !== false && materialAvailable();
    useEffect(() => {
        applyAppearance(document.documentElement, level, a.accent, material);
    }, [level, a.accent, material]);
    const tone = toneOf(level);
    useEffect(() => {
        void setWindowTone(tone);
    }, [tone]);
    return { level, accent: a.accent, systemDark: dark };
}
