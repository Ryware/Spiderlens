// The page HTML viewer (spec 007 T029): row layout and search helpers, the
// states (loading, error, not stored, no body), windowing of long sources,
// search with next/previous, and copy.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { PageHtml } from "../generated/PageHtml";
import { fakeTransport } from "../testing/fakeTransport";
import { fakePageHtml } from "../testing/fakeResults";
import { findMatches, HtmlViewer, layoutRows } from "./HtmlViewer";

const PATH = "/crawls/example.com.spiderlens";
const URL = "https://example.com/page-1";

const page = (html: string | null, patch: Partial<PageHtml> = {}): PageHtml => ({
  html,
  missing: html === null ? "no_body" : null,
  bytes: html?.length ?? 0,
  encoding: html === null ? null : "UTF-8",
  encoding_errors: false,
  ...patch,
});

function withHtml(answer: () => Promise<PageHtml>) {
  const t = fakeTransport();
  t.urlHtml = answer;
  return t;
}

function open(t = fakeTransport()) {
  const onOpenChange = vi.fn();
  render(<HtmlViewer transport={t} path={PATH} id={2} url={URL} open onOpenChange={onOpenChange} />);
  return { dialog: screen.getByRole("dialog"), onOpenChange };
}

const source = () => screen.findByRole("region", { name: "Page source" });
const rowsShown = (el: HTMLElement) => el.querySelectorAll("[data-row]");
const count = () => screen.getByTestId("match-count").textContent;

const DOC = fakePageHtml({ id: 2, url: URL, status: 200, title: "Page 1", depth: 1, internal: true });

describe("layoutRows", () => {
  test("one_row_per_line_with_line_numbers", () => {
    const r = layoutRows("a\nbb\r\n\nc");
    expect(r.lines).toBe(4);
    expect([...r.line]).toEqual([1, 2, 3, 4]);
    expect([...r.start].map((s, i) => "a\nbb\r\n\nc".slice(s, r.end[i]))).toEqual(["a", "bb", "", "c"]);
  });

  test("a_final_line_break_adds_no_line", () => {
    expect(layoutRows("a\nb\n").lines).toBe(2);
    expect(layoutRows("").lines).toBe(1);
  });

  test("long_lines_wrap_into_unnumbered_rows", () => {
    const text = "x".repeat(25) + "\ny";
    const r = layoutRows(text, 10);
    expect([...r.line]).toEqual([1, 0, 0, 2]);
    expect([...r.end].map((e, i) => e - r.start[i])).toEqual([10, 10, 5, 1]);
  });

  test("wrapping_never_splits_a_surrogate_pair", () => {
    const text = "abc😀def";
    const r = layoutRows(text, 4);
    expect([...r.start].map((s, i) => text.slice(s, r.end[i]))).toEqual(["abc", "😀de", "f"]);
  });
});

describe("findMatches", () => {
  test("ignores_case_and_treats_the_query_as_text", () => {
    const m = findMatches("<A href> a.b axb A.B", "a.b");
    expect([...m.start]).toEqual([9, 17]);
    expect([...m.end]).toEqual([12, 20]);
    expect(findMatches("abc", "").start.length).toBe(0);
  });

  test("stops_counting_at_the_cap", () => {
    const m = findMatches("aaaaa", "a", 3);
    expect(m.start.length).toBe(3);
    expect(m.capped).toBe(true);
  });
});

test("loading_then_the_source", async () => {
  let resolve!: (p: PageHtml) => void;
  const { dialog } = open(withHtml(() => new Promise((r) => (resolve = r))));
  expect(within(dialog).getByText("Loading…")).toBeTruthy();
  expect(within(dialog).getByRole("button", { name: "Copy" })).toHaveProperty("disabled", true);
  await act(async () => resolve(page("<p>Hello</p>\n")));
  expect(within(await source()).getByText("<p>Hello</p>")).toBeTruthy();
  expect(within(dialog).getByTestId("html-facts").textContent).toBe("13 B · 1 line · UTF-8");
});

test("load_error_shows_the_message", async () => {
  const { dialog } = open(withHtml(() => Promise.reject(new Error("stored HTML is damaged"))));
  expect((await within(dialog).findByRole("alert")).textContent).toBe("Error: stored HTML is damaged");
});

test("not_stored_explains_the_setting_and_a_new_crawl", async () => {
  const { dialog } = open(withHtml(async () => page(null, { missing: "not_stored" })));
  expect(await within(dialog).findByText("Page HTML was not stored")).toBeTruthy();
  expect(within(dialog).getByText(/Settings › Page data › Store page HTML, then run a new crawl/)).toBeTruthy();
  expect(within(dialog).getByLabelText("Find in source")).toHaveProperty("disabled", true);
});

test("no_body_says_it_is_not_an_html_page", async () => {
  const { dialog } = open(withHtml(async () => page(null)));
  expect(await within(dialog).findByText("No HTML for this URL")).toBeTruthy();
  expect(within(dialog).getByText(/not an HTML page, or no body was received/)).toBeTruthy();
});

test("encoding_errors_are_called_out", async () => {
  const { dialog } = open(withHtml(async () => page("<p>caf�</p>", { encoding: "windows-1252", encoding_errors: true })));
  expect(await within(dialog).findByText("Some bytes are not valid windows-1252 and show as �.")).toBeTruthy();
});

test("long_sources_render_only_the_rows_on_screen", async () => {
  const lines = Array.from({ length: 100_000 }, (_, i) => `<p>${i}</p>`).join("\n");
  open(withHtml(async () => page(lines)));
  const el = await source();
  expect(rowsShown(el).length).toBeGreaterThan(10);
  expect(rowsShown(el).length).toBeLessThan(100);
  expect(within(el).getByText("<p>0</p>")).toBeTruthy();
  // Line numbers sit in the gutter.
  expect(rowsShown(el)[0].firstElementChild!.textContent).toBe("1");
  fireEvent.scroll(el, { target: { scrollTop: 50_000 * 20 } });
  expect(within(el).getByText("<p>50000</p>")).toBeTruthy();
  expect(within(el).queryByText("<p>0</p>")).toBeNull();
});

test("search_counts_marks_and_steps_through_matches", async () => {
  open(withHtml(async () => page(DOC)));
  const el = await source();
  const input = screen.getByLabelText("Find in source");
  fireEvent.change(input, { target: { value: "PARAGRAPH 1" } });
  // 1, 10-19, 100-199 and 1000-1999.
  await waitFor(() => expect(count()).toBe("1 of 1,111"));
  expect(el.querySelector("mark[data-current]")!.textContent).toBe("Paragraph 1");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(count()).toBe("2 of 1,111");
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  expect(count()).toBe("1,111 of 1,111");
  // The last match is far down: the view scrolled to it.
  const current = el.querySelector("mark[data-current]")!;
  expect(current.parentElement!.parentElement!.firstElementChild!.textContent).toBe("2005");
  fireEvent.click(screen.getByRole("button", { name: "Next match" }));
  expect(count()).toBe("1 of 1,111");
  fireEvent.click(screen.getByRole("button", { name: "Previous match" }));
  expect(count()).toBe("1,111 of 1,111");

  fireEvent.change(input, { target: { value: "no such text" } });
  await waitFor(() => expect(count()).toBe("No matches"));
  expect(screen.getByRole("button", { name: "Next match" })).toHaveProperty("disabled", true);
  expect(el.querySelector("mark")).toBeNull();
});

test("a_match_across_a_wrapped_row_is_marked_on_both_rows", async () => {
  open(withHtml(async () => page(`${"x".repeat(158)}needle`)));
  const el = await source();
  fireEvent.change(screen.getByLabelText("Find in source"), { target: { value: "needle" } });
  await waitFor(() => expect(count()).toBe("1 of 1"));
  expect([...el.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["ne", "edle"]);
});

test("mod_f_focuses_the_search", async () => {
  const { dialog } = open(withHtml(async () => page(DOC)));
  const el = await source();
  el.focus();
  fireEvent.keyDown(el, { key: "f", code: "KeyF", ctrlKey: true });
  expect(document.activeElement).toBe(within(dialog).getByLabelText("Find in source"));
});

describe("copy", () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  test("copies_the_whole_source", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    open(withHtml(async () => page(DOC)));
    await source();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByText("Copied.")).toBeTruthy();
    expect(writeText).toHaveBeenCalledWith(DOC);
  });

  test("a_refused_copy_says_so", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => Promise.reject(new Error("denied")) } });
    open(withHtml(async () => page(DOC)));
    await source();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByText("Could not copy.")).toBeTruthy();
  });

  test("no_clipboard_says_so", async () => {
    open(withHtml(async () => page(DOC)));
    await source();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByText("Could not copy.")).toBeTruthy();
  });
});

test("fake_transport_follows_the_crawl_features", async () => {
  const t = fakeTransport();
  expect((await t.urlHtml(PATH, 2)).missing).toBe("not_stored");
  t.results.features = ["base", "raw_html"];
  expect((await t.urlHtml(PATH, 10)).missing).toBe("no_body");
  const p = await t.urlHtml(PATH, 2);
  expect(p.html).toContain("<title>Page 1</title>");
});
