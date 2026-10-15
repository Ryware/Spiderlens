// The Content section (spec 020): analysis state, the Content tab, clusters,
// the inspector's Duplicates section and the comparison view. The analysis
// itself is tested in Rust (crates/results/tests/near_dup.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { alignRows } from "./CompareSheet";

const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);

/** Crawls, finishes, and opens the Content section. */
async function openContent(t = fakeTransport(), analysed = true) {
  if (analysed) {
    t.results.nearDup = "done";
    t.results.features.push("extraction", "near_dup");
  }
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Content" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  return t;
}

test("content_tab_offers_the_analysis_and_shows_progress_then_results", async () => {
  const t = await openContent(fakeTransport(), false);
  fireEvent.click(await screen.findByRole("button", { name: "Find near duplicates" }));
  await waitFor(() => expect(t.results.analysisRuns).toBe(1));
  act(() => t.results.nearDupProgress({ done: 1, total: 3, step: "Comparing pages" }));
  expect(await screen.findByText("Comparing pages…")).toBeTruthy();
  expect(screen.getByRole("meter", { name: "Analysis progress" }).getAttribute("aria-valuenow")).toBe("33");
  act(() => t.results.finishAnalysis());
  expect((await screen.findByTestId("analysis-summary")).textContent).toContain("6 of 27 pages are near duplicates at 90%, in 2 clusters");
  await waitFor(() => expect(within(rows()[1]).getAllByRole("gridcell")[2].textContent).toBe("97"));
});

test("near_duplicates_filter_and_columns", async () => {
  const t = await openContent();
  const filter = within(await screen.findByRole("toolbar", { name: "Filters" })).getByRole("button", { name: /^Near duplicates/ });
  await waitFor(() => expect(filter.textContent).toBe("Near duplicates6"));
  fireEvent.click(filter);
  await waitFor(() => expect(t.results.opened.at(-1)).toMatchObject({ tab: "content", filter: "near_duplicates" }));
  await waitFor(() => expect(rows()).toHaveLength(6));
  const cells = within(rows()[0]).getAllByRole("gridcell").map((c) => c.textContent);
  expect(cells).toEqual(["https://example.com/page-1", "240", "97", "https://example.com/page-2", "3", "1"]);
});

test("analysis_waits_for_the_crawl_to_finish", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  fireEvent.click(await screen.findByRole("button", { name: "Content" }));
  expect(await screen.findByText("Near duplicates are found when the crawl finishes or is paused.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Find near duplicates" })).toBeNull();
});

test("clusters_list_largest_first_and_open_a_comparison", async () => {
  await openContent();
  fireEvent.click(screen.getByRole("radio", { name: "Clusters" }));
  const list = await screen.findByRole("listbox", { name: "Clusters" });
  const options = within(list).getAllByRole("option");
  expect(options.map((o) => o.textContent)).toEqual(["#14 pages91–97%https://example.com/page-1", "#22 pages100%https://example.com/page-11Exact"]);
  expect(options[0].getAttribute("aria-selected")).toBe("true");
  const members = await screen.findByRole("list", { name: "Pages in cluster 1" });
  expect(within(members).getAllByRole("listitem")).toHaveLength(4);
  expect(within(members).getAllByRole("listitem")[0].textContent).toContain("Representative");
  fireEvent.click(within(members).getByRole("button", { name: "Compare https://example.com/page-3 with https://example.com/page-1" }));
  const sheet = await screen.findByRole("dialog");
  expect(within(sheet).getByText("93% similar")).toBeTruthy();
  expect(within(sheet).getByText("2 of 3")).toBeTruthy();
  await waitFor(() => expect(sheet.querySelectorAll('mark[data-op="changed"]').length).toBe(2));
  // `]` steps to the next page in the cluster.
  fireEvent.keyDown(within(sheet).getByRole("table"), { key: "]" });
  expect(await within(sheet).findByText("3 of 3")).toBeTruthy();
  expect(within(sheet).getByText("91% similar")).toBeTruthy();
  fireEvent.keyDown(within(sheet).getByRole("table"), { key: "[" });
  expect(await within(sheet).findByText("2 of 3")).toBeTruthy();
});

test("comparison_marks_left_out_text_in_whole_page_mode", async () => {
  await openContent();
  fireEvent.click(screen.getByRole("radio", { name: "Clusters" }));
  const members = await screen.findByRole("list", { name: "Pages in cluster 1" });
  fireEvent.click(within(members).getByRole("button", { name: /Compare https:\/\/example.com\/page-2/ }));
  const sheet = await screen.findByRole("dialog");
  await waitFor(() => expect(sheet.querySelectorAll("mark").length).toBeGreaterThan(0));
  expect(sheet.querySelector('[data-op="excluded"]')).toBeNull();
  fireEvent.click(within(sheet).getByRole("switch", { name: "Show the whole page" }));
  await waitFor(() => expect(sheet.querySelectorAll('[data-op="excluded"]').length).toBe(2));
});

test("comparison_explains_when_the_crawl_kept_no_html", async () => {
  const t = fakeTransport();
  t.results.htmlStored = false;
  await openContent(t);
  fireEvent.click(screen.getByRole("radio", { name: "Clusters" }));
  const members = await screen.findByRole("list", { name: "Pages in cluster 1" });
  fireEvent.click(within(members).getByRole("button", { name: /Compare https:\/\/example.com\/page-2/ }));
  const sheet = await screen.findByRole("dialog");
  expect(await within(sheet).findByText("The text is not available")).toBeTruthy();
  expect(within(sheet).getByText("97% similar")).toBeTruthy();
});

test("inspector_lists_the_closest_matches_with_compare", async () => {
  await openContent();
  fireEvent.click(within(rows()[1]).getAllByRole("gridcell")[0]);
  const details = await screen.findByRole("complementary", { name: "Details" });
  const matches = await within(details).findByRole("list", { name: "Closest matches" });
  expect(within(details).getByText("Duplicates (3)")).toBeTruthy();
  expect(within(matches).getAllByRole("listitem").map((li) => li.textContent?.slice(0, 3))).toEqual(["97%", "93%", "91%"]);
  fireEvent.click(within(matches).getByRole("button", { name: "Compare with https://example.com/page-4" }));
  const sheet = await screen.findByRole("dialog");
  expect(within(sheet).getByText("3 of 3")).toBeTruthy();
  expect(within(sheet).getByText("91% similar")).toBeTruthy();
});

test("clusters_export_writes_csv", async () => {
  const t = await openContent();
  fireEvent.click(screen.getByRole("radio", { name: "Clusters" }));
  await screen.findByRole("listbox", { name: "Clusters" });
  fireEvent.click(screen.getByRole("button", { name: "Export clusters" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Export cluster 1 pairs as CSV…" }));
  await waitFor(() => expect(t.results.nearDupExports).toEqual([{ kind: "pairs", cluster: 1, dest: "/exports/near-duplicate-cluster-1.csv" }]));
  expect(await screen.findByText("Exported 6 rows to /exports/near-duplicate-cluster-1.csv")).toBeTruthy();
});

test("align_rows_pairs_aligned_paragraphs_and_keeps_the_rest_in_order", () => {
  const p = (pair: number | null) => ({ spans: [], pair });
  // a: [x0→b0, x1 only, x2→b2]; b: [y0→a0, y1 only, y2→a2]
  const rows = alignRows([p(0), p(null), p(2)], [p(0), p(null), p(2)]);
  expect(rows.map(([a, b]) => [a !== null, b !== null])).toEqual([
    [true, true],
    [true, false],
    [false, true],
    [true, true],
  ]);
  expect(alignRows([p(null)], [])).toHaveLength(1);
});
