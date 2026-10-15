// Crawl speed in the window (spec 008 FR-021, FR-028, FR-029): what keeps
// the crawl from going faster in the status bar, every site's pace in its
// popover, and the slowdowns in a finished crawl's overview.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import type { CrawlReport, SitePace, SpeedLimit } from "../transport";

const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const status = () => screen.getByRole("contentinfo", { name: "Crawl status" });
const speed = () => status().querySelector<HTMLButtonElement>('[data-figure="speed"]');

async function startCrawl(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

const site = (s: Partial<SitePace> & { host: string }): SitePace => ({
  state: "steady",
  allowance: 8,
  max: 8,
  in_flight: 8,
  response_ms: 120,
  crawled: 1500,
  waiting: 300,
  waiting_more: false,
  reason: "At the per-site cap of 8 at once.",
  seconds_left: null,
  ...s,
});

const wait = (seconds: number): SpeedLimit => ({
  kind: "requested_wait",
  host: "https://example.com:443",
  seconds_left: seconds,
  message: "https://example.com:443 asked us to wait (Retry-After).",
});

async function openSpeed() {
  fireEvent.click(speed()!);
  return screen.findByRole("table", { name: "Sites" });
}

test("status_bar_names_what_limits_the_crawl_and_lists_every_site", async () => {
  // FR-028, FR-029
  const t = await startCrawl();
  act(() =>
    t.crawls.progress({
      crawled: 1500,
      limit: wait(42),
      sites: [
        site({ host: "https://example.com:443", state: "waiting", allowance: 2, in_flight: 0, waiting: 4096, waiting_more: true, seconds_left: 42, reason: "Asked us to wait (Retry-After): 42 s left." }),
        site({ host: "http://cdn.example.com:8080", state: "speeding_up", allowance: 3, response_ms: null, crawled: 12, waiting: 7, reason: "3 of 8 at once, one more each time a round of answers comes back steady." }),
      ],
      sites_total: 5,
    }),
  );
  expect(speed()?.textContent).toBe("Site asked to wait · example.com · 00:42");
  expect(speed()?.dataset.limit).toBe("requested_wait");

  const table = await openSpeed();
  expect(screen.getByText("example.com asked us to wait (Retry-After).")).toBeTruthy();
  const [, first, second] = within(table).getAllByRole("row");
  expect(first.dataset.site).toBe("https://example.com:443");
  expect(within(first).getByText("example.com")).toBeTruthy();
  expect(first.querySelector("[data-state]")?.textContent).toBe("Waiting 00:42");
  expect(first.querySelector('[data-col="at-once"]')?.textContent).toBe("2 / 8");
  expect(first.querySelector('[data-col="response"]')?.textContent).toBe("120 ms");
  expect(first.querySelector('[data-col="crawled"]')?.textContent).toBe("1,500");
  // Only part of the queue is in memory: there are more on disk.
  expect(first.querySelector('[data-col="waiting"]')?.textContent).toBe("4,096+");
  expect(first.querySelector("[data-reason]")?.textContent).toBe("Asked us to wait (Retry-After): 42 s left.");

  // The port shows when unusual; no response time yet is a dash.
  expect(within(second).getByText("cdn.example.com:8080")).toBeTruthy();
  expect(second.querySelector("[data-state]")?.textContent).toBe("Speeding up");
  expect(second.querySelector('[data-col="response"]')?.textContent).toBe("–");
  expect(second.querySelector('[data-col="waiting"]')?.textContent).toBe("7");
  expect(screen.getByText("3 more sites with less work left.")).toBeTruthy();
});

test("user_caps_read_plainly_and_a_free_crawl_says_full_speed", async () => {
  const t = await startCrawl();
  act(() =>
    t.crawls.progress({
      limit: { kind: "concurrency", host: null, seconds_left: null, message: "At your cap of 10 requests at once." },
      sites: [site({ host: "https://example.com:443" })],
      sites_total: 1,
    }),
  );
  expect(speed()?.textContent).toBe("At your requests cap");
  expect(speed()?.className).not.toContain("text-warn");

  act(() => t.crawls.progress({ limit: { kind: "site_overload", host: "https://example.com:443", seconds_left: null, message: "overloaded" }, sites: [site({ host: "https://example.com:443" })] }));
  expect(speed()?.textContent).toBe("Site overloaded · example.com");
  expect(speed()?.className).toContain("text-warn");

  act(() => t.crawls.progress({ limit: null, sites: [site({ host: "https://example.com:443" })], sites_total: 1 }));
  expect(speed()?.textContent).toBe("Full speed");
  await openSpeed();
  expect(screen.getByText(/Nothing is holding the crawl back/)).toBeTruthy();
  expect(screen.queryByText(/more site/)).toBeNull();
});

test("nothing_shows_without_sites_or_while_waiting_on_paused_sites", async () => {
  const t = await startCrawl();
  act(() => t.crawls.progress({ crawled: 3 }));
  expect(speed()).toBeNull();
  // A crawl waiting on paused sites already says so (spec 004 US4.4).
  act(() =>
    t.crawls.progress({
      waiting_for_host: true,
      paused_hosts: [{ host: "https://example.com:443", reason: "failed", seconds_left: 30, probing: false, given_up: false }],
      limit: { kind: "site_paused", host: "https://example.com:443", seconds_left: 30, message: "paused" },
      sites: [site({ host: "https://example.com:443", state: "paused" })],
    }),
  );
  expect(speed()).toBeNull();
  // A finished crawl has no live speed.
  act(() => t.crawls.progress({ waiting_for_host: false, paused_hosts: [], limit: wait(10), sites: [site({ host: "https://example.com:443" })] }));
  expect(speed()).not.toBeNull();
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  expect(speed()).toBeNull();
});

describe("countdowns", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] }));
  afterEach(() => vi.useRealTimers());

  test("a_requested_wait_counts_down_between_progress_events", async () => {
    const t = await startCrawl();
    act(() => t.crawls.progress({ limit: wait(30), sites: [site({ host: "https://example.com:443", state: "waiting", seconds_left: 30 })] }));
    expect(speed()?.textContent).toBe("Site asked to wait · example.com · 00:30");
    act(() => vi.advanceTimersByTime(5000));
    expect(speed()?.textContent).toBe("Site asked to wait · example.com · 00:25");
    const table = await openSpeed();
    expect(within(table).getAllByRole("row")[1].querySelector("[data-state]")?.textContent).toBe("Waiting 00:25");
    // Over: no time is shown rather than a negative one.
    act(() => vi.advanceTimersByTime(40_000));
    expect(speed()?.textContent).toBe("Site asked to wait · example.com");
  });
});

test("finished_crawl_lists_its_slowdowns", async () => {
  // FR-021: a completed crawl explains why parts of it were slow.
  const t = await startCrawl();
  const report: CrawlReport = fakeReport("completed", 400);
  report.summary.speed_events = [
    { host: "https://example.com:443", summary: "https://example.com:443: cut back 3 times after 429 or 503 answers; asked to wait twice (45 s in total, longest 30 s)." },
    { host: "https://cdn.example.com:443", summary: "https://cdn.example.com:443: paused once after repeated failures." },
  ];
  await act(async () => t.crawls.finish({ report }));
  const overview = screen.getByRole("region", { name: "Overview" });
  const slow = within(overview).getByRole("region", { name: "Slowdowns" });
  const rows = within(slow).getAllByRole("row").slice(1);
  expect(rows.map((r) => r.dataset.speedHost)).toEqual(["https://example.com:443", "https://cdn.example.com:443"]);
  expect(rows[0].textContent).toBe("example.comcut back 3 times after 429 or 503 answers; asked to wait twice (45 s in total, longest 30 s).");
});

test("a_crawl_without_slowdowns_has_no_slowdowns_table", async () => {
  const t = await startCrawl();
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  expect(within(screen.getByRole("region", { name: "Overview" })).queryByRole("region", { name: "Slowdowns" })).toBeNull();
});
