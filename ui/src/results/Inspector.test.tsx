// The detail pane (spec 007 US3): empty state, redirects, errors, not-crawled
// reasons, link lists with paging, and fields the file has no data for.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { LinkRow } from "../generated/LinkRow";
import type { ListKind } from "../generated/ListKind";
import type { UrlDetail } from "../generated/UrlDetail";
import { fakeTransport } from "../testing/fakeTransport";
import { Inspector } from "./Inspector";

const PATH = "/crawls/example.com.spiderlens";

function withDetail(patch: Partial<UrlDetail>) {
  const t = fakeTransport();
  const base = t.urlDetail;
  t.urlDetail = async (p, id) => ({ ...(await base(p, id)), ...patch });
  return t;
}

const details = () => screen.getByRole("complementary", { name: "Details" });
const loaded = () => screen.findByRole("heading", { level: 2 });

const link = (i: number, kind = "a"): LinkRow => ({
  id: 1000 + i,
  url: `https://example.com/linked-${i}`,
  raw: `/linked-${i}`,
  kind,
  nofollow: false,
  followed: true,
  internal: true,
  status: 200,
  final_state: "Crawled",
  text: null,
  sponsored: false,
  ugc: false,
});

test("nothing_selected_shows_an_empty_state", () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={null} />);
  expect(within(details()).getByText("Nothing selected")).toBeTruthy();
  expect(within(details()).getByText(/Select a URL in the results/)).toBeTruthy();
});

test("no_crawl_file_also_shows_the_empty_state", () => {
  render(<Inspector transport={fakeTransport()} path={null} id={3} />);
  expect(within(details()).getByText("Nothing selected")).toBeTruthy();
});

test("shows_the_url_and_its_response_facts", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} />);
  expect((await loaded()).textContent).toBe("https://example.com/page-1");
  expect(within(details()).getByText("200 OK")).toBeTruthy();
  expect(within(details()).getAllByText("text/html").length).toBeGreaterThan(0);
  expect(within(details()).getByText("Start URL")).toBeTruthy();
  expect(within(details()).queryByText("External")).toBeNull();
});

test("redirect_chain_lists_each_hop", async () => {
  const t = withDetail({
    status: 301,
    redirects: [
      { id: 2, url: "http://example.com/a", status: 301, final_state: "Redirected" },
      { id: 3, url: "https://example.com/a", status: 302, final_state: "Redirected" },
      { id: 4, url: "https://example.com/b", status: 200, final_state: "Crawled" },
    ],
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Redirects (2 hops)" })).toBeTruthy();
  const hops = within(details()).getAllByRole("listitem").filter((li) => li.closest("ol"));
  expect(hops.map((h) => h.textContent)).toEqual(["301http://example.com/a", "302https://example.com/a", "200https://example.com/b"]);
  expect(within(hops[0]).getByText("301").className).toContain("text-info");
  expect(within(hops[2]).getByText("200").className).toContain("text-ok");
  expect(within(details()).queryByText("The chain loops back on itself.")).toBeNull();
});

test("single_hop_redirect_says_hop", async () => {
  const t = withDetail({
    redirects: [
      { id: 2, url: "http://example.com/a", status: 301, final_state: "Redirected" },
      { id: 3, url: "https://example.com/a", status: 200, final_state: "Crawled" },
    ],
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Redirects (1 hop)" })).toBeTruthy();
});

test("redirect_loop_and_over_limit_are_called_out", async () => {
  const t = withDetail({
    redirects: [
      { id: 2, url: "https://example.com/a", status: 301, final_state: "Redirected" },
      { id: 3, url: "https://example.com/b", status: 301, final_state: "Redirected" },
      { id: 2, url: "https://example.com/a", status: 301, final_state: "Redirected" },
    ],
    redirect_loop: true,
    redirect_over_limit: true,
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByText("The chain loops back on itself.")).toBeTruthy();
  expect(within(details()).getByText("The chain is longer than the crawl's redirect limit.")).toBeTruthy();
});

test("error_shows_its_label_and_message", async () => {
  const t = withDetail({ final_state: "Failed", status: null, status_text: null, error_class: "timeout", error_label: "Timed out", error_message: "no response after 30 s" });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Error" })).toBeTruthy();
  expect(within(details()).getByText("Timed out: no response after 30 s")).toBeTruthy();
  expect(within(details()).getByText("No response")).toBeTruthy();
  expect(within(details()).getByText("Failed").className).toMatch(/bad/);
});

test("not_crawled_reason_includes_the_robots_rule", async () => {
  const t = withDetail({
    final_state: "Blocked by robots.txt",
    status: null,
    status_text: null,
    not_crawled_reason: "Blocked by robots.txt",
    reason_detail: "user-agent *",
    robots_rule: "Disallow: /private",
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Not crawled" })).toBeTruthy();
  expect(within(details()).getByText("Blocked by robots.txt: user-agent * (rule Disallow: /private)")).toBeTruthy();
});

test("external_urls_get_a_badge", async () => {
  const t = withDetail({ internal: false, url: "https://other.org/", url_readable: "https://other.org/" });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByText("External")).toBeTruthy();
});

test("link_list_switches_between_in_out_and_images", async () => {
  const t = withDetail({ inlinks: 1, outlinks: 2, images: 1 });
  const asked: ListKind[] = [];
  t.urlList = async (_p, _id, kind, from) => {
    asked.push(kind);
    const n = kind === "inlinks" ? 1 : kind === "outlinks" ? 2 : 1;
    return { total: n, from, rows: Array.from({ length: n }, (_, i) => ({ ...link(i), url: `https://example.com/${kind}-${i}` })), headings: [], unavailable: null };
  };
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(await within(details()).findByText("https://example.com/inlinks-0")).toBeTruthy();
  const switcher = within(details()).getByRole("radiogroup", { name: "Link list" });
  fireEvent.click(within(switcher).getByRole("radio", { name: "Out 2" }));
  expect(await within(details()).findByText("https://example.com/outlinks-1")).toBeTruthy();
  fireEvent.click(within(switcher).getByRole("radio", { name: "Images 1" }));
  expect(await within(details()).findByText("https://example.com/images-0")).toBeTruthy();
  expect(asked).toEqual(["inlinks", "outlinks", "images"]);
  expect(within(switcher).getByRole("radio", { name: "Images 1" }).getAttribute("aria-checked")).toBe("true");
});

test("empty_link_list_says_none", async () => {
  const t = withDetail({ inlinks: 1, outlinks: 0 });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  fireEvent.click(within(details()).getByRole("radio", { name: "Out 0" }));
  expect(await within(details()).findByText("None.")).toBeTruthy();
});

test("nofollow_links_are_marked", async () => {
  const t = withDetail({ inlinks: 2 });
  t.urlList = async (_p, _id, _kind, from) => ({ total: 2, from, rows: [{ ...link(0), nofollow: true }, { ...link(1), followed: false }], headings: [], unavailable: null });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(await within(details()).findByText("nofollow")).toBeTruthy();
  expect(within(details()).getByText("not followed")).toBeTruthy();
});

test("show_more_pages_in_the_rest_of_the_list", async () => {
  const t = withDetail({ inlinks: 150 });
  const calls: [number, number][] = [];
  t.urlList = async (_p, _id, _kind, from, count) => {
    calls.push([from, count]);
    const rows = Array.from({ length: Math.min(count, 150 - from) }, (_, i) => link(from + i));
    return { total: 150, from, rows, headings: [], unavailable: null };
  };
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  const more = await within(details()).findByRole("button", { name: "Show more (50 left)" });
  expect(within(details()).getByText("https://example.com/linked-99")).toBeTruthy();
  expect(within(details()).queryByText("https://example.com/linked-100")).toBeNull();
  fireEvent.click(more);
  expect(await within(details()).findByText("https://example.com/linked-149")).toBeTruthy();
  expect(within(details()).getByText("https://example.com/linked-0")).toBeTruthy();
  expect(within(details()).queryByRole("button", { name: /Show more/ })).toBeNull();
  expect(calls).toEqual([
    [0, 100],
    [100, 100],
  ]);
});

test("link_list_error_shows_the_message", async () => {
  const t = fakeTransport();
  t.urlList = async () => {
    throw new Error("database is locked");
  };
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(await within(details()).findByText("Error: database is locked")).toBeTruthy();
});

test("link_list_unavailable_explains_why", async () => {
  const t = fakeTransport();
  t.urlList = async (_p, _id, _kind, from) => ({ total: 0, from, rows: [], headings: [], unavailable: "Links are not stored in this file." });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(await within(details()).findByText("Links are not stored in this file.")).toBeTruthy();
});

test("detail_error_shows_the_message", async () => {
  const t = fakeTransport();
  t.urlDetail = async () => {
    throw new Error("no such URL");
  };
  render(<Inspector transport={t} path={PATH} id={2} />);
  expect(await within(details()).findByText("Error: no such URL")).toBeTruthy();
});

test("not_collected_group_lists_unavailable_fields", async () => {
  const t = withDetail({
    unavailable: {
      headings: "Not collected yet: arrives with page extraction.",
      anchor_text: "Not collected yet: arrives with page extraction.",
      raw_html: "This crawl did not keep page HTML.",
    },
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Not collected" })).toBeTruthy();
  expect(within(details()).getByText("Headings, Anchor and alt text.")).toBeTruthy();
  // Page HTML is explained in its own section instead.
  expect(within(details()).queryByText("Page HTML.")).toBeNull();
  expect(within(details()).queryByText(/This crawl did not keep page HTML\./)).toBeNull();
});

test("page_html_not_stored_says_how_to_turn_it_on", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).getByRole("heading", { name: "Page HTML" })).toBeTruthy();
  expect(within(details()).getByText(/Turn on Settings › Page data › Store page HTML and crawl again/)).toBeTruthy();
  expect(within(details()).queryByRole("button", { name: "View source" })).toBeNull();
});

test("page_html_missing_for_a_url_without_a_body", async () => {
  const t = fakeTransport();
  t.results.features = ["base", "extraction", "raw_html"];
  render(<Inspector transport={t} path={PATH} id={10} />);
  await loaded();
  expect(within(details()).getByText(/None stored for this URL: it is not an HTML page/)).toBeTruthy();
  expect(within(details()).queryByRole("button", { name: "View source" })).toBeNull();
});

test("view_source_opens_the_page_html_viewer", async () => {
  const t = fakeTransport();
  t.results.features = ["base", "extraction", "raw_html"];
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  fireEvent.click(within(details()).getByRole("button", { name: "View source" }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByText("<!doctype html>")).toBeTruthy();
  expect(within(dialog).getByText("https://example.com/page-1", { selector: "p" })).toBeTruthy();
  expect(within(dialog).getByTestId("html-facts").textContent).toMatch(/^\d+ KB · 2,008 lines · UTF-8$/);
  fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("nothing_missing_hides_the_not_collected_group", async () => {
  const t = withDetail({ unavailable: {} });
  render(<Inspector transport={t} path={PATH} id={2} />);
  await loaded();
  expect(within(details()).queryByRole("heading", { name: "Not collected" })).toBeNull();
});

test("changing_the_selected_url_loads_its_details", async () => {
  const t = fakeTransport();
  const { rerender } = render(<Inspector transport={t} path={PATH} id={2} />);
  expect((await loaded()).textContent).toBe("https://example.com/page-1");
  rerender(<Inspector transport={t} path={PATH} id={5} />);
  await waitFor(() => expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("https://example.com/page-4"));
});

// Copy URL and Show in table on list rows (T027).

test("copy_url_puts_the_row_url_on_the_clipboard_and_says_so", async () => {
  const t = fakeTransport();
  render(<Inspector transport={t} path={PATH} id={2} />);
  fireEvent.click(await within(details()).findByRole("button", { name: "Copy https://example.com/" }));
  await waitFor(() => expect(t.results.copied).toEqual(["https://example.com/"]));
  expect(within(details()).getByRole("status").textContent).toBe("Copied.");
});

test("copy_failure_shows_the_error", async () => {
  const t = fakeTransport();
  t.copyText = async () => {
    throw "Could not copy: no clipboard";
  };
  render(<Inspector transport={t} path={PATH} id={2} />);
  fireEvent.click(await within(details()).findByRole("button", { name: "Copy https://example.com/" }));
  await waitFor(() => expect(within(details()).getByRole("status").textContent).toBe("Could not copy: no clipboard"));
});

test("show_in_table_hands_over_the_row_url_id", async () => {
  const show = vi.fn(async () => true);
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} onShowInTable={show} />);
  fireEvent.click(await within(details()).findByRole("button", { name: "Show https://example.com/ in table" }));
  await waitFor(() => expect(show).toHaveBeenCalledWith(1));
  expect(within(details()).getByRole("status").textContent).toBe("");
});

test("show_in_table_says_when_the_url_is_in_no_table", async () => {
  const show = vi.fn(async () => false);
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} onShowInTable={show} />);
  fireEvent.click(await within(details()).findByRole("button", { name: "Show https://example.com/ in table" }));
  await waitFor(() => expect(within(details()).getByRole("status").textContent).toBe("That URL is not in any results table of this crawl."));
});

test("show_in_table_is_left_out_without_a_table", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} />);
  await within(details()).findByRole("button", { name: "Copy https://example.com/" });
  expect(within(details()).queryByRole("button", { name: /in table$/ })).toBeNull();
});

test("redirect_hops_have_the_same_actions", async () => {
  const t = withDetail({
    redirects: [
      { id: 2, url: "http://example.com/a", status: 301, final_state: "Redirected" },
      { id: 4, url: "https://example.com/b", status: 200, final_state: "Crawled" },
    ],
  });
  const show = vi.fn(async () => true);
  render(<Inspector transport={t} path={PATH} id={2} onShowInTable={show} />);
  fireEvent.click(await within(details()).findByRole("button", { name: "Show https://example.com/b in table" }));
  await waitFor(() => expect(show).toHaveBeenCalledWith(4));
  fireEvent.click(within(details()).getByRole("button", { name: "Copy http://example.com/a" }));
  await waitFor(() => expect(t.results.copied).toEqual(["http://example.com/a"]));
});

test("right_click_on_a_row_offers_the_same_actions", async () => {
  const t = fakeTransport();
  const show = vi.fn(async () => true);
  render(<Inspector transport={t} path={PATH} id={2} onShowInTable={show} />);
  const row = (await within(details()).findByRole("button", { name: "Copy https://example.com/" })).closest("li")!;
  fireEvent.contextMenu(row);
  fireEvent.click(await screen.findByRole("menuitem", { name: "Show in table" }));
  await waitFor(() => expect(show).toHaveBeenCalledWith(1));
  fireEvent.contextMenu(row);
  fireEvent.click(await screen.findByRole("menuitem", { name: "Copy URL" }));
  await waitFor(() => expect(t.results.copied).toEqual(["https://example.com/"]));
});
