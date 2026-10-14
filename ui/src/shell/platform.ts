// Platform facts the UI needs: which modifier key to show and whether the
// window uses the macOS overlay title bar (research R4).

export type Platform = "mac" | "windows" | "linux";

export function detectPlatform(ua = typeof navigator === "undefined" ? "" : navigator.userAgent): Platform {
  if (/Mac|iPhone|iPad/.test(ua)) return "mac";
  if (/Windows/.test(ua)) return "windows";
  return "linux";
}

export const platform: Platform = detectPlatform();

/**
 * The window has the system's translucent material behind it (spec 017
 * FR-027): only the macOS desktop app, whose window is transparent and carries
 * the sidebar material (tauri.macos.conf.json). Browsers and other platforms
 * stay solid.
 */
export function materialAvailable(p: Platform = platform, w: object = typeof window === "undefined" ? {} : window): boolean {
  return p === "mac" && "__TAURI_INTERNALS__" in w;
}

/** `Mod+K` → `⌘K` on macOS, `Ctrl+K` elsewhere. */
export function formatShortcut(keys: string, p: Platform = platform): string {
  const parts = keys.split("+");
  if (p === "mac") {
    const map: Record<string, string> = { Mod: "⌘", Shift: "⇧", Alt: "⌥", Ctrl: "⌃", Enter: "↩", Escape: "⎋" };
    return parts.map((k) => map[k] ?? k.toUpperCase()).join("");
  }
  return parts.map((k) => (k === "Mod" ? "Ctrl" : k.length === 1 ? k.toUpperCase() : k)).join("+");
}

/** Does a keyboard event match `Mod+Shift+K`-style keys? */
export function matches(e: KeyboardEvent | React.KeyboardEvent, keys: string, p: Platform = platform): boolean {
  const parts = keys.split("+");
  const key = parts[parts.length - 1].toLowerCase();
  const mod = parts.includes("Mod");
  const wantMeta = p === "mac" ? mod : false;
  const wantCtrl = p === "mac" ? parts.includes("Ctrl") : mod || parts.includes("Ctrl");
  const codes: Record<string, string> = { ",": "comma", "\\": "backslash", "/": "slash", ".": "period" };
  const code = e.code?.toLowerCase() ?? "";
  // Option on macOS changes e.key (Option+S is "ß"), so also accept the physical key.
  const sameKey = e.key.toLowerCase() === key || code === `key${key}` || code === codes[key];
  return (
    sameKey &&
    e.metaKey === wantMeta &&
    e.ctrlKey === wantCtrl &&
    e.shiftKey === parts.includes("Shift") &&
    e.altKey === parts.includes("Alt")
  );
}
