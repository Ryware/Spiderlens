import { defaults, loadUiState, saveUiState } from "./storage";
import { resolveTheme } from "./useUiState";

afterEach(() => vi.restoreAllMocks());

test("missing_state_gives_defaults", () => {
  expect(loadUiState()).toEqual(defaults);
});

test("saved_state_round_trips_and_fills_new_fields", () => {
  saveUiState({ ...defaults, theme: "dark", sidebarWidth: 260 });
  expect(loadUiState()).toEqual({ ...defaults, theme: "dark", sidebarWidth: 260 });
  // An older save without some fields still gets them from the defaults.
  localStorage.setItem("spiderlens.ui", JSON.stringify({ v: 1, detailCollapsed: true }));
  expect(loadUiState()).toEqual({ ...defaults, detailCollapsed: true });
});

test("corrupt_or_other_version_state_gives_defaults", () => {
  localStorage.setItem("spiderlens.ui", "{oops");
  expect(loadUiState()).toEqual(defaults);
  localStorage.setItem("spiderlens.ui", JSON.stringify({ v: 2, theme: "dark" }));
  expect(loadUiState()).toEqual(defaults);
  localStorage.setItem("spiderlens.ui", "null");
  expect(loadUiState()).toEqual(defaults);
});

test("storage_errors_are_not_fatal", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("SecurityError");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("QuotaExceededError");
  });
  expect(loadUiState()).toEqual(defaults);
  expect(() => saveUiState(defaults)).not.toThrow();
});

test("defaults_are_not_shared", () => {
  const a = loadUiState();
  a.sidebarWidth = 999;
  expect(loadUiState().sidebarWidth).toBe(defaults.sidebarWidth);
});

test("resolve_theme", () => {
  expect(resolveTheme("light", true)).toBe("light");
  expect(resolveTheme("dark", false)).toBe("dark");
  expect(resolveTheme("system", true)).toBe("dark");
  expect(resolveTheme("system", false)).toBe("light");
  // jsdom has no matchMedia: System reads as light.
  expect(resolveTheme("system")).toBe("light");
});
