// Spec 012 in the app: setup, progress, overview, group lists, change
// detection and the side-by-side detail, over the in-memory comparison.

import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
import { PREVIOUS } from "../testing/fakeCompare";

const CURRENT = "/crawls/example.com.spiderlens";

/** The app with the crawl file at CURRENT open, as after Open crawl…. */
async function renderWithCrawl() {
  const t = fakeTransport();
  const picks: string[] = [CURRENT];
  t.pickCrawlFile = async () => picks.shift() ?? null;
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  await palette("Open crawl");
  await screen.findByRole("grid");
  return { t, picks };
}

async function palette(query: string) {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  fireEvent.keyDown(input, { key: "Enter" });
}

/** Opens setup from the palette and waits for both crawls to be described. */
async function openSetup() {
  await palette("Compare crawls");
  await screen.findByRole("heading", { name: "Compare crawls" });
  await screen.findByLabelText("Previous crawl");
  await screen.findByLabelText("Current crawl");
}

async function runComparison(t: ReturnType<typeof fakeTransport>) {
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
  await screen.findByText("Comparing example.com-old.spiderlens with example.com.spiderlens");
  act(() => t.compare.progress({ phase: "previous", done: 50, total: 120 }));
  await screen.findByText("Reading the previous crawl · 50 of 120 URLs");
  act(() => t.compare.finish());
  await screen.findByRole("table", { name: "Filter changes" });
}

test("setup_suggests_the_previous_crawl_and_shows_both_files", async () => {
  // US3-1, FR-001, FR-003, FR-007
  await renderWithCrawl();
  await openSetup();
  expect(screen.getByLabelText("Previous file").textContent).toBe("example.com-old.spiderlens");
  expect(screen.getByLabelText("Current file").textContent).toBe("example.com.spiderlens");
  const prev = screen.getByLabelText("Previous crawl");
  expect(within(prev).getByText("Complete")).toBeTruthy();
  expect(within(prev).getByText(/120 URLs/)).toBeTruthy();
  const diffs = screen.getByRole("region", { name: "Configuration differences" });
  expect(diffs.textContent).toContain("Maximum depth: No limit → 5");
  expect(screen.getByRole("button", { name: "Compare" }).hasAttribute("disabled")).toBe(false);
});

test("swap_exchanges_previous_and_current", async () => {
  // US3-2
  await renderWithCrawl();
  await openSetup();
  fireEvent.click(screen.getByRole("button", { name: "Swap" }));
  await waitFor(() => expect(screen.getByLabelText("Previous file").textContent).toBe("example.com.spiderlens"));
  expect(screen.getByLabelText("Current file").textContent).toBe("example.com-old.spiderlens");
});

test("same_file_twice_is_refused_with_a_reason", async () => {
  // Edge case: a crawl compared with itself
  const { picks } = await renderWithCrawl();
  await openSetup();
  picks.push(CURRENT);
  fireEvent.click(screen.getByRole("button", { name: "Choose the previous crawl" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Choose two different crawl files");
  expect(screen.getByRole("button", { name: "Compare" }).hasAttribute("disabled")).toBe(true);
});

test("comparison_runs_with_progress_and_shows_the_overview", async () => {
  // US1-1, US1-2, FR-011, FR-012
  const { t } = await renderWithCrawl();
  await openSetup();
  await runComparison(t);
  expect(t.compare.started).toEqual([{ previous: PREVIOUS, current: CURRENT, settings: { mapping: [], elements: [], issue_settings: "current" } }]);
  expect(screen.getByRole("button", { name: "New URLs: 20" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Missing URLs: 10" })).toBeTruthy();
  expect(screen.getByText("Lost indexable pages")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Comparison" }).getAttribute("aria-current")).toBe("page");

  const table = screen.getByRole("table", { name: "Filter changes" });
  const rows = within(table).getAllByRole("row").slice(1) as HTMLTableRowElement[];
  // Largest change first; unchanged filters hidden; not comparable kept with its reason.
  expect(rows.map((r) => r.cells[0].firstChild?.textContent)).toEqual(["Internal: All", "Response codes: Client error (4xx)", "H1: Missing"]);
  expect(rows[0].cells[3].textContent).toBe("+10");
  expect(within(table).getByText("The previous crawl did not read page content.")).toBeTruthy();

  fireEvent.click(screen.getByRole("switch", { name: "Hide unchanged" }));
  await waitFor(() => expect(within(table).getByText("Page titles: Missing")).toBeTruthy());
});

test("a_count_opens_its_urls_with_search_detail_and_export", async () => {
  // US1-3, US1-4, FR-013, FR-021
  const { t } = await renderWithCrawl();
  await openSetup();
  await runComparison(t);
  fireEvent.click(screen.getByRole("button", { name: "Response codes: Client error (4xx): added" }));
  const list = await screen.findByRole("grid", { name: "Response codes: Client error (4xx) · Added URLs" });
  expect(t.compare.opened.at(-1)).toEqual({ group: { kind: "filter", tab: "response-codes", filter: "client_error", change: "added" }, search: "" });
  await waitFor(() => expect(screen.getByTestId("group-count").textContent).toBe("12 URLs"));
  expect(within(list).getByText("Address (previous)")).toBeTruthy();

  fireEvent.change(screen.getByLabelText("Search addresses"), { target: { value: "page-1" } });
  await waitFor(() => expect(screen.getByTestId("group-count").textContent).toBe("4 URLs"));

  fireEvent.click(within(list).getAllByRole("row")[1]);
  const detail = await screen.findByRole("table", { name: "Previous and current" });
  const indexability = within(detail).getByText("Indexability").closest("tr")!;
  expect(indexability.getAttribute("data-changed")).toBe("true");
  expect(indexability.textContent).toContain("Non-indexable: noindex");

  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await waitFor(() => expect(t.compare.exports).toHaveLength(1));
  expect(await screen.findByText("Exported 4 rows to /exports/response-codes-client-error-4xx-added.csv")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Back to the comparison overview" }));
  await screen.findByRole("table", { name: "Filter changes" });
});

test("change_detection_thresholds_recount_without_a_rescan", async () => {
  // US2-2, FR-019
  const { t } = await renderWithCrawl();
  await openSetup();
  await runComparison(t);
  fireEvent.click(screen.getByRole("radio", { name: "Change detection" }));
  const table = await screen.findByRole("table", { name: "Change detection" });
  expect(within(table).getByRole("button", { name: "Word count: changed" }).textContent).toBe("30");
  fireEvent.change(screen.getByLabelText("Word count threshold"), { target: { value: "20" } });
  await waitFor(() => expect(within(table).getByRole("button", { name: "Word count: changed" }).textContent).toBe("15"));
  expect(t.compare.elementCalls.at(-1)).toContainEqual({ id: "word_count", on: true, threshold: 20 });
  expect(t.compare.started).toHaveLength(1);

  fireEvent.click(screen.getByRole("button", { name: "Page title: changed" }));
  await screen.findByRole("grid", { name: "Changed: Page title URLs" });
  expect(t.compare.opened.at(-1)?.group).toEqual({ kind: "element", element: "title" });
});

test("migration_mapping_is_suggested_tested_and_used", async () => {
  // US4-1, US4-2, FR-006, FR-008
  const { t } = await renderWithCrawl();
  t.compare.previousStart = "https://old.example.com/";
  await openSetup();
  fireEvent.click(await screen.findByRole("button", { name: "Use this rule" }));
  expect((screen.getByLabelText("Find") as HTMLInputElement).value).toBe("https://old.example.com/");
  fireEvent.change(screen.getByLabelText("Test URLs"), { target: { value: "https://old.example.com/a\nhttps://other.test/b" } });
  fireEvent.click(screen.getByRole("button", { name: "Test mapping" }));
  const results = await screen.findByRole("list", { name: "Mapping test results" });
  expect(within(results).getByText("https://example.com/a")).toBeTruthy();
  expect(within(results).getByText("Not in current crawl")).toBeTruthy();
  await runComparison(t);
  expect(t.compare.started[0].settings.mapping).toEqual([{ find: "https://old.example.com/", replace: "https://example.com/", regex: false }]);
  expect(screen.getByText("1 mapping rule")).toBeTruthy();
});

test("cancel_and_close_leave_the_comparison", async () => {
  // US6-2, FR-031
  const { t } = await renderWithCrawl();
  await openSetup();
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
  fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Comparison" })).toBeNull());
  expect(t.compare.cancelled).toEqual([1]);

  await openSetup();
  await runComparison(t);
  fireEvent.click(screen.getByRole("button", { name: "Close comparison" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Comparison" })).toBeNull());
  expect(t.compare.closed).toEqual([2]);
});

test("failed_comparison_says_why", async () => {
  const { t } = await renderWithCrawl();
  await openSetup();
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
  await screen.findByText(/Comparing/);
  act(() => t.compare.fail("The previous crawl file is damaged."));
  expect((await screen.findByRole("alert")).textContent).toBe("The previous crawl file is damaged.");
});
