import { shortcutCommand, type Command } from "./commands";
import { matches } from "./platform";

const cmd = (id: string, shortcut?: string, enabled?: boolean): Command => ({ id, title: id, group: "Crawl", shortcut, enabled, run: () => {} });
const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

test("shortcut_finds_the_matching_enabled_command", () => {
  const cs = [cmd("palette-only"), cmd("start", "Mod+Enter"), cmd("open", "Mod+O")];
  expect(shortcutCommand(cs, key({ key: "o", code: "KeyO", ctrlKey: true }), (e, k) => matches(e, k, "linux"))?.id).toBe("open");
  expect(shortcutCommand(cs, key({ key: "Enter", code: "Enter", ctrlKey: true }), (e, k) => matches(e, k, "linux"))?.id).toBe("start");
  expect(shortcutCommand(cs, key({ key: "o", code: "KeyO" }), (e, k) => matches(e, k, "linux"))).toBeUndefined();
});

test("disabled_commands_have_no_shortcut", () => {
  const cs = [cmd("start", "Mod+Enter", false)];
  expect(shortcutCommand(cs, key({ key: "Enter", ctrlKey: true }), (e, k) => matches(e, k, "linux"))).toBeUndefined();
});

test("mac_uses_command_not_control", () => {
  const cs = [cmd("open", "Mod+O")];
  const mac = (e: KeyboardEvent, k: string) => matches(e, k, "mac");
  expect(shortcutCommand(cs, key({ key: "o", metaKey: true }), mac)?.id).toBe("open");
  expect(shortcutCommand(cs, key({ key: "o", ctrlKey: true }), mac)).toBeUndefined();
});
