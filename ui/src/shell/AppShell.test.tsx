import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import type { Need } from "../generated/Need";
import type { Transport } from "../transport";

const renderApp = async (transport: Transport = fakeTransport()) => {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
};

const palette = async (query: string) => {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  fireEvent.keyDown(input, { key: "Enter" });
};

test("window_has_five_regions_and_focuses_start_url", async () => {
  // US1-1, US1-2
  await renderApp();
  expect(screen.getByRole("banner")).toBeTruthy();
  expect(screen.getByRole("navigation", { name: "Sections" })).toBeTruthy();
  expect(screen.getByRole("main")).toBeTruthy();
  expect(screen.getByRole("complementary", { name: "Details" })).toBeTruthy();
  expect(screen.getByRole("contentinfo", { name: "Crawl status" })).toBeTruthy();
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Start URL")));
  expect(screen.getByRole("heading", { name: "Crawl a website" })).toBeTruthy();
});

test("narrow_window_collapses_details_then_sidebar", async () => {
  // US1-4, FR-005
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1000 });
  await renderApp();
  expect(screen.queryByRole("complementary", { name: "Details" })).toBeNull();
  expect(screen.getByText("Overview")).toBeTruthy();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 800 });
  fireEvent(window, new Event("resize"));
  await waitFor(() => expect(screen.queryByText("Overview")).toBeNull());
  expect(screen.getByRole("button", { name: "Overview" })).toBeTruthy();
});

test("pane_state_survives_restart", async () => {
  // US1-3
  const { unmount } = render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  fireEvent.keyDown(screen.getByRole("separator", { name: "Resize sidebar" }), { key: "ArrowRight" });
  fireEvent.keyDown(window, { key: "i", code: "KeyI", ctrlKey: true, altKey: true });
  await waitFor(() => expect(screen.queryByRole("complementary", { name: "Details" })).toBeNull());
  unmount();
  await renderApp();
  expect(screen.getByRole("separator", { name: "Resize sidebar" }).getAttribute("aria-valuenow")).toBe("230");
  expect(screen.queryByRole("complementary", { name: "Details" })).toBeNull();
});

test("about_shows_build_identity", async () => {
  // US1-5 (replaces the spec 001 identity test)
  await renderApp();
  await palette("About");
  expect((await screen.findByTestId("build-id")).textContent).toContain("abc1234.7");
  expect(screen.getByTestId("product-version").textContent).toContain("0.1.0");
  expect(screen.getByTestId("engine-version").textContent).toContain("0.1.0");
});

test("about_shows_error_when_engine_unreachable", async () => {
  const broken: Transport = { ...fakeTransport(), buildInfo: () => Promise.reject(new Error("no ipc")) };
  await renderApp(broken);
  await palette("About");
  expect((await screen.findByText(/Could not read build information/)).textContent).toContain("no ipc");
});

test("theme_choice_applies_and_persists", async () => {
  // spec 017 US3-1, US3-2; spec 018 FR-014, FR-015
  const { unmount } = render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  // Follows the system by default; jsdom reports a light system.
  expect(document.documentElement.dataset.level).toBe("light");
  expect(document.documentElement.dataset.accent).toBe("teal");
  await palette("Brightness: Dark");
  await waitFor(() => expect(document.documentElement.dataset.level).toBe("dark"));
  expect(document.documentElement.dataset.tone).toBe("dark");
  await palette("Accent: Cobalt");
  await waitFor(() => expect(document.documentElement.dataset.accent).toBe("cobalt"));
  unmount();
  await renderApp();
  expect(document.documentElement.dataset.level).toBe("dark");
  expect(document.documentElement.dataset.accent).toBe("cobalt");
  await palette("Appearance: Follow system");
  await waitFor(() => expect(document.documentElement.dataset.level).toBe("light"));
});

test("keyboard_only_configure_and_start", async () => {
  // US4 task list as far as it exists today: settings, speed, start URL, start.
  const t = fakeTransport();
  await renderApp(t);
  fireEvent.change(document.activeElement!, { target: { value: "example.com" } });
  await palette("Speed: Fast");
  // Start enables once the engine has checked the new settings.
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.keyDown(window, { key: "Enter", code: "Enter", ctrlKey: true });
  await waitFor(() => expect(t.crawls.started.map((s) => s.startUrl)).toEqual(["https://example.com/"]));
  expect(t.crawls.started[0].config.speed.max_concurrent).toBe(64);
  expect(screen.getByRole("contentinfo", { name: "Crawl status" }).textContent).toContain("Fast");
});

test("recent_commands_come_first", async () => {
  await renderApp();
  await palette("Brightness: Soft");
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const recent = await screen.findByText("Recent");
  expect(recent.parentElement?.textContent).toContain("Brightness: Soft");
});

// Rendering an existing crawl (spec 014 FR-006, T073): the action is offered for a finished crawl whose
// raw HTML is stored and that was not rendered, and for no other.
async function finishedCrawl(features: Need[], requireBrowser = false) {
  const t = fakeTransport();
  t.results.features = features;
  t.render.requireBrowser = requireBrowser;
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  const start = await screen.findByRole("button", { name: /Start$/ });
  await waitFor(() => expect((start as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(start);
  await screen.findByRole("button", { name: /Pause/ });
  await act(async () => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await screen.findByRole("button", { name: /Retry failed/ });
  return t;
}

test("render_this_crawl_is_shown_for_a_finished_crawl_with_stored_html", async () => {
  const t = await finishedCrawl(["base", "raw_html"]);
  fireEvent.click(await screen.findByRole("button", { name: "Render this crawl" }));
  await waitFor(() => expect(t.crawls.rendered).toHaveLength(1));
  expect(t.crawls.rendered[0].path).toBe("/crawls/example.com.spiderlens");
  expect(t.crawls.rendered[0].rendering.mode).toBe("javascript");
  // It runs like a crawl: Pause is offered while it does.
  await screen.findByRole("button", { name: /Pause/ });
});

test("render_this_crawl_is_hidden_for_rendered_crawls_and_crawls_without_html", async () => {
  await finishedCrawl(["base", "raw_html", "rendering"]);
  expect(screen.queryByRole("button", { name: "Render this crawl" })).toBeNull();
});

test("render_this_crawl_is_hidden_without_stored_html", async () => {
  await finishedCrawl(["base"]);
  expect(screen.queryByRole("button", { name: "Render this crawl" })).toBeNull();
});

test("rendering_a_crawl_without_a_browser_asks_to_download_one_and_has_no_raw_alternative", async () => {
  const t = await finishedCrawl(["base", "raw_html"], true);
  fireEvent.click(await screen.findByRole("button", { name: "Render this crawl" }));
  await screen.findByText("JavaScript rendering needs a browser");
  expect(screen.getByRole("button", { name: "Download and render" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Crawl raw HTML instead" })).toBeNull();
  expect(t.crawls.rendered).toHaveLength(0);
});
