// Running a crawl from the window (spec 003 desktop commands): Start, live
// figures in the status bar, Pause, Resume, Stop, and the overview.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { formatElapsed } from "./useCrawlRun";
import type { RenderProgress } from "../transport";

const button = (name: RegExp) => screen.getByRole("button", { name }) as HTMLButtonElement;
const status = () => screen.getByRole("contentinfo", { name: "Crawl status" });
const figure = (label: string) => status().querySelector(`[data-figure="${label}"]`)?.textContent;

async function startCrawl() {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

test("start_runs_a_crawl_and_shows_live_figures", async () => {
  const t = await startCrawl();
  expect(t.crawls.started).toHaveLength(1);
  expect(within(status()).getByRole("status").textContent).toBe("Crawling example.com");
  // Mode and URL are locked while the crawl owns the window.
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).readOnly).toBe(true);

  act(() => t.crawls.progress({ discovered: 40, crawled: 12, queued: 25, not_crawled: 3, errors: 2 }));
  expect(figure("crawled")).toBe("12 crawled");
  expect(figure("queued")).toBe("25 queued");
  expect(figure("errors")).toBe("2 errors");
  expect(figure("left")).toBeUndefined();
  act(() => t.crawls.progress({ discovered: 40, crawled: 12, queued: 25, urls_per_sec: 100, eta_secs: 90 }));
  expect(figure("left")).toBe("~01:30 left");
  const overview = screen.getByRole("region", { name: "Overview" });
  expect(within(overview).getByText("Discovered").nextSibling?.textContent).toBe("40");
});

const renderProgress = (patch: Partial<RenderProgress> = {}): RenderProgress => ({
  rendered: 120,
  queued: 30,
  rendering: 4,
  pages_per_sec: 7.26,
  avg_pages_per_sec: 6,
  limit: 4,
  limiting: null,
  fetching_done: false,
  ...patch,
});

test("the_rendering_row_is_hidden_for_raw_progress_events", async () => {
  // Spec 014 US2 scenario 4: a raw crawl's events have no render figures.
  const t = await startCrawl();
  act(() => t.crawls.progress({ crawled: 12, queued: 25 }));
  expect(status().querySelector('[data-row="rendering"]')).toBeNull();
  act(() => t.crawls.progress({ crawled: 13, render: null }));
  expect(status().querySelector('[data-row="rendering"]')).toBeNull();
});

test("the_rendering_row_shows_the_figures_and_why_rendering_is_the_limit", async () => {
  const t = await startCrawl();
  act(() => t.crawls.progress({ crawled: 12, render: renderProgress() }));
  expect(status().querySelector('[data-row="rendering"]')).not.toBeNull();
  expect(figure("rendered")).toBe("120 rendered");
  expect(figure("render-waiting")).toBe("30 waiting to render");
  expect(figure("rendering")).toBe("4 rendering");
  expect(figure("render-rate")).toBe("7.3 pages/s");
  expect(status().textContent).not.toMatch(/Limited by/);
  expect(status().textContent).not.toMatch(/Fetching done/);

  act(() => t.crawls.progress({ crawled: 40, render: renderProgress({ limiting: "queue_full", fetching_done: false }) }));
  expect(status().textContent).toMatch(/Limited by: fetching is paused until fewer pages wait to be rendered/);

  act(() => t.crawls.progress({ crawled: 40, render: renderProgress({ limiting: "concurrency", fetching_done: true }) }));
  expect(status().textContent).toMatch(/Fetching done; rendering continues/);
  expect(status().textContent).toMatch(/Limited by: rendering 4 pages at once/);

  // The row goes away when a later event has no render figures.
  act(() => t.crawls.progress({ crawled: 41, render: undefined }));
  expect(status().querySelector('[data-row="rendering"]')).toBeNull();
});

test("analysis_phase_shows_in_the_status_bar_until_the_crawl_finishes", async () => {
  // Spec 009 plan D12: passes run at crawl end, before crawl-finished.
  const t = await startCrawl();
  act(() => t.crawls.analyse({ label: "Link Score", done: 40, total: 100 }));
  expect(within(status()).getByRole("status").textContent).toBe("Analysing: Link Score 40%");
  act(() => t.crawls.finish({ report: fakeReport("completed", 12) }));
  await waitFor(() => expect(within(status()).getByRole("status").textContent).toMatch(/^Completed/));
});

test("events_from_another_crawl_are_ignored", async () => {
  const t = await startCrawl();
  act(() => t.crawls.progress({ id: 99, crawled: 500 }));
  expect(figure("crawled")).toBe("0 crawled");
});

test("pause_then_resume_continues_the_same_file", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Pause/));
  expect(t.crawls.controls).toEqual(["pause 1"]);
  expect(button(/Pausing/).disabled).toBe(true);

  await act(async () => t.crawls.finish({ report: fakeReport("paused", 5) }));
  expect(within(status()).getByRole("status").textContent).toBe("Paused example.com");
  fireEvent.click(button(/Resume/));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.resumed).toEqual(["/crawls/example.com.spiderlens"]);
});

test("stop_on_a_paused_crawl_reopens_and_stops_it", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Pause/));
  await act(async () => t.crawls.finish({ report: fakeReport("paused", 5) }));
  fireEvent.click(button(/Stop/));
  await waitFor(() => expect(t.crawls.controls).toEqual(["pause 1", "stop 2"]));
  await act(async () => t.crawls.finish({ report: fakeReport("stopped", 5) }));
  expect(within(status()).getByRole("status").textContent).toBe("Stopped example.com");
  // A finished crawl gives the window back.
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).readOnly).toBe(false);
});

test("finished_crawl_explains_what_was_not_crawled", async () => {
  const t = await startCrawl();
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  const overview = screen.getByRole("region", { name: "Overview" });
  expect(within(overview).getByText("Completed")).toBeTruthy();
  expect(within(overview).getByText("Blocked by robots.txt").nextSibling?.textContent).toBe("1");
  expect(within(overview).getByText("/crawls/example.com.spiderlens")).toBeTruthy();
  expect(button(/Start$/).disabled).toBe(false);
});

test("failed_crawl_shows_the_engine_message", async () => {
  const t = await startCrawl();
  await act(async () => t.crawls.finish({ error: "Could not create the crawl file" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Could not create the crawl file");
  expect(within(status()).getByRole("status").textContent).toBe("Failed example.com");
});

test("warnings_arrive_while_crawling", async () => {
  const t = await startCrawl();
  act(() => t.crawls.warn("The start URL redirects to another site."));
  expect(screen.getByText("The start URL redirects to another site.")).toBeTruthy();
});

test("elapsed_time_format", () => {
  expect(formatElapsed(0)).toBe("00:00");
  expect(formatElapsed(65_000)).toBe("01:05");
  expect(formatElapsed(3_723_000)).toBe("1:02:03");
});

test("enter_right_after_typing_crawls_what_is_in_the_field", async () => {
  // Found on a Mac: typing "r", then the rest, then Enter before the debounced
  // check of "ryware.dev" landed crawled https://r/ and put it in the field.
  const t = fakeTransport();
  render(<App transport={t} />);
  const field = (await screen.findByLabelText("Start URL")) as HTMLInputElement;
  fireEvent.change(field, { target: { value: "r" } });
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  fireEvent.change(field, { target: { value: "ryware.dev" } });
  fireEvent.keyDown(field, { key: "Enter" });
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.started.map((c) => c.startUrl)).toEqual(["https://ryware.dev/"]);
  expect(field.value).toBe("https://ryware.dev/");
});

test("enter_on_a_host_without_a_dot_shows_an_error_and_does_not_crawl", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  const field = (await screen.findByLabelText("Start URL")) as HTMLInputElement;
  fireEvent.change(field, { target: { value: "ryware" } });
  fireEvent.keyDown(field, { key: "Enter" });
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  expect(t.crawls.started).toEqual([]);
  expect(field.value).toBe("ryware");
  expect(button(/Start$/).disabled).toBe(true);
});
