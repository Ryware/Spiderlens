// Running a crawl from the window (spec 003 desktop commands): Start, live
// figures in the status bar, Pause, Resume, Stop, and the overview.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { formatElapsed } from "./useCrawlRun";

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

  act(() => t.crawls.finish({ report: fakeReport("paused", 5) }));
  expect(within(status()).getByRole("status").textContent).toBe("Paused example.com");
  fireEvent.click(button(/Resume/));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.resumed).toEqual(["/crawls/example.com.spiderlens"]);
});

test("stop_on_a_paused_crawl_reopens_and_stops_it", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Pause/));
  act(() => t.crawls.finish({ report: fakeReport("paused", 5) }));
  fireEvent.click(button(/Stop/));
  await waitFor(() => expect(t.crawls.controls).toEqual(["pause 1", "stop 2"]));
  act(() => t.crawls.finish({ report: fakeReport("stopped", 5) }));
  expect(within(status()).getByRole("status").textContent).toBe("Stopped example.com");
  // A finished crawl gives the window back.
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).readOnly).toBe(false);
});

test("finished_crawl_explains_what_was_not_crawled", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  const overview = screen.getByRole("region", { name: "Overview" });
  expect(within(overview).getByText("Completed")).toBeTruthy();
  expect(within(overview).getByText("Blocked by robots.txt").nextSibling?.textContent).toBe("1");
  expect(within(overview).getByText("/crawls/example.com.spiderlens")).toBeTruthy();
  expect(button(/Start$/).disabled).toBe(false);
});

test("failed_crawl_shows_the_engine_message", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ error: "Could not create the crawl file" }));
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
