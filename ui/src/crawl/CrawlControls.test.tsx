// Crawl controls, status bar and overview edge cases (spec 003 desktop
// commands, spec 007 open crawl).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import type { CrawlReport } from "../transport";


const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const status = () => screen.getByRole("contentinfo", { name: "Crawl status" });
const phase = () => within(status()).getByRole("status").textContent;
const figure = (label: string) => status().querySelector(`[data-figure="${label}"]`)?.textContent;
const overview = () => screen.getByRole("region", { name: "Overview" });

async function ready(t = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

async function startCrawl(t = fakeTransport()) {
  await ready(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

afterEach(() => {
  vi.restoreAllMocks();
});

test("start_call_failing_shows_failed_and_the_message", async () => {
  const t = fakeTransport();
  t.crawlStart = async () => {
    throw new Error("disk full");
  };
  await ready(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  expect((await screen.findByRole("alert")).textContent).toContain("disk full");
  expect(phase()).toBe("Failed example.com");
  // The window is given back.
  expect(button(/Start$/).disabled).toBe(false);
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).readOnly).toBe(false);
});

test("crawl_finishing_before_its_start_call_returns_ends_completed", async () => {
  const t = fakeTransport();
  t.crawlStart = async (config, startUrl) => {
    t.crawls.started.push({ config, startUrl });
    t.crawls.finish({ id: 1, report: fakeReport("completed", 7) });
    return { id: 1, path: "/crawls/example.com.spiderlens" };
  };
  await ready(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await waitFor(() => expect(phase()).toBe("Completed example.com"));
  expect(within(overview()).getByText("Completed")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Pause/ })).toBeNull();
});

test("early_failure_before_start_returns_shows_failed", async () => {
  const t = fakeTransport();
  t.crawlStart = async (config, startUrl) => {
    t.crawls.started.push({ config, startUrl });
    t.crawls.finish({ id: 1, error: "Could not create the crawl file" });
    return { id: 1, path: "/crawls/example.com.spiderlens" };
  };
  await ready(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  expect((await screen.findByRole("alert")).textContent).toContain("Could not create the crawl file");
  expect(phase()).toBe("Failed example.com");
});

test("urls_per_second_needs_two_progress_events", async () => {
  const t = await startCrawl();
  expect(figure("URLs/s")).toBe("– URLs/s");
  const now = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
  act(() => t.crawls.progress({ crawled: 10 }));
  expect(figure("URLs/s")).toBe("– URLs/s");
  now.mockReturnValue(1_002_000);
  act(() => t.crawls.progress({ crawled: 30 }));
  expect(figure("URLs/s")).toBe("10 URLs/s");
  now.mockReturnValue(1_004_000);
  act(() => t.crawls.progress({ crawled: 40 }));
  // (40 - 10) / 4 s, under ten shows one decimal.
  expect(figure("URLs/s")).toBe("7.5 URLs/s");
  // A finished crawl has no current speed.
  act(() => t.crawls.finish({ report: fakeReport("completed", 40) }));
  expect(figure("URLs/s")).toBe("– URLs/s");
});

test("status_bar_says_one_error", async () => {
  const t = await startCrawl();
  expect(figure("errors")).toBe("0 errors");
  act(() => t.crawls.progress({ errors: 1 }));
  expect(figure("errors")).toBeUndefined();
  expect(figure("error")).toBe("1 error");
});

test("status_bar_shows_speed_and_robots_mode", async () => {
  await ready();
  await waitFor(() => expect(status().textContent).toContain("Polite · robots.txt respected"));
});

function richReport(): CrawlReport {
  const r = fakeReport("completed", 7);
  r.summary.url_limit_reached = true;
  r.summary.hosts = [
    { host: "example.com", robots: "ok", crawl_delay_secs: 2, error: null },
    { host: "cdn.example.com", robots: "unavailable", crawl_delay_secs: null, error: "timed out" },
  ];
  r.summary.sitemaps = [
    { url: "https://example.com/sitemap.xml", urls: 1234, error: null },
    { url: "https://example.com/broken.xml", urls: 0, error: "Not valid XML" },
  ];
  r.failures = [
    { class: "timeout", label: "Timed out", count: 2 },
    { class: "dns", label: "DNS lookup failed", count: 5 },
  ];
  return r;
}

test("overview_explains_failures_robots_and_sitemaps", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: richReport() }));
  const o = overview();
  expect(within(o).getByText(/The URL limit was reached/)).toBeTruthy();

  expect(within(o).getByRole("heading", { name: "Failed without a response" })).toBeTruthy();
  const failures = [...o.querySelectorAll("[data-failure]")].map((r) => r.textContent);
  // Largest first.
  expect(failures).toEqual(["DNS lookup failed5", "Timed out2"]);

  expect(within(o).getByRole("heading", { name: "robots.txt" })).toBeTruthy();
  const site = (host: string) => within(o).getAllByText(host).find((e) => e.tagName === "TD")!;
  expect(site("example.com").nextSibling?.textContent).toBe("Read, crawl-delay 2 s");
  expect(site("cdn.example.com").nextSibling?.textContent).toBe("Unavailable (timed out)");

  expect(within(o).getByRole("heading", { name: "Sitemaps" })).toBeTruthy();
  expect(within(o).getByText("https://example.com/sitemap.xml").nextSibling?.textContent).toBe((1234).toLocaleString());
  expect(within(o).getByText("https://example.com/broken.xml").nextSibling?.textContent).toBe("Not valid XML");
});

test("overview_hides_tables_without_data", async () => {
  const t = await startCrawl();
  const r = fakeReport("completed", 7);
  r.summary.hosts = [];
  act(() => t.crawls.finish({ report: r }));
  const o = overview();
  expect(within(o).queryByRole("heading", { name: "Failed without a response" })).toBeNull();
  expect(within(o).queryByRole("heading", { name: "robots.txt" })).toBeNull();
  expect(within(o).queryByRole("heading", { name: "Sitemaps" })).toBeNull();
  expect(within(o).queryByText(/The URL limit was reached/)).toBeNull();
});

test("list_mode_crawl_target_counts_urls", async () => {
  const t = await ready();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  fireEvent.change(await screen.findByRole("textbox", { name: "URL list" }), {
    target: { value: "https://a.test/1\nhttps://a.test/2\nhttps://a.test/3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Load pasted URLs" }));
  await screen.findByTestId("list-summary");
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await screen.findByRole("button", { name: "3 URLs loaded" });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.started[0].startUrl).toBeNull();
  expect(t.crawls.started[0].config.mode).toBe("list");
  expect(phase()).toBe("Crawling 3 URLs");
  expect(within(overview()).getByRole("heading", { name: "3 URLs" })).toBeTruthy();
  // The list button is locked while the crawl runs.
  expect(button("3 URLs loaded").disabled).toBe(true);
});

test("opening_a_crawl_left_running_shows_it_stopped", async () => {
  const t = fakeTransport();
  t.pickCrawlFile = async () => "/crawls/old.spiderlens";
  const summary = vi.fn(async (path: string) => {
    const r = fakeReport("running", 4, path);
    r.summary.active_ms = 65_000;
    return r;
  });
  t.crawlSummary = summary;
  await ready(t);
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  await waitFor(() => expect(phase()).toBe("Stopped example.com"));
  expect(summary).toHaveBeenCalledWith("/crawls/old.spiderlens");
  expect(figure("crawled")).toBe("4 crawled");
  expect(figure("elapsed")).toBe("01:05 elapsed");
  // An opened crawl goes straight to its results.
  const nav = screen.getByRole("navigation", { name: "Sections" });
  expect(within(nav).getByText("Internal").closest("button")!.getAttribute("aria-current")).toBe("page");
  // A stopped crawl is not resumed from here: Resume is not offered.
  expect(screen.queryByRole("button", { name: /Resume/ })).toBeNull();
});

test("cancelling_the_open_dialog_changes_nothing", async () => {
  const t = fakeTransport();
  const summary = vi.fn(t.crawlSummary);
  t.crawlSummary = summary;
  await ready(t);
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  await new Promise((r) => setTimeout(r, 50));
  expect(summary).not.toHaveBeenCalled();
  expect(phase()).toBe("Idle");
});

test("opening_a_bad_file_after_a_crawl_says_why", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  t.pickCrawlFile = async () => "/crawls/bad.spiderlens";
  t.crawlSummary = async () => {
    throw new Error("not a crawl file");
  };
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  expect((await screen.findByRole("alert")).textContent).toBe("Could not open /crawls/bad.spiderlens: Error: not a crawl file");
  // The finished crawl stays open.
  expect(phase()).toBe("Completed example.com");
});

test("opening_a_bad_file_from_the_start_state_says_why", async () => {
  const t = fakeTransport();
  t.pickCrawlFile = async () => "/crawls/bad.spiderlens";
  t.crawlSummary = async () => {
    throw new Error("not a crawl file");
  };
  await ready(t);
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  expect((await screen.findByText(/Could not open \/crawls\/bad\.spiderlens/, undefined, { timeout: 1000 })).textContent).toContain("not a crawl file");
});
