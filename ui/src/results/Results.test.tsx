// The result tabs (spec 007): table, filters with counts, sort, search,
// selection, copy, export, the detail pane, live refresh and opening a file.
// The queries themselves are tested in Rust (crates/results/tests).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { fakeUrls } from "../testing/fakeResults";

const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);
const rowCount = () => screen.getByTestId("row-count").textContent;

async function openInternal(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  return t;
}

test("result_tabs_are_off_until_a_crawl_exists", async () => {
  render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  expect(screen.getByRole("button", { name: "Internal" }).getAttribute("aria-disabled")).toBe("true");
});

test("internal_tab_shows_rows_and_the_full_count_once_ready", async () => {
  const t = await openInternal();
  expect(rowCount()).toContain("30 URLs");
  const first = rows()[0];
  expect(within(first).getAllByRole("gridcell")[0].textContent).toBe("https://example.com/");
  expect(within(first).getAllByRole("gridcell")[1].className).toContain("text-ok");
  act(() => t.results.ready());
  expect(rowCount()).toContain("30 URLs");
});

test("filters_show_counts_and_explain_unavailable_ones", async () => {
  const t = await openInternal();
  const html = await screen.findByRole("button", { name: /^HTML/ });
  await waitFor(() => expect(html.textContent).toBe("HTML27"));
  const missing = screen.getByRole("button", { name: /Missing H1/ }) as HTMLButtonElement;
  expect(missing.disabled).toBe(true);
  fireEvent.click(html);
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("html"));
  await waitFor(() => expect(rowCount()).toContain("27 URLs"));
});

test("header_click_sorts_then_reverses_then_clears", async () => {
  const t = await openInternal();
  const header = () => within(grid()).getByRole("columnheader", { name: /Title/ });
  fireEvent.click(within(header()).getByRole("button"));
  await waitFor(() => expect(t.results.opened.at(-1)?.sort).toBe("title"));
  expect(header().getAttribute("aria-sort")).toBe("ascending");
  fireEvent.click(within(header()).getByRole("button"));
  await waitFor(() => expect(t.results.opened.at(-1)?.sort).toBe("title:desc"));
  fireEvent.click(within(header()).getByRole("button"));
  await waitFor(() => expect(t.results.opened.at(-1)?.sort).toBeNull());
});

test("search_filters_the_view", async () => {
  const t = await openInternal();
  fireEvent.change(screen.getByLabelText("Search Internal"), { target: { value: "page-2" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.search).toBe("page-2"));
  await waitFor(() => expect(rowCount()).toContain("11 URLs"));
});

test("not_collected_columns_are_marked", async () => {
  await openInternal();
  const h1 = within(grid()).getByRole("columnheader", { name: /H1/ });
  expect(within(h1).getByLabelText("Not collected")).toBeTruthy();
});

test("selection_copies_rows_with_a_header", async () => {
  const t = await openInternal();
  fireEvent.click(rows()[0]);
  fireEvent.click(rows()[2], { shiftKey: true });
  expect(rowCount()).toContain("3 selected");
  fireEvent.keyDown(grid(), { key: "c", code: "KeyC", ctrlKey: true });
  await waitFor(() => expect(t.results.copied).toHaveLength(1));
  const lines = t.results.copied[0].split("\n");
  expect(lines[0]).toBe("Address\tStatus code\tTitle\tH1");
  expect(lines).toHaveLength(4);
  expect(await screen.findByText("Copied 3 rows.")).toBeTruthy();
});

test("keyboard_moves_the_selection_and_feeds_the_detail_pane", async () => {
  await openInternal();
  grid().focus();
  fireEvent.keyDown(grid(), { key: "ArrowDown" });
  fireEvent.keyDown(grid(), { key: "ArrowDown" });
  expect(rows()[1].getAttribute("aria-selected")).toBe("true");
  const details = screen.getByRole("complementary", { name: "Details" });
  await waitFor(() => expect(within(details).getByRole("heading", { level: 2 }).textContent).toBe("https://example.com/page-1"));
  expect(within(details).getByText("Headings.")).toBeTruthy();
  await waitFor(() => expect(within(details).getByText("https://example.com/")).toBeTruthy());
});

test("export_shows_progress_then_where_the_file_went", async () => {
  const t = await openInternal();
  act(() => t.results.ready());
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: /Export 30 rows as CSV/ }));
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0]).toMatchObject({ columns: ["address", "status_code", "title", "h1"], ranges: null, dest: "/exports/internal.csv" });
  act(() => t.results.progress({ rows_written: 10, total: 30 }));
  expect(await screen.findByText("Exporting 10 of 30 rows")).toBeTruthy();
  act(() => t.results.progress({ rows_written: 30, total: 30, state: "done", path: "/exports/internal.csv" }));
  expect(await screen.findByText("Exported 30 rows to /exports/internal.csv")).toBeTruthy();
});

test("live_crawl_refreshes_the_view_and_keeps_the_selected_row", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  fireEvent.click(screen.getByRole("button", { name: "Internal" }));
  fireEvent.click(screen.getByRole("columnheader", { name: /Title/ }).querySelector("button")!);
  await waitFor(() => expect(t.results.opened.at(-1)?.sort).toBe("title"));
  act(() => t.results.ready());
  await waitFor(() => expect(rows().length).toBeGreaterThan(2));
  fireEvent.click(rows()[1]);
  const picked = rows()[1].getAttribute("data-id");
  // Titles that sort before every other one push the selected row down.
  act(() => t.results.add(fakeUrls(2).map((u, i) => ({ ...u, id: 100 + i, title: `A first ${i}` }))));
  await waitFor(() => expect(rowCount()).toContain("32 URLs"), { timeout: 3000 });
  const selected = rows().find((r) => r.getAttribute("aria-selected") === "true");
  expect(selected?.getAttribute("data-id")).toBe(picked);
});

test("open_crawl_file_browses_its_results", async () => {
  const t = fakeTransport();
  t.pickCrawlFile = async () => "/crawls/old.spiderlens";
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  await waitFor(() => expect(screen.getByRole("region", { name: "Internal" })).toBeTruthy());
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
});
