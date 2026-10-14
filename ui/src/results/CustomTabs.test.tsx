// The custom extraction and search tabs (spec 013 US1, US3): they appear in
// the sidebar once the crawl has rules, show values with their filters, run
// the rules again from stored HTML, export all values, and the detail pane
// lists what each rule found. The queries are tested in Rust
// (crates/results/tests/custom.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import type { TabDef } from "../generated/TabDef";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { Inspector } from "./Inspector";

const PATH = "/crawls/example.com.spiderlens";
const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);
const sections = () => screen.getByRole("navigation", { name: "Sections" });

const extractionTab: TabDef = {
  id: "custom_extraction",
  title: "Custom extraction",
  columns: [
    { id: "address", label: "Address", kind: "url", width: 420, default_visible: true, need: "base" },
    { id: "x0", label: "Price", kind: "text", width: 200, default_visible: true, need: "base" },
  ],
  filters: [
    { id: "all", label: "All", need: "base" },
    { id: "x0_has", label: "Price: has a value", need: "base" },
    { id: "x0_none", label: "Price: no value", need: "base" },
  ],
  search_column: "x0",
  scoped: false,
};

const searchTab: TabDef = {
  id: "custom_search",
  title: "Custom search",
  columns: [
    { id: "address", label: "Address", kind: "url", width: 420, default_visible: true, need: "base" },
    { id: "s0", label: "Consent", kind: "int", width: 100, default_visible: true, need: "base" },
  ],
  filters: [
    { id: "all", label: "All", need: "base" },
    { id: "s0_meets", label: "Consent: contains", need: "base" },
  ],
  search_column: null,
  scoped: false,
};

/** A finished crawl whose rules priced pages 1-4 and left 5-6 empty. */
async function crawlWithRules() {
  const t = fakeTransport();
  t.results.customTabs = [extractionTab, searchTab];
  for (let id = 1; id <= 6; id++) t.results.customCells.set(id, { x0: id <= 4 ? `$${id}0` : null, s0: id % 2 });
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  return t;
}

async function openExtraction(t: Awaited<ReturnType<typeof crawlWithRules>>) {
  fireEvent.click(await within(sections()).findByRole("button", { name: "Custom extraction" }));
  await waitFor(() => expect(rows()).toHaveLength(6));
  return t;
}

test("crawls_without_rules_show_no_custom_tabs", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await screen.findByRole("button", { name: "Internal" });
  expect(within(sections()).queryByText("Custom")).toBeNull();
  expect(within(sections()).queryByRole("button", { name: /Custom/ })).toBeNull();
});

test("custom_tabs_list_each_page_with_its_values_and_filters", async () => {
  const t = await openExtraction(await crawlWithRules());
  expect(within(sections()).getByRole("button", { name: "Custom search" })).toBeTruthy();
  expect(within(rows()[0]).getAllByRole("gridcell").map((c) => c.textContent)).toEqual(["https://example.com/", "$10"]);
  fireEvent.click(screen.getByRole("button", { name: /^Price: no value/ }));
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("x0_none"));
  await waitFor(() => expect(rows()).toHaveLength(2));
});

test("run_rules_again_shows_progress_and_reloads_the_tab", async () => {
  const t = await openExtraction(await crawlWithRules());
  fireEvent.click(screen.getByRole("button", { name: /Run rules again/ }));
  await waitFor(() => expect(t.custom.runs).toEqual([{ path: PATH, rules: null, only: [] }]));
  act(() => t.custom.progress({ done: 3, total: 6 }));
  expect(await screen.findByText("Running rules on 3 of 6 pages")).toBeTruthy();
  expect((screen.getByRole("button", { name: /Run rules again/ }) as HTMLButtonElement).disabled).toBe(true);
  // New values arrive with the run; the tab opens its view again to show them.
  t.results.customCells.set(5, { x0: "$50", s0: 1 });
  const opened = t.results.opened.length;
  act(() => t.custom.done({ state: "complete", pages: 6, path: PATH }));
  await waitFor(() => expect(t.results.opened.length).toBeGreaterThan(opened));
  expect(await screen.findByText("Rules ran on 6 pages from stored HTML. No requests were sent.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^Price: has a value/ }));
  await waitFor(() => expect(rows()).toHaveLength(5));
});

test("stopping_a_rerun_says_what_was_kept", async () => {
  const t = await openExtraction(await crawlWithRules());
  fireEvent.click(screen.getByRole("button", { name: /Run rules again/ }));
  act(() => t.custom.progress({ done: 2, total: 6 }));
  fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
  await waitFor(() => expect(t.custom.cancelled).toEqual([1]));
  act(() => t.custom.done({ state: "partial", pages: 2, path: PATH }));
  expect(await screen.findByText("Stopped after 2 pages. Pages after that keep their earlier values.")).toBeTruthy();
});

test("extraction_export_can_join_all_values", async () => {
  const t = await openExtraction(await crawlWithRules());
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: 'Export all values, joined by "|"…' }));
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0]).toEqual(expect.objectContaining({ dest: "/exports/custom-extraction.csv", allValues: " | " }));
});

test("search_tab_has_no_all_values_export", async () => {
  await crawlWithRules();
  fireEvent.click(await within(sections()).findByRole("button", { name: "Custom search" }));
  await waitFor(() => expect(rows()).toHaveLength(6));
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await screen.findByRole("menuitem", { name: /rows as CSV/ });
  expect(screen.queryByRole("menuitem", { name: /all values/ })).toBeNull();
});

test("built_in_tabs_have_no_rerun_action", async () => {
  await crawlWithRules();
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  expect(screen.queryByRole("button", { name: /Run rules again/ })).toBeNull();
});

test("detail_pane_lists_what_each_rule_found", async () => {
  const t = fakeTransport();
  t.custom.results.set(2, [
    { name: "Price", kind: "extractor", count: 2, flags: [], values: ["$20", "$25"], error: null, run_state: null },
    { name: "Consent", kind: "search", count: 0, flags: ["no_match"], values: [], error: null, run_state: "stale" },
    { name: "Author", kind: "extractor", count: null, flags: [], values: [], error: null, run_state: null },
  ]);
  render(<Inspector transport={t} path={PATH} id={2} />);
  const section = (await screen.findByRole("heading", { name: "Custom rules" })).closest("section")!;
  expect(within(section).getByText("2 values")).toBeTruthy();
  expect(within(section).getByText("$25")).toBeTruthy();
  expect(within(section).getByText("Does not meet")).toBeTruthy();
  expect(within(section).getByText(/rule changed after it ran/)).toBeTruthy();
  expect(within(section).getByText("Not run")).toBeTruthy();
});

test("detail_pane_has_no_custom_section_without_rules", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} />);
  await screen.findByRole("heading", { level: 2 });
  expect(screen.queryByRole("heading", { name: "Custom rules" })).toBeNull();
});

test("settings_rules_can_be_tried_on_a_crawled_page", async () => {
  // US4: the rules in the settings, on this page's stored HTML, without saving anything.
  const t = fakeTransport();
  t.results.features = ["base", "raw_html"];
  t.custom.html.set(2, `<span class="price">$99</span>`);
  const rules = { extractors: [{ name: "Price", kind: "css" as const, expr: ".price", mode: "text" as const }], searches: [] };
  render(<Inspector transport={t} path={PATH} id={2} rules={rules} />);
  fireEvent.click(await screen.findByRole("button", { name: "Test settings rules" }));
  const section = (await screen.findByRole("heading", { name: "Custom rules (settings, not saved)" })).closest("section")!;
  expect(within(section).getByText("$99")).toBeTruthy();
  expect(t.custom.tested).toEqual([{ urlId: 2, rules }]);
  fireEvent.click(within(section).getByRole("button", { name: "Show stored" }));
  expect(await screen.findByRole("heading", { name: "Custom rules" })).toBeTruthy();
});

test("trying_rules_on_a_page_without_stored_html_says_why", async () => {
  const t = fakeTransport();
  t.results.features = ["base", "raw_html"];
  const rules = { extractors: [], searches: [{ name: "Consent", term: "x", regex: false, condition: "contains" as const, target: "html" as const, ignore_case: false }] };
  render(<Inspector transport={t} path={PATH} id={3} rules={rules} />);
  fireEvent.click(await screen.findByRole("button", { name: "Test settings rules" }));
  expect(await screen.findByText(/can only be tested by URL/)).toBeTruthy();
});
