// Keyboard shortcuts (spec 017 US4, FR-023/FR-024). jsdom is not a Mac, so
// "Mod" is Ctrl here.
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
import type { Transport } from "../transport";


const renderApp = async (transport: Transport = fakeTransport()) => {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  return transport;
};

const key = (k: string, code: string, mods: { alt?: boolean } = {}) =>
  fireEvent.keyDown(window, { key: k, code, ctrlKey: true, altKey: !!mods.alt });

const sheetOpen = () => screen.queryByRole("navigation", { name: "Settings categories" });
const details = () => screen.queryByRole("complementary", { name: "Details" });
const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const stored = () => JSON.parse(localStorage.getItem("spiderlens.ui") ?? "{}");

test("ctrl_l_focuses_the_start_url", async () => {
  await renderApp();
  const sep = screen.getByRole("separator", { name: "Resize sidebar" });
  sep.focus();
  expect(document.activeElement).toBe(sep);
  key("l", "KeyL");
  expect(document.activeElement).toBe(screen.getByLabelText("Start URL"));
});

test("ctrl_comma_opens_settings", async () => {
  await renderApp();
  key(",", "Comma");
  expect(await screen.findByRole("navigation", { name: "Settings categories" })).toBeTruthy();
});

test("ctrl_alt_s_collapses_sidebar_to_icons_and_back", async () => {
  await renderApp();
  const nav = () => screen.getByRole("navigation", { name: "Sections" });
  expect(nav().textContent).toContain("Overview");
  expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeTruthy();
  key("s", "KeyS", { alt: true });
  await waitFor(() => expect(nav().textContent).not.toContain("Overview"));
  expect(screen.getByRole("button", { name: "Overview" })).toBeTruthy();
  expect(screen.queryByRole("separator", { name: "Resize sidebar" })).toBeNull();
  expect(stored().sidebarCollapsed).toBe(true);
  // Option+S on a Mac types "ß"; the physical key still matches.
  key("ß", "KeyS", { alt: true });
  await waitFor(() => expect(nav().textContent).toContain("Overview"));
  expect(stored().sidebarCollapsed).toBe(false);
});

test("ctrl_alt_i_hides_and_shows_details", async () => {
  await renderApp();
  expect(details()).toBeTruthy();
  key("i", "KeyI", { alt: true });
  await waitFor(() => expect(details()).toBeNull());
  expect(screen.queryByRole("separator", { name: "Resize details" })).toBeNull();
  key("i", "KeyI", { alt: true });
  await waitFor(() => expect(details()).toBeTruthy());
});

test("ctrl_o_opens_a_crawl_only_when_none_is_running", async () => {
  const t = fakeTransport();
  const pick = vi.fn(async () => null);
  t.pickCrawlFile = pick;
  await renderApp(t);
  key("o", "KeyO");
  await waitFor(() => expect(pick).toHaveBeenCalledTimes(1));

  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  key("o", "KeyO");
  await new Promise((r) => setTimeout(r, 50));
  expect(pick).toHaveBeenCalledTimes(1);
});

test("shortcuts_are_ignored_while_the_palette_is_open", async () => {
  await renderApp();
  key("k", "KeyK");
  await screen.findByPlaceholderText("Type a command or a tab name");
  key("i", "KeyI", { alt: true });
  key("s", "KeyS", { alt: true });
  key(",", "Comma");
  await new Promise((r) => setTimeout(r, 50));
  expect(stored().detailCollapsed).toBe(false);
  expect(stored().sidebarCollapsed).toBe(false);
  expect(sheetOpen()).toBeNull();
});

test("shortcuts_are_ignored_while_settings_are_open", async () => {
  const t = await renderApp();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  key(",", "Comma");
  await screen.findByRole("navigation", { name: "Settings categories" });
  key("i", "KeyI", { alt: true });
  key("Enter", "Enter");
  await new Promise((r) => setTimeout(r, 50));
  expect(stored().detailCollapsed).toBe(false);
  expect((t as ReturnType<typeof fakeTransport>).crawls.started).toEqual([]);
  // Ctrl+K still toggles the palette from anywhere.
  key("k", "KeyK");
  expect(await screen.findByPlaceholderText("Type a command or a tab name")).toBeTruthy();
});

test("shortcuts_are_ignored_while_about_is_open", async () => {
  await renderApp();
  fireEvent.click(screen.getByText("About SpiderLens"));
  await screen.findByTestId("build-id");
  key("i", "KeyI", { alt: true });
  await new Promise((r) => setTimeout(r, 50));
  expect(stored().detailCollapsed).toBe(false);
});

test("shortcuts_need_exactly_their_modifiers", async () => {
  await renderApp();
  // Ctrl+Shift+Alt+I and plain Alt+I are not Ctrl+Alt+I.
  fireEvent.keyDown(window, { key: "i", code: "KeyI", ctrlKey: true, altKey: true, shiftKey: true });
  fireEvent.keyDown(window, { key: "i", code: "KeyI", altKey: true });
  fireEvent.keyDown(window, { key: "i", code: "KeyI", metaKey: true, altKey: true });
  await new Promise((r) => setTimeout(r, 50));
  expect(details()).toBeTruthy();
});

// Probe for the Enter race fixed in PR #13, through Ctrl+Enter instead of
// Enter in the field. The "start" command is enabled by setup.canStart, the
// result of the last (debounced) validation, so right after typing a valid
// URL over an invalid one the shortcut is disabled and nothing happens.
test.fails("ctrl_enter_right_after_typing_crawls_what_is_in_the_field", async () => {
  const t = fakeTransport();
  await renderApp(t);
  const field = screen.getByLabelText("Start URL") as HTMLInputElement;
  fireEvent.change(field, { target: { value: "r" } });
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  fireEvent.change(field, { target: { value: "ryware.dev" } });
  key("Enter", "Enter");
  await screen.findByRole("button", { name: /Pause/ }, { timeout: 1000 });
  expect(t.crawls.started.map((c) => c.startUrl)).toEqual(["https://ryware.dev/"]);
});

test("ctrl_enter_after_validation_lands_starts_the_crawl", async () => {
  // The same keys work once the check of the new value has come back.
  const t = fakeTransport();
  await renderApp(t);
  const field = screen.getByLabelText("Start URL") as HTMLInputElement;
  fireEvent.change(field, { target: { value: "r" } });
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  fireEvent.change(field, { target: { value: "ryware.dev" } });
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("false"));
  key("Enter", "Enter");
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.started.map((c) => c.startUrl)).toEqual(["https://ryware.dev/"]);
});
