// Segments (spec 027 part B) in the window: the settings editor with its
// inline errors, the open crawl regrouped at once, the segment filter shared
// by every tab, exports and the scope tester. Membership itself is tested
// in Rust (crates/results/tests/segments.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { newSegment, opsFor, withField } from "./segments";

type T = ReturnType<typeof fakeTransport>;

const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);
const rowCount = () => screen.getByTestId("row-count").textContent;
const nav = () => screen.queryByRole("navigation", { name: "Settings categories" });
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;

async function openSettings(category: string) {
  let n = nav();
  if (!n) {
    fireEvent.click(screen.getByRole("button", { name: "Configure" }));
    n = await screen.findByRole("navigation", { name: "Settings categories" });
  }
  fireEvent.click(within(n).getByRole("tab", { name: new RegExp(`^${category}`) }));
  return n;
}

async function closeSettings() {
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
}

async function crawled(t: T = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  return t;
}

/** Adds a segment named `name` whose path starts with `prefix`. */
async function addSegment(name: string, prefix: string) {
  await openSettings("Segments");
  fireEvent.click(screen.getByRole("button", { name: "Add segment" }));
  const cards = screen.getAllByRole("region").filter((r) => r.hasAttribute("data-segment"));
  const card = cards.at(-1)!;
  fireEvent.change(within(card).getByLabelText("Segment name"), { target: { value: name } });
  fireEvent.change(within(card).getByLabelText(`${name} condition 1: value`), { target: { value: prefix } });
  return card;
}

test("segment_helpers_keep_what_still_makes_sense", () => {
  const a = newSegment([]);
  expect(a).toMatchObject({ id: 1, name: "Segment 1", color: "teal", enabled: true, mode: "all" });
  const b = newSegment([a, { ...a, id: 7, name: "Segment 2" }]);
  expect(b.id).toBe(8);
  expect(b.name).toBe("Segment 3");
  expect(withField({ field: "path", op: "contains", value: "/x" }, "url")).toEqual({ field: "url", op: "contains", value: "/x" });
  expect(withField({ field: "path", op: "contains", value: "/x" }, "depth")).toEqual({ field: "depth", op: "lt", value: "" });
  expect(withField({ field: "path", op: "equals", value: "/x" }, "status_class")).toEqual({ field: "status_class", op: "equals", value: "2xx" });
  expect(opsFor("indexability").map((o) => o.value)).toEqual(["equals", "not_equals"]);
});

test("segments_editor_adds_orders_and_shows_errors_at_the_condition", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  await openSettings("Segments");
  expect(screen.getByText(/No segments yet/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Add segment" }));
  // A new condition has no value yet: the error sits under it.
  const alert = await screen.findByRole("alert");
  expect(alert.closest("[data-clause]")).toBeTruthy();
  expect(alert.textContent).toContain("Enter a value");
  fireEvent.change(screen.getByLabelText("Segment 1 condition 1: value"), { target: { value: "/blog/" } });
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());

  fireEvent.click(screen.getByRole("button", { name: "Add segment" }));
  fireEvent.change(screen.getByLabelText("Segment 2 condition 1: value"), { target: { value: "/shop/" } });
  fireEvent.click(screen.getAllByRole("button", { name: "Add condition" })[1]);
  expect(screen.getByLabelText("Segment 2 condition 2: value")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Remove segment 2 condition 2" }));
  fireEvent.click(screen.getByRole("button", { name: "Move Segment 2 up" }));
  const names = () => screen.getAllByLabelText("Segment name").map((i) => (i as HTMLInputElement).value);
  expect(names()).toEqual(["Segment 2", "Segment 1"]);
  // A field change swaps the tests and values to ones that field takes.
  fireEvent.change(screen.getByLabelText("Segment 1 condition 1: field"), { target: { value: "status_class" } });
  expect((screen.getByLabelText("Segment 1 condition 1: value") as HTMLSelectElement).value).toBe("2xx");
  fireEvent.change(screen.getByLabelText("Segment 1: colour"), { target: { value: "pink" } });
  fireEvent.click(screen.getByRole("switch", { name: "Segment 1: on" }));
  fireEvent.click(screen.getByRole("button", { name: "Delete Segment 2" }));
  expect(names()).toEqual(["Segment 1"]);

  // Segments travel with the crawl settings.
  await closeSettings();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.segments).toEqual([
    { id: 1, name: "Segment 1", color: "pink", enabled: false, mode: "all", clauses: [{ field: "status_class", op: "equals", value: "2xx" }] },
  ]);
});

test("segments_regroup_the_open_crawl_and_the_filter_is_shared_by_every_tab", async () => {
  const t = await crawled();
  expect(screen.queryByLabelText("Segment")).toBeNull();
  await addSegment("Ones", "/page-1");
  await closeSettings();
  await waitFor(() => expect(t.results.segments.map((s) => s.name)).toEqual(["Ones"]));

  const filter = (await screen.findByLabelText("Segment")) as HTMLSelectElement;
  // page-1 and page-10..19.
  await waitFor(() => expect(within(filter).getByRole("option", { name: /^Ones/ }).textContent).toBe("Ones (11)"));
  expect(within(filter).getByRole("option", { name: /^No segment/ }).textContent).toBe("No segment (19)");
  fireEvent.change(filter, { target: { value: "1" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.segment).toBe(1));
  await waitFor(() => expect(rowCount()).toContain("11 URLs"));
  expect(screen.getByTestId("segment-dot")).toBeTruthy();
  const html = screen.getByRole("button", { name: /^HTML/ });
  await waitFor(() => expect(html.textContent).toBe("HTML10"));

  fireEvent.click(screen.getByRole("button", { name: "Response codes" }));
  await waitFor(() => expect(t.results.opened.at(-1)?.tab).toBe("response-codes"));
  expect(t.results.opened.at(-1)?.segment).toBe(1);
  expect((screen.getByLabelText("Segment") as HTMLSelectElement).value).toBe("1");

  fireEvent.change(screen.getByLabelText("Segment"), { target: { value: "0" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.segment).toBe(0));
  await waitFor(() => expect(rowCount()).toContain("19 URLs"));
  fireEvent.change(screen.getByLabelText("Segment"), { target: { value: "" } });
  await waitFor(() => expect(t.results.opened.at(-1)?.segment).toBeUndefined());
});

test("editing_a_segment_updates_the_open_view", async () => {
  const t = await crawled();
  await addSegment("Ones", "/page-1");
  await closeSettings();
  fireEvent.change(await screen.findByLabelText("Segment"), { target: { value: "1" } });
  await waitFor(() => expect(rowCount()).toContain("11 URLs"));
  await openSettings("Segments");
  fireEvent.change(within(screen.getAllByRole("region").find((r) => r.hasAttribute("data-segment"))!).getByLabelText("Ones condition 1: value"), { target: { value: "/page-2" } });
  await closeSettings();
  await waitFor(() => expect(t.results.segmentCalls.at(-1)?.[0].clauses[0].value).toBe("/page-2"));
  // page-2 and page-20..29.
  await waitFor(() => expect(rowCount()).toContain("11 URLs"));
  await waitFor(() => expect(within(grid()).getAllByRole("row")[1].textContent).toContain("page-2"));
});

test("export_with_segments_adds_the_segment_column", async () => {
  const t = await crawled();
  await addSegment("Ones", "/page-1");
  await closeSettings();
  await screen.findByLabelText("Segment");
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: /Export 30 rows as CSV/ }));
  await waitFor(() => expect(t.results.exports).toHaveLength(1));
  expect(t.results.exports[0].columns.at(-1)).toBe("segment");
  expect(t.results.exports[0].segments?.map((s) => s.name)).toEqual(["Ones"]);
});

test("scope_tester_names_the_segment_a_url_would_be_in", async () => {
  render(<App transport={fakeTransport()} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  await addSegment("Blog", "/blog/");
  await openSettings("Scope");
  fireEvent.change(screen.getByLabelText("URLs to test"), { target: { value: "https://example.com/blog/a\nhttps://example.com/about" } });
  fireEvent.click(screen.getByRole("button", { name: "Test URLs" }));
  const cells = await screen.findAllByTestId("scope-segment");
  expect(cells.map((c) => c.textContent)).toEqual(["Blog", "No segment"]);
});

test("segments_edited_while_crawling_are_applied_again_when_it_ends", async () => {
  // A running crawl holds its file, so the engine saves them once it ends.
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  await addSegment("Ones", "/page-1");
  await closeSettings();
  await waitFor(() => expect(t.results.segmentCalls.length).toBeGreaterThan(0));
  const before = t.results.segmentCalls.length;
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await waitFor(() => expect(t.results.segmentCalls.length).toBe(before + 1));
  expect(t.results.segmentCalls.at(-1)?.map((s) => s.name)).toEqual(["Ones"]);
});
