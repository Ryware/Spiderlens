// The raw against rendered comparison (spec 014 FR-028, US3 scenario 6):
// differences side by side, next and previous difference, copy each side,
// and the states (no differences, not stored, more than one fetch).
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { DiffHunk } from "../generated/DiffHunk";
import type { HtmlDiff } from "../generated/HtmlDiff";
import { fakeRender } from "../testing/fakeResults";
import { fakeTransport } from "../testing/fakeTransport";
import { HtmlDiffSheet } from "./HtmlDiff";

const PATH = "/crawls/example.com.spiderlens";
const URL = "https://example.com/page-1";

const hunk = (kind: DiffHunk["kind"], raw: string[], rendered: string[], at: number): DiffHunk => ({
  raw_start: at,
  raw_lines: raw,
  rendered_start: at,
  rendered_lines: rendered,
  kind,
  context_before: [],
});

const DIFF: HtmlDiff = {
  hunks: [
    hunk("changed", ["Loading"], ["Red Shoes | Shop"], 3),
    hunk("added", [], ['<a href="/only-js-1">', "First", "</a>"], 20),
    hunk("removed", ['<meta name="robots" content="noindex">'], [], 5),
  ],
  total_hunks: 3,
  raw_bytes: 2048,
  rendered_bytes: 4096,
  raw_lines: 40,
  rendered_lines: 44,
};

function open(diff: HtmlDiff | null = DIFF) {
  const t = fakeTransport();
  t.results.htmlDiff = diff;
  const onOpenChange = vi.fn();
  render(<HtmlDiffSheet transport={t} path={PATH} id={2} url={URL} open onOpenChange={onOpenChange} />);
  return t;
}

const differences = () => screen.findByRole("region", { name: "Differences" });
const status = () => screen.getByTestId("diff-count").textContent;

test("shows_each_difference_with_the_raw_and_rendered_lines_side_by_side", async () => {
  open();
  const all = await differences();
  expect(within(all).getAllByRole("region")).toHaveLength(3);
  const first = within(all).getByRole("region", { name: /^Difference 1:/ });
  expect(within(within(first).getByLabelText("Raw HTML")).getByText("Loading")).toBeTruthy();
  expect(within(within(first).getByLabelText("Rendered HTML")).getByText("Red Shoes | Shop")).toBeTruthy();
  // A side with no lines says so.
  const added = within(all).getByRole("region", { name: /^Difference 2:/ });
  expect(within(within(added).getByLabelText("Raw HTML")).getByText("nothing")).toBeTruthy();
  expect(within(added).getByText("Only in the rendered HTML")).toBeTruthy();
  expect(screen.getByTestId("diff-facts").textContent).toContain("40 lines");
});

test("next_and_previous_move_through_the_differences_and_wrap", async () => {
  open();
  await differences();
  expect(status()).toBe("Difference 1 of 3");
  const current = () => screen.getByRole("region", { name: /Difference \d:/, current: true }).getAttribute("aria-label");
  expect(current()).toMatch(/^Difference 1:/);
  fireEvent.click(screen.getByRole("button", { name: "Next difference" }));
  expect(status()).toBe("Difference 2 of 3");
  expect(current()).toMatch(/^Difference 2:/);
  fireEvent.click(screen.getByRole("button", { name: "Next difference" }));
  fireEvent.click(screen.getByRole("button", { name: "Next difference" }));
  expect(status()).toBe("Difference 1 of 3");
  fireEvent.click(screen.getByRole("button", { name: "Previous difference" }));
  expect(status()).toBe("Difference 3 of 3");
});

test("pages_with_the_same_content_say_there_are_no_differences", async () => {
  open({ ...DIFF, hunks: [], total_hunks: 0 });
  // Said twice: in the difference counter and as the empty state's title.
  await waitFor(() => expect(screen.getByTestId("diff-count").textContent).toBe("No differences"));
  expect(screen.getAllByText("No differences")).toHaveLength(2);
  expect(screen.getByText(/the raw and rendered HTML are the same/)).toBeTruthy();
  expect((screen.getByRole("button", { name: "Next difference" }) as HTMLButtonElement).disabled).toBe(true);
});

test("a_version_that_is_not_stored_is_explained", async () => {
  open(null);
  expect((await screen.findByRole("alert")).textContent).toContain("rendered HTML of this page is not stored");
});

test("more_differences_are_fetched_on_request", async () => {
  const many = Array.from({ length: 205 }, (_, i) => hunk("changed", [`a${i}`], [`b${i}`], i));
  open({ ...DIFF, hunks: many, total_hunks: 205 });
  const all = await differences();
  expect(within(all).getAllByRole("region")).toHaveLength(200);
  fireEvent.click(screen.getByRole("button", { name: /Show more \(5 left\)/ }));
  await waitFor(() => expect(within(screen.getByRole("region", { name: "Differences" })).getAllByRole("region")).toHaveLength(205));
  expect(status()).toBe("Difference 1 of 205");
});

describe("copy", () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  test("copies_each_version_as_stored", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    const t = open();
    t.results.features.push("raw_html");
    t.results.renders.set(2, fakeRender());
    await differences();
    fireEvent.click(screen.getByRole("button", { name: "Copy rendered" }));
    expect(await screen.findByText("Copied the rendered HTML.")).toBeTruthy();
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("<title>Rendered 2</title>"));
    fireEvent.click(screen.getByRole("button", { name: "Copy raw" }));
    expect(await screen.findByText("Copied the raw HTML.")).toBeTruthy();
    expect(writeText).toHaveBeenCalledTimes(2);
  });

  test("a_version_that_is_not_stored_cannot_be_copied", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn(async () => {}) } });
    open();
    await differences();
    fireEvent.click(screen.getByRole("button", { name: "Copy raw" }));
    expect(await screen.findByText("The raw HTML is not stored.")).toBeTruthy();
  });

  test("a_refused_copy_says_so", async () => {
    const t = open();
    t.results.features.push("raw_html");
    await differences();
    fireEvent.click(screen.getByRole("button", { name: "Copy raw" }));
    expect(await screen.findByText("Could not copy.")).toBeTruthy();
  });
});
