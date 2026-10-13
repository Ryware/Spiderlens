// Column choices per tab, kept in local storage (FR-014).
import { fakeTabs } from "../testing/fakeResults";
import { defaultPrefs, loadPrefs, savePrefs } from "./columnPrefs";

const KEY = "spiderlens.columns.v1";
const internal = fakeTabs[0];

test("defaults_are_the_tab_default_visible_columns", () => {
  expect(defaultPrefs(internal)).toEqual({ order: ["address", "status_code", "title", "h1"], widths: {} });
  expect(loadPrefs(internal)).toEqual(defaultPrefs(internal));
});

test("save_then_load_round_trips_per_tab", () => {
  savePrefs("internal", { order: ["title", "address"], widths: { title: 500 } });
  savePrefs("response-codes", { order: ["status_code"], widths: {} });
  expect(loadPrefs(internal)).toEqual({ order: ["title", "address"], widths: { title: 500 } });
  expect(loadPrefs(fakeTabs[1]).order).toEqual(["status_code"]);
});

test("load_drops_columns_the_tab_no_longer_has", () => {
  localStorage.setItem(KEY, JSON.stringify({ internal: { order: ["gone", "title", "old_col", "depth"], widths: { title: 300 } } }));
  expect(loadPrefs(internal)).toEqual({ order: ["title", "depth"], widths: { title: 300 } });
});

test("load_falls_back_to_defaults_when_nothing_known_is_left", () => {
  localStorage.setItem(KEY, JSON.stringify({ internal: { order: ["gone"] } }));
  expect(loadPrefs(internal)).toEqual(defaultPrefs(internal));
});

test("load_survives_corrupt_json", () => {
  localStorage.setItem(KEY, "{not json");
  expect(loadPrefs(internal)).toEqual(defaultPrefs(internal));
  savePrefs("internal", { order: ["address"], widths: {} });
  expect(loadPrefs(internal).order).toEqual(["address"]);
});

test("save_survives_storage_that_throws", () => {
  const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("quota");
  });
  try {
    expect(() => savePrefs("internal", { order: ["address"], widths: {} })).not.toThrow();
  } finally {
    spy.mockRestore();
  }
  expect(loadPrefs(internal)).toEqual(defaultPrefs(internal));
});
