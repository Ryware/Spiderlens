// Which columns each tab shows, their order and widths (FR-014), kept per
// machine like the rest of the layout. Storage can be missing or throw.
const KEY = "spiderlens.columns.v1";
function load() {
    try {
        return JSON.parse(localStorage.getItem(KEY) ?? "{}");
    }
    catch {
        return {};
    }
}
export function defaultPrefs(tab) {
    return { order: tab.columns.filter((c) => c.default_visible).map((c) => c.id), widths: {} };
}
/** Saved prefs for a tab, dropping columns the tab no longer has. */
export function loadPrefs(tab) {
    const saved = load()[tab.id];
    if (!saved)
        return defaultPrefs(tab);
    const ids = new Set(tab.columns.map((c) => c.id));
    const order = saved.order.filter((id) => ids.has(id));
    return { order: order.length ? order : defaultPrefs(tab).order, widths: saved.widths ?? {} };
}
export function savePrefs(tab, prefs) {
    try {
        localStorage.setItem(KEY, JSON.stringify({ ...load(), [tab]: prefs }));
    }
    catch {
        // Not fatal: the columns just aren't remembered.
    }
}
