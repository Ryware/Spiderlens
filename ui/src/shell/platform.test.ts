import { detectPlatform, formatShortcut, matches } from "./platform";

test("detects_platform_from_user_agent", () => {
  expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe("mac");
  expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
  expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
});

test("formats_shortcuts_the_platform_way", () => {
  expect(formatShortcut("Mod+K", "mac")).toBe("⌘K");
  expect(formatShortcut("Mod+Alt+S", "mac")).toBe("⌘⌥S");
  expect(formatShortcut("Mod+K", "windows")).toBe("Ctrl+K");
  expect(formatShortcut("Mod+,", "linux")).toBe("Ctrl+,");
});

test("matches_physical_keys_when_option_changes_the_character", () => {
  const e = { key: "ß", code: "KeyS", metaKey: true, ctrlKey: false, shiftKey: false, altKey: true } as KeyboardEvent;
  expect(matches(e, "Mod+Alt+S", "mac")).toBe(true);
  expect(matches(e, "Mod+S", "mac")).toBe(false);
  const w = { key: "k", code: "KeyK", metaKey: false, ctrlKey: true, shiftKey: false, altKey: false } as KeyboardEvent;
  expect(matches(w, "Mod+K", "windows")).toBe(true);
  expect(matches(w, "Mod+K", "mac")).toBe(false);
});
