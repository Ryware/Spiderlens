// The result tabs (spec 007), beyond Results.test.tsx: keyboard in the grid,
// the search box, copy limits, the columns menu and drag, scope, per-tab
// memory, export outcomes and error and empty states.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import type { ViewQuery } from "../generated/ViewQuery";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { fakeUrls, type FakeUrl } from "../testing/fakeResults";

const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);
const rowCount = () => screen.getByTestId("row-count").textContent;
const headers = () => within(grid()).getAllByRole("columnheader").map((h) => h.textContent);
const header = (name: RegExp | string) => within(grid()).getByRole("columnheader", { name });
const details = () => screen.getByRole("complementary", { name: "Details" });
const selectedIds = () => rows().filter((r) => r.getAttribute("aria-selected") === "true").map((r) => r.getAttribute("data-id"));
const search = () => screen.getByLabelText("Search Internal") as HTMLInputElement;

/** Waits for the detail pane to show `url`, which is the focused row. */
const focusedIs = (url: string) => waitFor(() => expect(within(details()).getByRole("heading", { level: 2 }).textContent).toBe(url));
const key = (k: string, mods: Partial<Record<"shiftKey" | "ctrlKey", boolean>> = {}) => fireEvent.keyDown(grid(), { key: k, ...mods });

async function finishedCrawl(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  return t;
}

async function openInternal(t = fakeTransport()) {
  await finishedCrawl(t);
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  return t;
}

/** Records every query the views open, scope included. */
function recordQueries(t: ReturnType<typeof fakeTransport>) {
  const queries: ViewQuery[] = [];
  const open = t.viewOpen;
  t.viewOpen = (p, q, n) => {
    queries.push(q);
    return open(p, q, n);
  };
  return queries;
}

async function openColumns() {
  fireEvent.click(screen.getByRole("button", { name: "Columns" }));
  return screen.findByRole("menu");
}
const columnItem = (name: string) => screen.getByRole("menuitemcheckbox", { name: new RegExp(`^${name}`) });

async function openExportItem(name: RegExp) {
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  fireEvent.click(await screen.findByRole("menuitem", { name }));
}

const dataTransfer = () => ({ effectAllowed: "", dropEffect: "", setData() {}, getData() {} });
function drag(from: HTMLElement, to: HTMLElement) {
  const dt = dataTransfer();
  fireEvent.dragStart(from, { dataTransfer: dt });
  fireEvent.dragEnter(to, { dataTransfer: dt });
  fireEvent.dragOver(to, { dataTransfer: dt });
  fireEvent.drop(to, { dataTransfer: dt });
  fireEvent.dragEnd(from, { dataTransfer: dt });
}

// Keyboard in the grid --------------------------------------------------------

test("home_and_end_jump_to_the_first_and_last_row", async () => {
  await openInternal();
  grid().focus();
  key("End");
  await focusedIs("https://example.com/page-29");
  expect(rowCount()).not.toContain("selected");
  key("Home");
  await focusedIs("https://example.com/");
  expect(rows()[0].getAttribute("aria-selected")).toBe("true");
  expect(grid().getAttribute("aria-activedescendant")).toBe("row-0");
});

test("page_down_and_page_up_move_by_a_page", async () => {
  await openInternal();
  grid().focus();
  key("ArrowDown");
  key("PageDown");
  key("PageDown");
  const down = selectedIds();
  expect(down).toHaveLength(1);
  expect(Number(down[0])).toBeGreaterThan(1);
  await focusedIs(`https://example.com/page-${Number(down[0]) - 1}`);
  key("PageUp");
  key("PageUp");
  expect(selectedIds()).toEqual(["1"]);
  key("PageUp");
  expect(selectedIds()).toEqual(["1"]);
});

test("shift_arrow_down_extends_the_selection", async () => {
  await openInternal();
  grid().focus();
  key("ArrowDown");
  key("ArrowDown", { shiftKey: true });
  key("ArrowDown", { shiftKey: true });
  expect(rowCount()).toContain("3 selected");
  expect(selectedIds()).toEqual(["1", "2", "3"]);
  key("ArrowUp", { shiftKey: true });
  expect(rowCount()).toContain("2 selected");
  key("ArrowDown");
  expect(selectedIds()).toEqual(["3"]);
  expect(rowCount()).not.toContain("selected");
});

test("ctrl_a_selects_every_row", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  grid().focus();
  key("a", { ctrlKey: true });
  expect(rowCount()).toContain("30 selected");
  expect(rows().every((r) => r.getAttribute("aria-selected") === "true")).toBe(true);
});

test("ctrl_arrow_jumps_to_the_ends", async () => {
  await openInternal();
  grid().focus();
  key("ArrowDown");
  key("ArrowDown", { ctrlKey: true });
  await focusedIs("https://example.com/page-29");
  key("ArrowUp", { ctrlKey: true });
  await focusedIs("https://example.com/");
  key("ArrowDown", { ctrlKey: true, shiftKey: true });
  expect(rowCount()).toContain("30 selected");
});

test("ctrl_click_toggles_rows_in_and_out", async () => {
  await openInternal();
  fireEvent.click(rows()[0]);
  fireEvent.click(rows()[2], { ctrlKey: true });
  fireEvent.click(rows()[4], { ctrlKey: true });
  expect(rowCount()).toContain("3 selected");
  expect(selectedIds()).toEqual(["1", "3", "5"]);
  fireEvent.click(rows()[2], { ctrlKey: true });
  expect(selectedIds()).toEqual(["1", "5"]);
  fireEvent.click(rows()[3]);
  expect(selectedIds()).toEqual(["4"]);
});

// Search box ------------------------------------------------------------------

test("escape_in_search_clears_it", async () => {
  const t = await openInternal();
  fireEvent.change(search(), { target: { value: "page-2" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.search).toBe("page-2"));
  fireEvent.keyDown(search(), { key: "Escape" });
  expect(search().value).toBe("");
  await waitFor(() => expect(t.results.opened.at(-1)?.search).toBe(""));
  await waitFor(() => expect(rowCount()).toContain("30 URLs"));
});

test("arrow_down_in_search_moves_into_the_grid_on_the_first_row", async () => {
  await openInternal();
  search().focus();
  fireEvent.keyDown(search(), { key: "ArrowDown" });
  expect(document.activeElement).toBe(grid());
  expect(rows()[0].getAttribute("aria-selected")).toBe("true");
  await focusedIs("https://example.com/");
});

test("enter_in_search_keeps_an_existing_selection", async () => {
  await openInternal();
  fireEvent.click(rows()[3]);
  search().focus();
  fireEvent.keyDown(search(), { key: "Enter" });
  expect(document.activeElement).toBe(grid());
  expect(selectedIds()).toEqual(["4"]);
});

test("search_with_no_matches_says_so", async () => {
  await openInternal();
  fireEvent.change(search(), { target: { value: "zzz-nothing" } });
  expect(await screen.findByText("No URLs match the search")).toBeTruthy();
  expect(screen.getByText("Try fewer words, or another filter.")).toBeTruthy();
  expect(rowCount()).toContain("0 URLs");
});

// Copy ------------------------------------------------------------------------

test("copy_over_ten_thousand_rows_warns_and_copies_nothing", async () => {
  const t = await openInternal(fakeTransport(fakeUrls(10_001)));
  act(() => t.results.ready());
  await waitFor(() => expect(rowCount()).toContain("10,001 URLs"));
  grid().focus();
  key("a", { ctrlKey: true });
  expect(rowCount()).toContain("10,001 selected");
  fireEvent.keyDown(grid(), { key: "c", code: "KeyC", ctrlKey: true });
  expect(await screen.findByText("Copy takes up to 10,000 rows. Export the selection to a CSV file instead.")).toBeTruthy();
  expect(t.results.copied).toHaveLength(0);
});

test("copy_failure_shows_an_alert", async () => {
  const t = await openInternal();
  t.copyRows = async () => {
    throw new Error("clipboard unavailable");
  };
  fireEvent.click(rows()[0]);
  fireEvent.keyDown(grid(), { key: "c", code: "KeyC", ctrlKey: true });
  expect((await screen.findByRole("alert")).textContent).toContain("clipboard unavailable");
});

test("copy_with_nothing_selected_does_nothing", async () => {
  const t = await openInternal();
  const spy = vi.fn(t.copyRows);
  t.copyRows = spy;
  fireEvent.keyDown(grid(), { key: "c", code: "KeyC", ctrlKey: true });
  await new Promise((r) => setTimeout(r, 20));
  expect(spy).not.toHaveBeenCalled();
});

// Columns ---------------------------------------------------------------------

test("columns_menu_hides_a_column", async () => {
  await openInternal();
  await openColumns();
  expect(columnItem("Title").getAttribute("aria-checked")).toBe("true");
  fireEvent.click(columnItem("Title"));
  await waitFor(() => expect(headers()).toEqual(["Address", "Status code", "H1"]));
  expect(columnItem("Title").getAttribute("aria-checked")).toBe("false");
});

test("columns_menu_shows_a_hidden_column_in_tab_order", async () => {
  await openInternal();
  expect(headers()).not.toContain("Depth");
  await openColumns();
  expect(columnItem("Depth").getAttribute("aria-checked")).toBe("false");
  fireEvent.click(columnItem("Title"));
  fireEvent.click(columnItem("Depth"));
  fireEvent.click(columnItem("Title"));
  await waitFor(() => expect(headers()).toEqual(["Address", "Status code", "Title", "H1", "Depth"]));
  expect(within(rows()[1]).getAllByRole("gridcell")[4].textContent).toBe("1");
});

test("columns_menu_marks_not_collected_columns", async () => {
  await openInternal();
  await openColumns();
  expect(within(columnItem("H1")).getByText("Not collected")).toBeTruthy();
  expect(within(columnItem("Title")).queryByText("Not collected")).toBeNull();
});

test("the_last_visible_column_cannot_be_hidden", async () => {
  await openInternal();
  await openColumns();
  for (const c of ["Status code", "Title", "H1"]) fireEvent.click(columnItem(c));
  await waitFor(() => expect(headers()).toEqual(["Address"]));
  fireEvent.click(columnItem("Address"));
  await new Promise((r) => setTimeout(r, 20));
  expect(headers()).toEqual(["Address"]);
  expect(columnItem("Address").getAttribute("aria-checked")).toBe("true");
});

test("reset_columns_restores_the_defaults", async () => {
  await openInternal();
  await openColumns();
  fireEvent.click(columnItem("Title"));
  fireEvent.click(columnItem("Depth"));
  await waitFor(() => expect(headers()).toEqual(["Address", "Status code", "H1", "Depth"]));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset columns" }));
  await waitFor(() => expect(headers()).toEqual(["Address", "Status code", "Title", "H1"]));
});

test("column_choices_persist_across_restarts", async () => {
  await openInternal();
  await openColumns();
  fireEvent.click(columnItem("Status code"));
  fireEvent.click(columnItem("Depth"));
  await waitFor(() => expect(headers()).toEqual(["Address", "Title", "H1", "Depth"]));
  cleanup();
  await openInternal();
  expect(headers()).toEqual(["Address", "Title", "H1", "Depth"]);
});

test("double_click_on_resize_handle_restores_default_width", async () => {
  localStorage.setItem("spiderlens.columns.v1", JSON.stringify({ internal: { order: ["address", "status_code", "title", "h1"], widths: { title: 500 } } }));
  await openInternal();
  expect(header(/Title/).style.width).toBe("500px");
  const handle = header(/Title/).querySelector("button + div[aria-hidden]")!;
  expect(handle).toBeTruthy();
  fireEvent.doubleClick(handle);
  await waitFor(() => expect(header(/Title/).style.width).toBe("320px"));
  expect((within(rows()[0]).getAllByRole("gridcell")[2] as HTMLElement).style.width).toBe("320px");
  expect(JSON.parse(localStorage.getItem("spiderlens.columns.v1")!).internal.widths.title).toBe(320);
});

test("dragging_a_header_reorders_columns_and_it_persists", async () => {
  await openInternal();
  drag(within(header(/Title/)).getByRole("button"), within(header(/Address/)).getByRole("button"));
  await waitFor(() => expect(headers()).toEqual(["Title", "Address", "Status code", "H1"]));
  expect(within(rows()[0]).getAllByRole("gridcell")[1].textContent).toBe("https://example.com/");
  drag(within(header(/Title/)).getByRole("button"), within(header(/Status code/)).getByRole("button"));
  await waitFor(() => expect(headers()).toEqual(["Address", "Status code", "Title", "H1"]));
  drag(within(header(/H1/)).getByRole("button"), within(header(/Address/)).getByRole("button"));
  await waitFor(() => expect(headers()).toEqual(["H1", "Address", "Status code", "Title"]));
  cleanup();
  await openInternal();
  expect(headers()).toEqual(["H1", "Address", "Status code", "Title"]);
});

test("dropping_a_header_on_itself_changes_nothing", async () => {
  await openInternal();
  const title = within(header(/Title/)).getByRole("button");
  drag(title, title);
  await new Promise((r) => setTimeout(r, 20));
  expect(headers()).toEqual(["Address", "Status code", "Title", "H1"]);
  expect(localStorage.getItem("spiderlens.columns.v1")).toBeNull();
});

// Response codes: scope and filters -----------------------------------------

async function openResponseCodes(urls?: FakeUrl[]) {
  const t = fakeTransport(urls);
  const queries = recordQueries(t);
  await openInternal(t);
  fireEvent.click(screen.getByRole("button", { name: "Response codes" }));
  await screen.findByRole("region", { name: "Response codes" });
  await waitFor(() => expect(queries.at(-1)?.tab).toBe("response-codes"));
  return { t, queries };
}

test("scope_switch_changes_which_urls_are_queried", async () => {
  const urls = fakeUrls(30).map((u, i) => (i >= 25 ? { ...u, url: `https://other.org/${i}`, internal: false } : u));
  const { queries } = await openResponseCodes(urls);
  const scope = screen.getByRole("radiogroup", { name: "Which URLs" });
  expect(within(scope).getByRole("radio", { name: "All" }).getAttribute("aria-checked")).toBe("true");
  expect(queries.at(-1)?.scope).toBe("all");
  await waitFor(() => expect(rowCount()).toContain("30 URLs"));
  fireEvent.click(within(scope).getByRole("radio", { name: "External" }));
  await waitFor(() => expect(queries.at(-1)?.scope).toBe("external"));
  await waitFor(() => expect(rowCount()).toContain("5 URLs"));
  fireEvent.click(within(scope).getByRole("radio", { name: "Internal" }));
  await waitFor(() => expect(queries.at(-1)?.scope).toBe("internal"));
  await waitFor(() => expect(rowCount()).toContain("25 URLs"));
  expect(within(scope).getByRole("radio", { name: "Internal" }).getAttribute("aria-checked")).toBe("true");
});

test("unscoped_tabs_have_no_scope_switch_and_query_all", async () => {
  const t = fakeTransport();
  const queries = recordQueries(t);
  await openInternal(t);
  expect(screen.queryByRole("radiogroup", { name: "Which URLs" })).toBeNull();
  expect(queries.at(-1)?.scope).toBe("all");
});

test("client_error_filter_counts_and_lists_the_4xx_urls", async () => {
  const { t } = await openResponseCodes();
  const chip = await screen.findByRole("button", { name: /^Client error \(4xx\)/ });
  await waitFor(() => expect(chip.textContent).toBe("Client error (4xx)3"));
  fireEvent.click(chip);
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("client_error"));
  await waitFor(() => expect(rowCount()).toContain("3 URLs"));
  expect(chip.getAttribute("aria-pressed")).toBe("true");
  expect(rows().map((r) => within(r).getAllByRole("gridcell")[0].textContent)).toEqual([
    "https://example.com/page-9",
    "https://example.com/page-19",
    "https://example.com/page-29",
  ]);
  expect(rows().every((r) => within(r).getAllByRole("gridcell")[1].textContent === "404")).toBe(true);
  expect(grid().getAttribute("aria-label")).toBe("Response codes: Client error (4xx)");
});

// Per-tab memory --------------------------------------------------------------

test("each_tab_remembers_its_filter_search_sort_and_row", async () => {
  const t = await openInternal();
  fireEvent.click(await screen.findByRole("button", { name: /^HTML/ }));
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("html"));
  fireEvent.change(search(), { target: { value: "page-2" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.search).toBe("page-2"));
  fireEvent.click(within(header(/Title/)).getByRole("button"));
  await waitFor(() => expect(t.results.opened.at(-1)?.sort).toBe("title"));
  await waitFor(() => expect(rowCount()).toContain("10 URLs"));
  fireEvent.click(rows()[2]);
  const picked = rows()[2].getAttribute("data-id");
  await focusedIs(`https://example.com/page-${Number(picked) - 1}`);

  fireEvent.click(screen.getByRole("button", { name: "Response codes" }));
  await waitFor(() => expect(t.results.opened.at(-1)).toEqual({ tab: "response-codes", filter: "all", search: "", sort: null }));
  expect((screen.getByLabelText("Search Response codes") as HTMLInputElement).value).toBe("");

  fireEvent.click(screen.getByRole("button", { name: "Internal" }));
  await waitFor(() => expect(t.results.opened.at(-1)).toEqual({ tab: "internal", filter: "html", search: "page-2", sort: "title" }));
  expect(search().value).toBe("page-2");
  expect(screen.getByRole("button", { name: /^HTML/ }).getAttribute("aria-pressed")).toBe("true");
  expect(header(/Title/).getAttribute("aria-sort")).toBe("ascending");
  await waitFor(() => expect(selectedIds()).toEqual([picked]));
  await focusedIs(`https://example.com/page-${Number(picked) - 1}`);
});

test("a_tab_opened_for_the_first_time_has_no_row_selected", async () => {
  const t = await openInternal();
  fireEvent.click(rows()[3]);
  await focusedIs("https://example.com/page-3");
  fireEvent.click(screen.getByRole("button", { name: "Response codes" }));
  await waitFor(() => expect(t.results.opened.at(-1)?.tab).toBe("response-codes"));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  await new Promise((r) => setTimeout(r, 50));
  expect(selectedIds()).toEqual([]);
  expect(within(details()).getByText("Nothing selected")).toBeTruthy();
});

// Export ----------------------------------------------------------------------

test("cancel_during_export_stops_it", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  const cancelled: number[] = [];
  t.exportCancel = async (job) => void cancelled.push(job);
  await openExportItem(/Export 30 rows as CSV/);
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  act(() => t.results.progress({ rows_written: 5, total: 30 }));
  expect(await screen.findByText("Exporting 5 of 30 rows")).toBeTruthy();
  expect((screen.getByRole("button", { name: "Export" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(cancelled).toEqual([1]);
  act(() => t.results.progress({ rows_written: 5, total: 30, state: "cancelled" }));
  expect(await screen.findByText("Export cancelled; no file was written.")).toBeTruthy();
  expect(screen.queryByText(/Exporting/)).toBeNull();
  expect((screen.getByRole("button", { name: "Export" }) as HTMLButtonElement).disabled).toBe(false);
});

test("failed_export_says_why", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  await openExportItem(/Export 30 rows as CSV/);
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  act(() => t.results.progress({ state: "failed", error: "disk full" }));
  expect(await screen.findByText("Export failed: disk full")).toBeTruthy();
});

test("export_start_rejecting_says_why", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  t.exportStart = async () => {
    throw new Error("cannot write there");
  };
  await openExportItem(/Export 30 rows as CSV/);
  expect(await screen.findByText("Export failed: Error: cannot write there")).toBeTruthy();
});

test("export_selected_sends_the_selection_ranges", async () => {
  const t = await openInternal();
  fireEvent.click(rows()[0]);
  fireEvent.click(rows()[2], { shiftKey: true });
  fireEvent.click(rows()[5], { ctrlKey: true });
  await openExportItem(/Export 4 selected as CSV/);
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0].ranges).toEqual([
    [0, 3],
    [5, 6],
  ]);
});

test("export_selected_is_off_with_nothing_selected", async () => {
  await openInternal();
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  const item = await screen.findByRole("menuitem", { name: /Export 0 selected as CSV/ });
  expect(item.getAttribute("aria-disabled")).toBe("true");
});

test("picking_no_file_starts_no_export", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  const asked: string[] = [];
  t.pickCsvPath = async (name) => {
    asked.push(name);
    return null;
  };
  await openExportItem(/Export 30 rows as CSV/);
  await waitFor(() => expect(asked).toEqual(["internal"]));
  await new Promise((r) => setTimeout(r, 20));
  expect(t.results.exports).toHaveLength(0);
  expect(screen.queryByText(/Exporting/)).toBeNull();
});

test("export_file_name_includes_the_active_filter", async () => {
  const t = await openInternal();
  fireEvent.click(await screen.findByRole("button", { name: /^HTML/ }));
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("html"));
  act(() => t.results.ready());
  await openExportItem(/Export 27 rows as CSV/);
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0].dest).toBe("/exports/internal-html.csv");
});

test("export_file_name_for_a_multi_word_tab", async () => {
  const { t } = await openResponseCodes();
  fireEvent.click(await screen.findByRole("button", { name: /^Client error/ }));
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("client_error"));
  await waitFor(() => expect(rowCount()).toContain("3 URLs"));
  await openExportItem(/Export 3 rows as CSV/);
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0]).toMatchObject({ dest: "/exports/response-codes-client_error.csv", columns: ["address", "status_code"] });
});

// Error and empty states, cell tones ----------------------------------------

test("view_open_failing_shows_the_error", async () => {
  const t = await finishedCrawl();
  t.viewOpen = async () => {
    throw new Error("file is locked");
  };
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  expect(await screen.findByText("Could not read the results")).toBeTruthy();
  expect(screen.getByText("Error: file is locked")).toBeTruthy();
  expect(screen.queryByRole("grid")).toBeNull();
});

test("all_filter_with_no_rows_says_no_urls", async () => {
  const urls = fakeUrls(3);
  const { queries } = await openResponseCodes(urls);
  fireEvent.click(within(screen.getByRole("radiogroup", { name: "Which URLs" })).getByRole("radio", { name: "External" }));
  await waitFor(() => expect(queries.at(-1)?.scope).toBe("external"));
  expect(await screen.findByText("No URLs")).toBeTruthy();
});

test("a_filter_with_no_rows_names_the_filter", async () => {
  const urls = fakeUrls(30).map((u) => ({ ...u, title: null, status: 404 }));
  await openInternal(fakeTransport(urls));
  const html = await screen.findByRole("button", { name: /^HTML/ });
  await waitFor(() => expect(html.textContent).toBe("HTML0"));
  fireEvent.click(html);
  expect(await screen.findByText("No HTML URLs")).toBeTruthy();
  expect(rowCount()).toContain("0 URLs");
});

test("status_cells_are_colored_by_meaning", async () => {
  await openInternal();
  expect(within(rows()[0]).getAllByRole("gridcell")[1].className).toContain("text-ok");
  fireEvent.change(search(), { target: { value: "page-9" } });
  await waitFor(() => expect(rowCount()).toContain("1 URL"));
  const cell = within(rows()[0]).getAllByRole("gridcell")[1];
  expect(cell.textContent).toBe("404");
  expect(cell.className).toContain("text-warn");
  expect(cell.className).not.toContain("text-ok");
});
