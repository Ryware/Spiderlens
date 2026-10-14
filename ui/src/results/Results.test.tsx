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

test("content_columns_and_filters_work_when_the_crawl_collected_them", async () => {
  const urls = fakeUrls(30).map((u, i) => ({ ...u, h1: i % 3 === 0 ? null : `Heading ${i}` }));
  const t = fakeTransport(urls);
  t.results.features = ["base", "extraction", "timing", "raw_html"];
  await openInternal(t);
  const h1 = within(grid()).getByRole("columnheader", { name: /H1/ });
  expect(within(h1).queryByLabelText("Not collected")).toBeNull();
  expect(within(rows()[1]).getAllByRole("gridcell")[3].textContent).toBe("Heading 1");
  const missing = (await screen.findByRole("button", { name: /Missing H1/ })) as HTMLButtonElement;
  expect(missing.disabled).toBe(false);
  // Every third URL has no H1 (10 of 30); page-9 is a 404, not an HTML page.
  await waitFor(() => expect(missing.textContent).toBe("Missing H19"));
  fireEvent.click(missing);
  await waitFor(() => expect(t.results.opened.at(-1)?.filter).toBe("missing_h1"));
  await waitFor(() => expect(rowCount()).toContain("9 URLs"));
});

test("detail_pane_shows_attempts_and_a_manual_retry", async () => {
  const urls = fakeUrls(30).map((u, i) => (i === 1 ? { ...u, attempts: 3, manual_retry: true } : i === 2 ? { ...u, attempts: 1 } : u));
  const t = fakeTransport(urls);
  t.results.features = ["base", "attempts"];
  await openInternal(t);
  const details = screen.getByRole("complementary", { name: "Details" });
  fireEvent.click(rows()[1]);
  expect(await within(details).findByText("3, last by Retry failed URLs")).toBeTruthy();
  fireEvent.click(rows()[2]);
  await waitFor(() => expect(within(details).getByRole("heading", { level: 2 }).textContent).toBe("https://example.com/page-2"));
  // One attempt is the normal case: no line for it.
  expect(within(details).queryByText("Attempts")).toBeNull();
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
  expect(within(details).getByText("Indexable")).toBeTruthy();
  expect(within(details).getByText("2.0 KB")).toBeTruthy();
  expect(within(details).getByText("x-robots-tag")).toBeTruthy();
  await waitFor(() => expect(within(details).getByText("https://example.com/")).toBeTruthy());
  expect(within(details).getByText("Home")).toBeTruthy();
  fireEvent.click(within(details).getByRole("radio", { name: "Headings 2" }));
  expect(await within(details).findByText("Section one")).toBeTruthy();
  expect(within(details).getByText("H2")).toBeTruthy();
  fireEvent.click(within(details).getByRole("radio", { name: /^Images/ }));
  expect(await within(details).findByText("No alt attribute")).toBeTruthy();
  expect(within(details).getByText("Empty alt")).toBeTruthy();
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

test("live_refresh_never_blanks_the_rows_on_screen", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  fireEvent.click(screen.getByRole("button", { name: "Internal" }));
  act(() => t.results.ready());
  await waitFor(() => expect(rows().length).toBeGreaterThan(2));
  const filled = () => rows().every((r) => r.getAttribute("data-id") !== null && within(r).getAllByRole("gridcell")[0].textContent !== "");
  expect(filled()).toBe(true);
  // Every refresh while the crawl runs rebuilds the view; the rows on screen
  // must stay filled through it, not drop to empty placeholders.
  act(() => t.results.add(fakeUrls(2).map((u, i) => ({ ...u, id: 100 + i, url: `https://example.com/zz-${i}` }))));
  await waitFor(() => expect(rowCount()).toContain("32 URLs"), { timeout: 3000 });
  expect(filled()).toBe(true);
  act(() => t.results.add(fakeUrls(1).map((u) => ({ ...u, id: 200, url: "https://example.com/zz-late" }))));
  await waitFor(() => expect(rowCount()).toContain("33 URLs"), { timeout: 3000 });
  expect(filled()).toBe(true);
});

test("values_switch_is_hidden_without_rendering", async () => {
  // Spec 014 T029: a raw crawl has no rendered values to show.
  const t = await openInternal();
  expect(screen.queryByRole("radiogroup", { name: "Which values" })).toBeNull();
  expect(t.results.opened.every((q) => q.values === undefined)).toBe(true);
});

test("values_switch_reopens_the_view_with_the_chosen_values", async () => {
  const t = fakeTransport();
  t.results.features = ["base", "rendering"];
  await openInternal(t);
  const group = await screen.findByRole("radiogroup", { name: "Which values" });
  const rendered = within(group).getByRole("radio", { name: "Rendered values" });
  await waitFor(() => expect(rendered.getAttribute("aria-checked")).toBe("true"));
  await waitFor(() => expect(t.results.opened.at(-1)?.values).toBe("rendered"));
  await waitFor(() => expect(t.results.counted.at(-1)).toBe("rendered"));

  fireEvent.click(within(group).getByRole("radio", { name: "Raw values" }));
  await waitFor(() => expect(t.results.opened.at(-1)?.values).toBe("raw"));
  await waitFor(() => expect(t.results.counted.at(-1)).toBe("raw"));
  expect(within(group).getByRole("radio", { name: "Raw values" }).getAttribute("aria-checked")).toBe("true");

  // The choice belongs to the crawl, so another tab keeps it.
  fireEvent.click(screen.getByRole("button", { name: "Response codes" }));
  await waitFor(() => expect(t.results.opened.at(-1)?.tab).toBe("response-codes"));
  expect(t.results.opened.at(-1)?.values).toBe("raw");
  const again = screen.getByRole("radiogroup", { name: "Which values" });
  expect(within(again).getByRole("radio", { name: "Raw values" }).getAttribute("aria-checked")).toBe("true");
});

// Sidebar counts (T014) ---------------------------------------------------------

const sidebarCount = (tab: string) => {
  const b = screen.getByRole("button", { name: tab });
  const id = b.getAttribute("aria-describedby");
  return id ? document.getElementById(id)?.textContent : null;
};

test("sidebar_shows_how_many_urls_each_tab_lists", async () => {
  const t = fakeTransport(fakeUrls(1234));
  await openInternal(t);
  await waitFor(() => expect(sidebarCount("Internal")).toBe("1,234 URLs"));
  const internal = screen.getByRole("button", { name: "Internal" });
  expect(internal.textContent).toBe("Internal1.2k");
  expect(sidebarCount("Response codes")).toBe("1,234 URLs");
  // A tab the file has no count for shows nothing, not 0.
  expect(sidebarCount("External")).toBeNull();
  expect(screen.getByRole("button", { name: "External" }).textContent).toBe("External");
});

test("sidebar_counts_are_blank_while_unknown", async () => {
  const t = fakeTransport();
  t.tabCounts = () => new Promise(() => {});
  await openInternal(t);
  expect(sidebarCount("Internal")).toBeNull();
  expect(screen.getByRole("button", { name: "Internal" }).textContent).toBe("Internal");
});

test("sidebar_counts_follow_a_live_crawl_one_query_at_a_time", async () => {
  const t = fakeTransport();
  let inFlight = 0;
  let overlapped = false;
  const ask = t.tabCounts;
  t.tabCounts = async (p) => {
    overlapped ||= inFlight > 0;
    inFlight++;
    try {
      return await ask(p);
    } finally {
      inFlight--;
    }
  };
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  await waitFor(() => expect(sidebarCount("Internal")).toBe("30 URLs"));
  act(() => t.results.add(fakeUrls(5).map((u, i) => ({ ...u, id: 100 + i }))));
  await waitFor(() => expect(sidebarCount("Internal")).toBe("35 URLs"), { timeout: 3000 });
  // The crawl ends: asked once more, then no longer polled.
  act(() => t.results.add(fakeUrls(1).map((u) => ({ ...u, id: 200 }))));
  act(() => t.crawls.finish({ report: fakeReport("completed", 36) }));
  await waitFor(() => expect(sidebarCount("Internal")).toBe("36 URLs"));
  const calls = t.results.tabCountCalls;
  await new Promise((r) => setTimeout(r, 1200));
  expect(t.results.tabCountCalls).toBe(calls);
  expect(overlapped).toBe(false);
});
