import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
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
  // US3-1, US3-2
  const { unmount } = render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  expect(document.documentElement.dataset.theme).toBe("light");
  await palette("Appearance: Dark");
  await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  unmount();
  await renderApp();
  expect(document.documentElement.dataset.theme).toBe("dark");
});

test("keyboard_only_configure_and_start", async () => {
  // US4 task list as far as it exists today: settings, speed, start URL, start.
  await renderApp();
  fireEvent.change(document.activeElement!, { target: { value: "example.com" } });
  await palette("Speed: Fast");
  // Start enables once the engine has checked the new settings.
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.keyDown(window, { key: "Enter", code: "Enter", ctrlKey: true });
  await waitFor(() => expect(screen.getByText(/Ready to crawl https:\/\/example.com\//)).toBeTruthy());
  expect(screen.getByRole("contentinfo", { name: "Crawl status" }).textContent).toContain("Fast");
});

test("recent_commands_come_first", async () => {
  await renderApp();
  await palette("Appearance: Light");
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const recent = await screen.findByText("Recent");
  expect(recent.parentElement?.textContent).toContain("Appearance: Light");
});
