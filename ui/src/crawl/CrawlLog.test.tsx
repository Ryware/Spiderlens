// The crawl log screen (spec 004 FR-035): entries in words, filters by kind
// with counts, an empty state, and live reloads while the crawl runs.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { CrawlLog } from "./CrawlLog";
import { fakeTransport } from "../testing/fakeTransport";
import type { CrawlLogEntry } from "../transport";

const at = Date.UTC(2026, 9, 14, 9, 30, 0);
const entries: CrawlLogEntry[] = [
  { at_ms: at + 3000, kind: "host_resumed", host: "https://cdn.example.com", url: null, message: "cdn.example.com answers again." },
  { at_ms: at + 2000, kind: "failure", host: "https://example.com", url: "https://example.com/down", message: "Server error (5xx) after 3 attempts." },
  { at_ms: at + 1000, kind: "host_paused", host: "https://cdn.example.com", url: null, message: "cdn.example.com failed 5 times in a row; paused for 1 minute." },
  { at_ms: at, kind: "retry", host: "https://example.com", url: "https://example.com/flaky", message: "503: trying again in 1 s (attempt 2 of 3)." },
];

function setup(log: CrawlLogEntry[] | (() => CrawlLogEntry[]), live = false) {
  const t = fakeTransport();
  const read = vi.fn(async () => (typeof log === "function" ? log() : log));
  t.crawlLog = read;
  render(<CrawlLog transport={t} path="/tmp/a.spiderlens" live={live} />);
  return read;
}

const rows = () => within(screen.getByRole("table")).getAllByRole("row").slice(1);

test("entries_read_in_words_newest_first", async () => {
  setup(entries);
  await waitFor(() => expect(rows()).toHaveLength(4));
  const first = within(rows()[0]).getAllByRole("cell");
  expect(first[1].textContent).toBe("Site resumed");
  expect(first[2].textContent).toBe("https://cdn.example.com");
  const failure = within(rows()[1]).getAllByRole("cell");
  expect(failure[1].textContent).toBe("Failed");
  expect(failure[1].className).toContain("text-bad");
  expect(failure[2].textContent).toBe("https://example.com/down");
  expect(failure[3].textContent).toBe("Server error (5xx) after 3 attempts.");
});

test("filters_count_and_narrow_by_kind", async () => {
  setup(entries);
  const sites = await screen.findByRole("button", { name: /^Sites/ });
  await waitFor(() => expect(sites.textContent).toBe("Sites2"));
  expect(screen.getByRole("button", { name: /^Retries/ }).textContent).toBe("Retries1");
  fireEvent.click(sites);
  expect(sites.getAttribute("aria-pressed")).toBe("true");
  expect(rows()).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: /^Crawl/ }));
  expect(screen.getByText("No entries of this kind.")).toBeTruthy();
});

test("a_clean_crawl_says_there_is_nothing_to_report", async () => {
  setup([]);
  expect(await screen.findByText(/Nothing to report/)).toBeTruthy();
});

test("the_log_reloads_while_the_crawl_runs", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    let log = entries.slice(2);
    const read = setup(() => log, true);
    await waitFor(() => expect(rows()).toHaveLength(2));
    log = entries;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    await waitFor(() => expect(rows()).toHaveLength(4));
    expect(read.mock.calls.length).toBeGreaterThanOrEqual(2);
  } finally {
    vi.useRealTimers();
  }
});
