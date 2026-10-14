// Fetch errors and retries in the window (spec 004 US4, US5, FR-028 to
// FR-036): paused sites in the status bar, notices, "Retry failed URLs",
// settings applied to a running crawl, and the summary counts.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import type { CrawlReport, PausedHost } from "../transport";

const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const status = () => screen.getByRole("contentinfo", { name: "Crawl status" });
const figure = (label: string) => status().querySelector(`[data-figure="${label}"]`)?.textContent;
const PATH = "/crawls/example.com.spiderlens";

async function startCrawl(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

const host = (h: Partial<PausedHost> & { host: string }): PausedHost => ({ reason: "5 requests in a row failed", seconds_left: 42, probing: false, given_up: false, ...h });

async function palette(query: string) {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  return screen.getByRole("dialog", { name: "Command palette" });
}

async function openSettings(category: string) {
  let nav = screen.queryByRole("navigation", { name: "Settings categories" });
  if (!nav) {
    fireEvent.click(button("Configure"));
    nav = await screen.findByRole("navigation", { name: "Settings categories" });
  }
  fireEvent.click(within(nav).getByRole("tab", { name: new RegExp(`^${category}`) }));
}

const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
// Controls in a disabled fieldset are disabled without their own attribute.
const locked = (label: string) => input(label).matches(":disabled");

test("status_bar_lists_paused_sites_and_resumes_one", async () => {
  // FR-028, US4.1, US4.5
  const t = await startCrawl();
  act(() =>
    t.crawls.progress({
      crawled: 20,
      waiting: 3,
      paused_hosts: [host({ host: "https://media.example.com:443" }), host({ host: "http://old.example.com:8080", given_up: true, seconds_left: 0 })],
    }),
  );
  expect(figure("waiting")).toBe("3 waiting to retry");
  expect(figure("paused")).toBe("1 site paused · 1 given up");

  fireEvent.click(status().querySelector<HTMLButtonElement>('[data-figure="paused"]')!);
  const list = await screen.findByRole("list", { name: "Paused sites" });
  const [media, old] = within(list).getAllByRole("listitem");
  expect(within(media).getByText("media.example.com")).toBeTruthy();
  expect(within(media).getByText("5 requests in a row failed")).toBeTruthy();
  expect(media.querySelector("[data-pause-state]")?.textContent).toBe("00:42");
  // A site given up has nothing left to resume; the port shows when unusual.
  expect(within(old).getByText("old.example.com:8080")).toBeTruthy();
  expect(within(old).getByText("Given up")).toBeTruthy();
  expect(within(old).queryByRole("button")).toBeNull();

  fireEvent.click(within(media).getByRole("button", { name: "Resume media.example.com now" }));
  expect(t.crawls.resumedHosts).toEqual(["1 https://media.example.com:443"]);
  expect(media.querySelector("[data-pause-state]")?.textContent).toBe("Testing…");
  expect((within(media).getByRole("button", { name: /Resume/ }) as HTMLButtonElement).disabled).toBe(true);
});

test("no_paused_sites_and_no_waiting_shows_nothing_extra", async () => {
  const t = await startCrawl();
  act(() => t.crawls.progress({ crawled: 5 }));
  expect(figure("waiting")).toBeUndefined();
  expect(figure("paused")).toBeUndefined();
});

describe("countdowns", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] }));
  afterEach(() => vi.useRealTimers());

  test("pause_counts_down_between_progress_events", async () => {
    const t = await startCrawl();
    act(() => t.crawls.progress({ paused_hosts: [host({ host: "https://media.example.com:443", seconds_left: 30 })] }));
    fireEvent.click(status().querySelector<HTMLButtonElement>('[data-figure="paused"]')!);
    const row = within(await screen.findByRole("list", { name: "Paused sites" })).getByRole("listitem");
    expect(row.querySelector("[data-pause-state]")?.textContent).toBe("00:30");
    act(() => vi.advanceTimersByTime(5000));
    expect(row.querySelector("[data-pause-state]")?.textContent).toBe("00:25");
    // A new event starts from the engine's own figure again.
    act(() => t.crawls.progress({ paused_hosts: [host({ host: "https://media.example.com:443", seconds_left: 60 })] }));
    expect(row.querySelector("[data-pause-state]")?.textContent).toBe("01:00");
    act(() => vi.advanceTimersByTime(61_000));
    expect(row.querySelector("[data-pause-state]")?.textContent).toBe("Testing…");
  });

  test("waiting_for_site_shows_the_soonest_test", async () => {
    // US4.4: the only site is paused; the crawl must not look frozen.
    const t = await startCrawl();
    act(() =>
      t.crawls.progress({
        waiting_for_host: true,
        paused_hosts: [host({ host: "https://example.com:443", seconds_left: 90 }), host({ host: "https://cdn.example.com:443", seconds_left: 30 })],
      }),
    );
    const line = () => within(status()).getByRole("status").textContent;
    expect(line()).toBe("Waiting for site example.com · 00:30");
    expect(status().querySelector('[role="status"]')?.getAttribute("data-phase")).toBe("running");
    act(() => vi.advanceTimersByTime(10_000));
    expect(line()).toBe("Waiting for site example.com · 00:20");
    act(() => vi.advanceTimersByTime(20_000));
    expect(line()).toBe("Waiting for site example.com · Testing…");
    // Pause and Stop still work while waiting.
    expect(button(/Pause/).disabled).toBe(false);
    act(() => t.crawls.progress({ waiting_for_host: false, paused_hosts: [] }));
    expect(line()).toBe("Crawling example.com");
  });
});

test("notices_appear_in_the_overview", async () => {
  // US4.1: the pause is told in the crawl status view.
  const t = await startCrawl();
  act(() => t.crawls.notice({ host: "https://media.example.com:443", message: "https://media.example.com:443 is paused for 1 minute because 5 requests in a row failed; other sites keep being crawled." }));
  act(() => t.crawls.notice({ id: 99, message: "From another crawl" }));
  const notices = screen.getByRole("list", { name: "Crawl notices" });
  expect(notices.textContent).toContain("is paused for 1 minute");
  expect(screen.queryByText("From another crawl")).toBeNull();
  // The same message twice is shown once.
  act(() => t.crawls.notice({ message: "https://media.example.com:443 is paused for 1 minute because 5 requests in a row failed; other sites keep being crawled." }));
  expect(within(notices).getAllByRole("listitem")).toHaveLength(1);
});

const preview = { total: 40, by_class: [{ class: "timeout", label: "Timed out", count: 30 }, { class: "http_5xx", label: "Server error (5xx)", count: 10 }], available: true, reason: null };

test("retry_failed_urls_shows_counts_then_runs_like_a_resume", async () => {
  // US5.1, US5.2, FR-031
  const t = fakeTransport();
  t.crawls.preview = preview;
  await startCrawl(t);
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await waitFor(() => expect(button("Retry failed URLs").disabled).toBe(false));
  expect(button("Retry failed URLs").textContent).toContain("40");

  fireEvent.click(button("Retry failed URLs"));
  const dialog = await screen.findByRole("dialog", { name: "Retry failed URLs" });
  await within(dialog).findByText(/40 URLs failed for a reason that may have passed/);
  const rows = within(within(dialog).getByRole("table", { name: "Failed URLs by error" })).getAllByRole("row");
  expect(rows.map((r) => r.textContent)).toEqual(["Timed out30", "Server error (5xx)10"]);

  fireEvent.click(within(dialog).getByRole("button", { name: "Retry 40 URLs" }));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.retried).toEqual([PATH]);
  expect(within(status()).getByRole("status").textContent).toBe("Crawling example.com");
  // Running again: the action waits until the crawl settles.
  expect(button("Retry failed URLs").disabled).toBe(true);
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 9) }));
  expect(within(status()).getByRole("status").textContent).toBe("Completed example.com");
});

test("cancel_leaves_the_crawl_alone", async () => {
  const t = fakeTransport();
  t.crawls.preview = preview;
  await startCrawl(t);
  await act(async () => t.crawls.finish({ report: fakeReport("paused", 7) }));
  // A paused crawl can be retried too (FR-030).
  await waitFor(() => expect(button("Retry failed URLs").disabled).toBe(false));
  fireEvent.click(button("Retry failed URLs"));
  const dialog = await screen.findByRole("dialog", { name: "Retry failed URLs" });
  fireEvent.click(await within(dialog).findByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Retry failed URLs" })).toBeNull());
  expect(t.crawls.retried).toEqual([]);
});

test("retry_is_unavailable_with_a_reason", async () => {
  // FR-034, US5.4
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  // No crawl open: not in the toolbar; the palette explains.
  expect(screen.queryByRole("button", { name: "Retry failed URLs" })).toBeNull();
  let p = await palette("Retry failed");
  expect(within(p).getByText("Run or open a crawl first.")).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull());

  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  expect(button("Retry failed URLs").disabled).toBe(true);
  p = await palette("Retry failed");
  expect(within(p).getByText("Wait until the crawl is paused or finished.")).toBeTruthy();
  // Choosing a disabled entry does nothing.
  fireEvent.keyDown(within(p).getByPlaceholderText("Type a command or a tab name"), { key: "Enter" });
  expect(screen.queryByRole("dialog", { name: "Retry failed URLs" })).toBeNull();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull());

  // Finished with nothing temporary: the engine's reason.
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await waitFor(() => expect(button("Retry failed URLs").disabled).toBe(true));
  p = await palette("Retry failed");
  await within(p).findByText("No URL failed for a reason that could change.");
});

test("speed_and_retry_settings_apply_to_the_running_crawl", async () => {
  // Spec 004 D13: Speed and retries stay editable while crawling.
  const t = await startCrawl();
  await openSettings("Speed");
  const apply = () => button("Apply to running crawl");
  expect(apply().disabled).toBe(true);
  expect(screen.getByText("In use by the running crawl")).toBeTruthy();
  fireEvent.change(input("Max requests per site"), { target: { value: "2" } });
  await waitFor(() => expect(apply().disabled).toBe(false));
  expect(screen.getByText("Not applied to the running crawl yet")).toBeTruthy();
  fireEvent.click(apply());
  await waitFor(() => expect(t.crawls.applied).toHaveLength(1));
  // Only what changed is sent.
  expect(t.crawls.applied[0].id).toBe(1);
  expect(t.crawls.applied[0].speed?.max_per_host).toBe(2);
  expect(t.crawls.applied[0].retry).toBeNull();
  await waitFor(() => expect(apply().disabled).toBe(true));

  await openSettings("Network and retries");
  // Timeouts are not changed mid-crawl; retries are.
  expect(locked("Connect timeout (seconds)")).toBe(true);
  expect(locked("Attempts per URL")).toBe(false);
  fireEvent.change(input("First pause (seconds)"), { target: { value: "120" } });
  await waitFor(() => expect(apply().disabled).toBe(false));
  fireEvent.click(apply());
  await waitFor(() => expect(t.crawls.applied).toHaveLength(2));
  expect(t.crawls.applied[1].speed).toBeNull();
  expect(t.crawls.applied[1].retry?.host_pause_secs).toBe(120);

  // Every other category stays locked.
  await openSettings("Limits");
  expect(locked("Max URLs")).toBe(true);
  expect(screen.getByText(/Locked while the crawl runs/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Apply to running crawl" })).toBeNull();
});

test("invalid_retry_setting_blocks_apply", async () => {
  await startCrawl();
  await openSettings("Network and retries");
  fireEvent.change(input("First wait (seconds)"), { target: { value: "100" } });
  expect((await screen.findByRole("alert")).textContent).toContain("between 0.1 and 60 seconds");
  expect(button("Apply to running crawl").disabled).toBe(true);
});

test("paused_crawl_locks_every_setting", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Pause/));
  await act(async () => t.crawls.finish({ report: fakeReport("paused", 5) }));
  await openSettings("Speed");
  expect(locked("Max requests per site")).toBe(true);
  expect(screen.getByText(/Locked while a crawl is open/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Apply to running crawl" })).toBeNull();
});

test("new_retry_settings_reach_the_crawl", async () => {
  // FR-024 to FR-026 thresholds, set before Start.
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  await openSettings("Network and retries");
  fireEvent.change(input("First wait (seconds)"), { target: { value: "0.5" } });
  fireEvent.change(input("Pause a site after failures in a row"), { target: { value: "8" } });
  fireEvent.change(input("Longest pause (seconds)"), { target: { value: "1200" } });
  fireEvent.change(input("Give up after failed tests"), { target: { value: "3" } });
  fireEvent.click(button(/Done/));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.retry).toMatchObject({ base_wait_secs: 0.5, host_pause_after: 8, host_pause_secs: 60, host_pause_max_secs: 1200, host_give_up_after: 3 });
});

test("summary_shows_errors_by_class_and_retries", async () => {
  // FR-036
  const t = await startCrawl();
  const report: CrawlReport = fakeReport("paused", 400);
  Object.assign(report.summary, {
    by_class: { http_4xx: 300, timeout: 30, http_5xx: 10, dns: 0 },
    transient_errors: 40,
    permanent_errors: 300,
    retries: 120,
    recovered: 12,
    hosts_paused: 2,
    manual_retries: 1,
    pause_reason: "network",
  });
  report.failures = [{ class: "timeout", label: "Timed out", count: 30 }];
  await act(async () => t.crawls.finish({ report }));

  const overview = screen.getByRole("region", { name: "Overview" });
  expect(within(overview).getByText(/your network connection seemed to be down/)).toBeTruthy();
  expect(within(overview).getByText("Errors", { selector: "dt" }).nextSibling?.textContent).toBe("340");

  const errors = within(overview).getByRole("region", { name: "Errors" });
  expect(errors.querySelector("[data-aside]")?.textContent).toBe("40 temporary · 300 permanent");
  expect(within(errors).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual(["Client error (4xx)300", "Timed out30", "Server error (5xx)10"]);

  const retries = within(overview).getByRole("region", { name: "Retries" });
  const value = (label: string) => within(retries).getByText(label).nextSibling?.textContent;
  expect(value("Automatic retries")).toBe("120");
  expect(value("URLs that worked after a retry")).toBe("12");
  expect(value("Sites paused")).toBe("2");
  expect(value("Times you retried failed URLs")).toBe("1");
});

test("summary_without_retries_stays_quiet", async () => {
  const t = await startCrawl();
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  const overview = screen.getByRole("region", { name: "Overview" });
  expect(within(overview).queryByRole("region", { name: "Retries" })).toBeNull();
  expect(within(overview).queryByRole("region", { name: "Errors" })).toBeNull();
  expect(within(overview).queryByText(/paused this crawl/)).toBeNull();
});
