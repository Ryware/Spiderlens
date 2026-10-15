// The visualisation view (spec 022 US1, US2, US6) over the fake scene
// builder: what it asks Rust for, and what it shows.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { fakeTransport } from "../testing/fakeTransport";
import { VizView } from "./VizView";

const PATH = "/crawls/example.com.spiderlens";

function setup(view: "crawl_tree" | "directory_tree" | "link_graph" = "crawl_tree", live = false) {
  const t = fakeTransport();
  const onFocusId = vi.fn();
  const utils = render(<VizView transport={t} path={PATH} view={view} live={live} focusId={null} onFocusId={onFocusId} />);
  return { t, onFocusId, ...utils };
}

const canvas = () => screen.getByRole("application");
const last = (t: ReturnType<typeof fakeTransport>) => t.results.viz[t.results.viz.length - 1];

test("first_scene_is_the_default_state", async () => {
  const { t } = setup();
  await screen.findByRole("application");
  expect(t.results.viz[0]).toEqual({ view: "crawl_tree", layout: "tree", colour: "status", size: "uniform", expanded: null, pages: {}, budget: null, focus: null, distance: 1, pinned: {} });
  expect(canvas().getAttribute("aria-label")).toMatch(/^Crawl tree: \d+ nodes/);
});

test("legend_counts_and_what_was_left_out", async () => {
  setup();
  const legend = await screen.findByRole("group", { name: "Legend" });
  expect(within(legend).getByText("Success (2xx)")).toBeTruthy();
  expect(within(legend).getByText("27")).toBeTruthy();
  expect(within(legend).getByText("3")).toBeTruthy();
  // 30 fake URLs and a budget of 20 nodes: some branches stay folded.
  const note = screen.getByRole("note");
  expect(note.textContent).toMatch(/^Showing \d+ of 30 URLs\. The rest are in folded branches/);
});

test("layout_colour_and_size_ask_for_a_new_scene", async () => {
  const { t } = setup();
  await screen.findByRole("application");
  fireEvent.click(screen.getByRole("radio", { name: "Radial" }));
  await waitFor(() => expect(last(t).layout).toBe("radial"));
  fireEvent.change(screen.getByRole("combobox", { name: "Colour" }), { target: { value: "depth" } });
  await waitFor(() => expect(last(t).colour).toBe("depth"));
  expect(screen.getByText("Colour: crawl depth, darker is closer to the start")).toBeTruthy();
  fireEvent.change(screen.getByRole("combobox", { name: "Size" }), { target: { value: "score" } });
  await waitFor(() => expect(last(t).size).toBe("score"));
  expect(await screen.findByText("Size: same for all (Link Score not worked out yet)")).toBeTruthy();
});

test("arrow_keys_select_and_the_inspector_follows", async () => {
  const { onFocusId } = setup();
  const c = await screen.findByRole("application");
  fireEvent.keyDown(c, { key: "ArrowRight" });
  // Nothing selected yet: the first key selects the root.
  expect(onFocusId).toHaveBeenLastCalledWith(1);
  fireEvent.keyDown(c, { key: "ArrowRight" });
  expect(onFocusId).toHaveBeenCalledTimes(2);
  expect(onFocusId.mock.calls[1][0]).not.toBe(1);
});

test("enter_on_a_folded_branch_opens_it", async () => {
  const { t } = setup();
  const c = await screen.findByRole("application");
  const scene = await t.vizScene(PATH, t.results.viz[0]);
  const folded = scene.nodes.find((n) => n.expandable && !n.expanded)!;
  expect(folded).toBeTruthy();
  // Walk to it with the find box, then Enter.
  fireEvent.change(screen.getByRole("textbox", { name: "Find in view" }), { target: { value: folded.label } });
  fireEvent.submit(screen.getByRole("textbox", { name: "Find in view" }).closest("form")!);
  fireEvent.keyDown(c, { key: "Enter" });
  await waitFor(() => expect(last(t).expanded).toContain(folded.key));
  // Every node that was open stays open.
  const open = scene.nodes.filter((n) => n.expanded).map((n) => n.key);
  for (const k of open) expect(last(t).expanded).toContain(k);
  // Enter again, on the now open branch, closes it. (The test's own
  // vizScene call above is the second request.)
  await waitFor(() => expect(t.results.viz.length).toBe(3));
  await act(async () => {});
  fireEvent.keyDown(screen.getByRole("application"), { key: "Enter" });
  await waitFor(() => expect(last(t).expanded).not.toContain(folded.key));
});

test("directory_tree_asks_for_its_own_view", async () => {
  const { t } = setup("directory_tree");
  await screen.findByRole("application");
  expect(t.results.viz[0].view).toBe("directory_tree");
  expect(canvas().getAttribute("aria-label")).toMatch(/^Directory tree/);
});

test("live_crawls_show_the_snapshot_time_and_refresh_rebuilds", async () => {
  const { t } = setup("crawl_tree", true);
  await screen.findByRole("application");
  expect(screen.getByText(/^As of /)).toBeTruthy();
  const before = t.results.viz.length;
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(t.results.viz.length).toBe(before + 1));
});

test("a_failed_scene_says_why", async () => {
  const t = fakeTransport();
  t.vizScene = async () => {
    throw new Error("disk read failed");
  };
  render(<VizView transport={t} path={PATH} view="crawl_tree" live={false} focusId={null} onFocusId={() => {}} />);
  expect(await screen.findByText("Error: disk read failed")).toBeTruthy();
});

test("selecting_in_the_table_selects_in_the_view", async () => {
  const t = fakeTransport();
  const { rerender } = render(<VizView transport={t} path={PATH} view="crawl_tree" live={false} focusId={null} onFocusId={() => {}} />);
  const c = await screen.findByRole("application");
  rerender(<VizView transport={t} path={PATH} view="crawl_tree" live={false} focusId={2} onFocusId={() => {}} />);
  // Enter acts on the selection: u:2 starts open, so it closes.
  await act(async () => {});
  fireEvent.keyDown(c, { key: "Enter" });
  await waitFor(() => expect(t.results.viz.length).toBe(2));
  expect(t.results.viz[1].expanded).not.toContain("u:2");
  expect(t.results.viz[1].expanded).toContain("u:1");
});

test("link_graph_sizes_by_score_and_draws_links", async () => {
  const { t } = setup("link_graph");
  await screen.findByRole("application");
  expect(t.results.viz[0]).toMatchObject({ view: "link_graph", layout: "force", size: "score", focus: null, distance: 1, pinned: {} });
  expect(canvas().getAttribute("aria-label")).toMatch(/^Link graph: 20 nodes, \d+ links\. .*Enter centres on a page/);
  // No tree layouts here; the focus needs a selected page first.
  expect(screen.queryByRole("radio", { name: "Radial" })).toBeNull();
  expect((screen.getByRole("button", { name: "Focus on selected" }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole("note").textContent).toMatch(/^Showing 20 of 30 URLs\. Only the pages with the most links in/);
});

test("link_graph_focus_distance_and_whole_site", async () => {
  const { t } = setup("link_graph");
  const c = await screen.findByRole("application");
  fireEvent.keyDown(c, { key: "ArrowRight" });
  fireEvent.click(screen.getByRole("button", { name: "Focus on selected" }));
  await waitFor(() => expect(last(t).focus).toBe("u:1"));
  expect(await screen.findByText(/^Around /)).toBeTruthy();
  // The note about left-out pages is for the whole site only.
  await waitFor(() => expect(screen.queryByRole("note")).toBeNull());
  fireEvent.click(screen.getByRole("radio", { name: "2 links" }));
  await waitFor(() => expect(last(t).distance).toBe(2));
  fireEvent.click(screen.getByRole("button", { name: "Whole site" }));
  await waitFor(() => expect(last(t).focus).toBeNull());
  // Enter on a page centres on it too.
  await act(async () => {});
  fireEvent.keyDown(screen.getByRole("application"), { key: "ArrowRight" });
  fireEvent.keyDown(screen.getByRole("application"), { key: "Enter" });
  await waitFor(() => expect(last(t).focus).toMatch(/^u:\d+$/));
});

test("dragging_a_page_pins_it_and_reset_layout_clears", async () => {
  const { t } = setup("link_graph");
  const c = await screen.findByRole("application");
  const scene = await t.vizScene(PATH, t.results.viz[0]);
  await act(async () => {});
  const [cx, cy, k] = c.getAttribute("data-camera")!.split(",").map(Number);
  const n = scene.nodes[3];
  const at = { x: n.x * k + cx, y: n.y * k + cy };
  fireEvent.pointerDown(c, { clientX: at.x, clientY: at.y, pointerId: 1 });
  fireEvent.pointerMove(c, { clientX: at.x + 40, clientY: at.y + 30, pointerId: 1 });
  fireEvent.pointerUp(c, { clientX: at.x + 40, clientY: at.y + 30, pointerId: 1 });
  await waitFor(() => expect(Object.keys(last(t).pinned)).toEqual([n.key]));
  const [px, py] = last(t).pinned[n.key];
  expect(px).toBeCloseTo(n.x + 40 / k, 3);
  expect(py).toBeCloseTo(n.y + 30 / k, 3);
  fireEvent.click(screen.getByRole("button", { name: "Reset layout" }));
  await waitFor(() => expect(last(t).pinned).toEqual({}));
});
