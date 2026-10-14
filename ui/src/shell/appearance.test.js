// Spec 018: choosing and resolving the brightness step, and the stored state.
import { applyAppearance, chooseLevel, resolveLevel, toneOf } from "./appearance";
import { defaults, loadUiState, saveUiState } from "./storage";
import { applyStoredAppearance } from "./useUiState";
const base = { followSystem: true, level: "dark", systemLight: "soft", systemDark: "deep", accent: "teal" };
test("follow_system_uses_the_chosen_step_for_each_side", () => {
    // US3-3
    expect(resolveLevel(base, false)).toBe("soft");
    expect(resolveLevel(base, true)).toBe("deep");
    expect(resolveLevel({ ...base, followSystem: false, level: "island" }, true)).toBe("island");
});
test("picking_a_step_while_following_sets_that_side_and_keeps_following", () => {
    // US3-2
    expect(chooseLevel(base, "midnight", true)).toEqual({ systemDark: "midnight" });
    expect(chooseLevel(base, "island", false)).toEqual({ systemLight: "island" });
});
test("picking_a_step_of_the_other_tone_stops_following_so_it_shows", () => {
    expect(chooseLevel(base, "midnight", false)).toEqual({ followSystem: false, level: "midnight" });
    expect(chooseLevel({ ...base, followSystem: false }, "light", true)).toEqual({ level: "light" });
});
test("tones_of_the_six_steps", () => {
    expect(["light", "soft", "island", "dark", "deep", "midnight"].map(toneOf)).toEqual(["light", "light", "light", "dark", "dark", "dark"]);
});
test("apply_sets_level_tone_and_accent_on_the_root", () => {
    const el = document.createElement("html");
    applyAppearance(el, "island", "orchid");
    expect({ ...el.dataset }).toEqual({ level: "island", tone: "light", accent: "orchid" });
});
test("apply_marks_and_unmarks_the_material", () => {
    const el = document.createElement("html");
    applyAppearance(el, "dark", "teal", true);
    expect(el.dataset.material).toBe("on");
    applyAppearance(el, "dark", "teal", false);
    expect(el.dataset.material).toBeUndefined();
});
test("version_1_theme_is_carried_across", () => {
    // Users keep their spec 017 choice after the update.
    localStorage.setItem("spiderlens.ui", JSON.stringify({ v: 1, theme: "dark", sidebarWidth: 260 }));
    expect(loadUiState()).toMatchObject({ v: 2, followSystem: false, level: "dark", accent: "teal", sidebarWidth: 260 });
    localStorage.setItem("spiderlens.ui", JSON.stringify({ v: 1, theme: "system" }));
    expect(loadUiState()).toMatchObject({ followSystem: true, systemLight: "light", systemDark: "dark" });
    expect(loadUiState()).not.toHaveProperty("theme");
});
test("unknown_step_or_palette_falls_back_for_that_field_only", () => {
    // Edge case: a value from a newer build.
    localStorage.setItem("spiderlens.ui", JSON.stringify({ ...defaults, level: "dusk", accent: "neon", systemLight: "midnight", systemDark: "deep", detailWidth: 340 }));
    expect(loadUiState()).toMatchObject({ level: "dark", accent: "teal", systemLight: "light", systemDark: "deep", detailWidth: 340 });
});
test("stored_appearance_is_applied_before_react_renders", () => {
    // FR-014: main.tsx calls this before the first render.
    saveUiState({ ...defaults, followSystem: false, level: "midnight", accent: "celery" });
    applyStoredAppearance();
    expect(document.documentElement.dataset).toMatchObject({ level: "midnight", tone: "dark", accent: "celery" });
});
