// The macOS window config replaces the main window entry wholesale (Tauri
// merges platform files as JSON merge patches, and arrays are replaced), so
// it must repeat every field and add only the material (spec 017 FR-027).
import { readFileSync } from "node:fs";
import { join } from "node:path";
const conf = (name) => JSON.parse(readFileSync(join(process.cwd(), "../app/src-tauri", name), "utf8"));
test("mac_window_is_the_main_window_plus_the_material", () => {
    const main = conf("tauri.conf.json").app.windows[0];
    const { transparent, windowEffects, ...rest } = conf("tauri.macos.conf.json").app.windows[0];
    expect(rest).toEqual(main);
    expect(transparent).toBe(true);
    expect(windowEffects.effects).toEqual(["sidebar"]);
});
test("transparent_window_needs_the_private_api_flag", () => {
    expect(conf("tauri.conf.json").app.macOSPrivateApi).toBe(true);
    const cargo = readFileSync(join(process.cwd(), "../app/src-tauri/Cargo.toml"), "utf8");
    expect(cargo).toMatch(/tauri = \{[^}]*"macos-private-api"/);
});
