// The window around the crawl (spec 017): command palette, sidebar, splitters,
// themes, stored layout, About, the start state and the toolbar.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import type { Transport } from "../transport";

// jsdom has no pointer capture; the splitters call it on pointer down.
Element.prototype.setPointerCapture ??= () => {};


const renderApp = async (transport: Transport = fakeTransport()) => {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  return transport;
};

const openPalette = async (query = "") => {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  if (query) fireEvent.change(input, { target: { value: query } });
  return input;
};

const palette = async (query: string) => {
  const input = await openPalette(query);
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(screen.queryByPlaceholderText("Type a command or a tab name")).toBeNull());
};

/** Titles of the commands the palette lists for a query (shortcut hints dropped). */
const listed = async (query: string) => {
  await openPalette(query);
  return screen.queryAllByRole("option").map((o) => o.querySelector("span")?.textContent);
};

const closePalette = async () => {
  fireEvent.keyDown(screen.getByPlaceholderText("Type a command or a tab name"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByPlaceholderText("Type a command or a tab name")).toBeNull());
};

const selectedCategory = () => screen.getAllByRole("tab").find((t) => t.getAttribute("aria-selected") === "true")?.textContent;
const sheetOpen = () => screen.queryByRole("navigation", { name: "Settings categories" });
const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const stored = () => JSON.parse(localStorage.getItem("spiderlens.ui") ?? "{}");

async function startCrawl(t = fakeTransport()) {
  await renderApp(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

afterEach(() => {
  vi.restoreAllMocks();
});

// --- Command palette -------------------------------------------------------

test("palette_hides_crawl_controls_that_do_not_apply", async () => {
  await renderApp();
  // Nothing to crawl yet: Start is not offered either.
  expect(await listed("crawl")).not.toContain("Start crawl");
  await closePalette();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  const idle = await listed("crawl");
  expect(idle).toContain("Start crawl");
  expect(idle).toContain("Open crawl…");
  expect(idle).not.toContain("Pause crawl");
  expect(idle).not.toContain("Resume crawl");
  expect(idle).not.toContain("Stop crawl");
});

test("palette_lists_result_tabs_only_once_a_crawl_exists", async () => {
  const t = fakeTransport();
  await renderApp(t);
  expect(await listed("Internal")).not.toContain("Internal");
  await closePalette();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  const running = await listed("Internal");
  expect(running).toContain("Internal");
  await closePalette();
  // While it runs, Start, Open and the mode switches are gone; Pause and Stop are offered.
  const controls = await listed("crawl");
  expect(controls).toEqual(expect.arrayContaining(["Pause crawl", "Stop crawl"]));
  expect(controls).not.toContain("Start crawl");
  expect(controls).not.toContain("Open crawl…");
  expect(controls).not.toContain("Resume crawl");
});

test("palette_says_when_nothing_matches", async () => {
  await renderApp();
  await openPalette("zzqxv");
  expect(screen.queryAllByRole("option")).toHaveLength(0);
  expect(screen.getByText("No matching command")).toBeTruthy();
});

test("palette_finds_commands_by_keyword", async () => {
  await renderApp();
  expect(await listed("version")).toContain("About SpiderLens");
  await closePalette();
  expect(await listed("preferences")).toContain("Crawl settings");
  await closePalette();
  // "cancel" finds Stop, but only while there is a crawl to stop.
  expect(await listed("cancel")).not.toContain("Stop crawl");
});

test("palette_keyword_cancel_finds_stop_while_running", async () => {
  const t = await startCrawl();
  await openPalette("cancel");
  const opts = screen.getAllByRole("option");
  expect(opts[0].textContent).toContain("Stop crawl");
  fireEvent.keyDown(screen.getByPlaceholderText("Type a command or a tab name"), { key: "Enter" });
  await waitFor(() => expect(t.crawls.controls).toEqual(["stop 1"]));
});

test("palette_escape_closes_and_ctrl_k_toggles", async () => {
  await renderApp();
  await openPalette();
  await closePalette();
  await openPalette();
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  await waitFor(() => expect(screen.queryByPlaceholderText("Type a command or a tab name")).toBeNull());
});

test("toolbar_button_opens_the_palette", async () => {
  await renderApp();
  fireEvent.click(button("Open command palette"));
  expect(await screen.findByPlaceholderText("Type a command or a tab name")).toBeTruthy();
});

test.each([
  ["Settings: Limits", "Limits"],
  ["Settings: Scope", "Scope"],
  ["Settings: Identity", "Identity"],
  ["Settings: Speed", "Speed"],
  ["Settings: robots.txt", "robots.txt"],
  ["Settings: Network and retries", "Network and retries"],
])("palette_%s_opens_the_sheet_on_that_category", async (command, category) => {
  await renderApp();
  await palette(command);
  await waitFor(() => expect(sheetOpen()).toBeTruthy());
  expect(selectedCategory()).toBe(category);
  expect(screen.getByRole("tabpanel", { name: category })).toBeTruthy();
});

test("palette_switches_between_list_and_spider_mode", async () => {
  await renderApp();
  await palette("Switch to List mode");
  expect(await screen.findByRole("heading", { name: "Crawl a list of URLs" })).toBeTruthy();
  expect(screen.queryByLabelText("Start URL")).toBeNull();
  expect(screen.getByText("Add your URLs, then press Start. Only the URLs in the list are crawled.")).toBeTruthy();
  await palette("Switch to Spider mode");
  expect(await screen.findByRole("heading", { name: "Crawl a website" })).toBeTruthy();
  expect(screen.getByLabelText("Start URL")).toBeTruthy();
});

test("palette_edit_start_url_focuses_the_field", async () => {
  await renderApp();
  await palette("Edit start URL");
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Start URL")));
});

test("recent_commands_keep_five_newest_without_duplicates", async () => {
  await renderApp();
  for (const c of ["Brightness: Light", "Brightness: Dark", "Appearance: Follow system", "Speed: Polite", "Hide details", "Brightness: Light"]) {
    await palette(c);
  }
  expect(stored().recentCommands).toEqual(["level-light", "toggle-detail", "speed-polite", "theme-system", "level-dark"]);
  await openPalette();
  const recent = screen.getByText("Recent").parentElement!;
  const items = [...recent.querySelectorAll("[cmdk-item]")].map((i) => i.querySelector("span")?.textContent);
  // "Hide details" now reads "Show details": the recent list shows current titles.
  expect(items).toEqual(["Brightness: Light", "Show details", "Speed: Polite", "Appearance: Follow system", "Brightness: Dark"]);
});

// --- Sidebar ----------------------------------------------------------------

test("overview_is_selected_by_default", async () => {
  await renderApp();
  const nav = screen.getByRole("navigation", { name: "Sections" });
  expect(within(nav).getByText("Overview").closest("button")!.getAttribute("aria-current")).toBe("page");
  expect(within(nav).getByText("Internal").closest("button")!.getAttribute("aria-current")).toBeNull();
});

test("disabled_result_tabs_do_not_navigate", async () => {
  await renderApp();
  const nav = screen.getByRole("navigation", { name: "Sections" });
  const internal = within(nav).getByText("Internal").closest("button")!;
  expect(internal.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(internal);
  expect(internal.getAttribute("aria-current")).toBeNull();
  expect(within(nav).getByText("Overview").closest("button")!.getAttribute("aria-current")).toBe("page");
  expect(screen.getByRole("heading", { name: "Crawl a website" })).toBeTruthy();
});

test("links_tab_follows_internal_and_waits_for_a_crawl", async () => {
  await renderApp();
  const nav = screen.getByRole("navigation", { name: "Sections" });
  const titles = within(nav).getAllByRole("button").map((b) => b.textContent);
  expect(titles.indexOf("Links")).toBe(titles.indexOf("Internal") + 1);
  expect(within(nav).getByText("Links").closest("button")!.getAttribute("aria-disabled")).toBe("true");
});

test("ai_visibility_items_are_marked_for_m4_and_disabled", async () => {
  await renderApp();
  const nav = screen.getByRole("navigation", { name: "Sections" });
  for (const title of ["AI crawlers", "llms.txt"]) {
    const item = within(nav).getByText(title).closest("button")!;
    expect(item.getAttribute("aria-disabled")).toBe("true");
    expect(item.textContent).toContain("M4");
    fireEvent.click(item);
    expect(item.getAttribute("aria-current")).toBeNull();
  }
});

test("sidebar_about_item_opens_about", async () => {
  await renderApp();
  fireEvent.click(within(screen.getByRole("navigation", { name: "Sections" })).getByText("About SpiderLens"));
  expect((await screen.findByTestId("build-id")).textContent).toBe("abc1234.7");
});

test("result_tabs_navigate_once_a_crawl_exists", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  const nav = screen.getByRole("navigation", { name: "Sections" });
  const internal = within(nav).getByText("Internal").closest("button")!;
  expect(internal.getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(internal);
  await waitFor(() => expect(internal.getAttribute("aria-current")).toBe("page"));
  expect(screen.queryByRole("region", { name: "Overview" })).toBeNull();
});

// --- Splitters --------------------------------------------------------------

test("sidebar_splitter_steps_with_keys_and_clamps", async () => {
  await renderApp();
  const sep = screen.getByRole("separator", { name: "Resize sidebar" });
  const now = () => sep.getAttribute("aria-valuenow");
  expect(now()).toBe("220");
  expect(sep.getAttribute("aria-valuemin")).toBe("180");
  expect(sep.getAttribute("aria-valuemax")).toBe("320");
  fireEvent.keyDown(sep, { key: "ArrowRight", shiftKey: true });
  expect(now()).toBe("260");
  fireEvent.keyDown(sep, { key: "ArrowRight", shiftKey: true });
  fireEvent.keyDown(sep, { key: "ArrowRight", shiftKey: true });
  expect(now()).toBe("320");
  fireEvent.keyDown(sep, { key: "ArrowLeft" });
  expect(now()).toBe("310");
  for (let i = 0; i < 5; i++) fireEvent.keyDown(sep, { key: "ArrowLeft", shiftKey: true });
  expect(now()).toBe("180");
  expect(stored().sidebarWidth).toBe(180);
});

test("details_splitter_is_inverted_and_clamps", async () => {
  await renderApp();
  const sep = screen.getByRole("separator", { name: "Resize details" });
  const now = () => sep.getAttribute("aria-valuenow");
  expect(now()).toBe("300");
  // The details panel is on the right: moving its edge left makes it wider.
  fireEvent.keyDown(sep, { key: "ArrowLeft" });
  expect(now()).toBe("310");
  fireEvent.keyDown(sep, { key: "ArrowRight", shiftKey: true });
  expect(now()).toBe("270");
  fireEvent.keyDown(sep, { key: "ArrowRight", shiftKey: true });
  expect(now()).toBe("240");
  for (let i = 0; i < 6; i++) fireEvent.keyDown(sep, { key: "ArrowLeft", shiftKey: true });
  expect(now()).toBe("440");
  expect(stored().detailWidth).toBe(440);
});

test("splitters_follow_a_pointer_drag", async () => {
  await renderApp();
  const side = screen.getByRole("separator", { name: "Resize sidebar" });
  fireEvent.pointerDown(side, { clientX: 100, pointerId: 1 });
  fireEvent.pointerMove(side, { clientX: 150, pointerId: 1 });
  expect(side.getAttribute("aria-valuenow")).toBe("270");
  fireEvent.pointerMove(side, { clientX: 400, pointerId: 1 });
  expect(side.getAttribute("aria-valuenow")).toBe("320");
  fireEvent.pointerUp(side, { clientX: 400, pointerId: 1 });
  // Moving without a button held does nothing.
  fireEvent.pointerMove(side, { clientX: 0, pointerId: 1 });
  expect(side.getAttribute("aria-valuenow")).toBe("320");

  const details = screen.getByRole("separator", { name: "Resize details" });
  fireEvent.pointerDown(details, { clientX: 1000, pointerId: 2 });
  fireEvent.pointerMove(details, { clientX: 950, pointerId: 2 });
  expect(details.getAttribute("aria-valuenow")).toBe("350");
  fireEvent.pointerMove(details, { clientX: 1200, pointerId: 2 });
  expect(details.getAttribute("aria-valuenow")).toBe("240");
  fireEvent.pointerUp(details, { pointerId: 2 });
});

// --- Themes -----------------------------------------------------------------

function stubMatchMedia(dark: boolean) {
  const listeners = new Set<() => void>();
  const mq = {
    matches: dark,
    media: "(prefers-color-scheme: dark)",
    addEventListener: (_: string, cb: () => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: () => void) => listeners.delete(cb),
  };
  Object.defineProperty(window, "matchMedia", { configurable: true, writable: true, value: () => mq });
  return {
    set(d: boolean) {
      mq.matches = d;
      listeners.forEach((cb) => cb());
    },
    restore() {
      delete (window as { matchMedia?: unknown }).matchMedia;
    },
  };
}

test("appearance_commands_switch_theme", async () => {
  await renderApp();
  // jsdom has no matchMedia: the system reads as light, so Dark stops following it.
  await palette("Brightness: Dark");
  await waitFor(() => expect(document.documentElement.dataset.level).toBe("dark"));
  expect(document.documentElement.dataset.tone).toBe("dark");
  await palette("Brightness: Light");
  await waitFor(() => expect(document.documentElement.dataset.level).toBe("light"));
  expect(stored()).toMatchObject({ followSystem: false, level: "light" });
  await palette("Appearance: Follow system");
  await waitFor(() => expect(stored().followSystem).toBe(true));
  expect(document.documentElement.dataset.level).toBe("light");
});

test("system_theme_follows_the_os_live", async () => {
  const mm = stubMatchMedia(true);
  try {
    await renderApp();
    expect(document.documentElement.dataset.level).toBe("dark");
    act(() => mm.set(false));
    await waitFor(() => expect(document.documentElement.dataset.level).toBe("light"));
    act(() => mm.set(true));
    await waitFor(() => expect(document.documentElement.dataset.level).toBe("dark"));
    // A step of the other tone is a fixed choice and ignores the system.
    await palette("Brightness: Light");
    await waitFor(() => expect(document.documentElement.dataset.level).toBe("light"));
    act(() => mm.set(true));
    expect(document.documentElement.dataset.level).toBe("light");
  } finally {
    mm.restore();
  }
});

// --- Stored layout ----------------------------------------------------------

test("corrupt_stored_state_falls_back_to_defaults", async () => {
  localStorage.setItem("spiderlens.ui", "{not json");
  await renderApp();
  expect(screen.getByRole("separator", { name: "Resize sidebar" }).getAttribute("aria-valuenow")).toBe("220");
  expect(screen.getByRole("complementary", { name: "Details" })).toBeTruthy();
});

test("stored_state_from_another_version_is_ignored", async () => {
  localStorage.setItem("spiderlens.ui", JSON.stringify({ v: 3, sidebarWidth: 300, detailCollapsed: true, level: "dark" }));
  await renderApp();
  expect(screen.getByRole("separator", { name: "Resize sidebar" }).getAttribute("aria-valuenow")).toBe("220");
  expect(screen.getByRole("complementary", { name: "Details" })).toBeTruthy();
  expect(document.documentElement.dataset.level).toBe("light");
});

test("storage_that_throws_does_not_break_the_window", async () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("QuotaExceededError");
  });
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("SecurityError");
  });
  await renderApp();
  fireEvent.keyDown(window, { key: "i", code: "KeyI", ctrlKey: true, altKey: true });
  await waitFor(() => expect(screen.queryByRole("complementary", { name: "Details" })).toBeNull());
  await palette("Brightness: Dark");
  await waitFor(() => expect(document.documentElement.dataset.level).toBe("dark"));
});

// --- About ------------------------------------------------------------------

test("about_copies_build_identity_and_closes", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  try {
    await renderApp();
    await palette("About");
    await screen.findByTestId("build-id");
    fireEvent.click(button(/Copy/));
    expect(writeText).toHaveBeenCalledWith("SpiderLens 0.1.0 (engine 0.1.0, build abc1234.7)");
    expect(await screen.findByRole("button", { name: /Copied/ })).toBeTruthy();
    fireEvent.click(button("Close"));
    await waitFor(() => expect(screen.queryByTestId("build-id")).toBeNull());
  } finally {
    delete (navigator as { clipboard?: unknown }).clipboard;
  }
});

test("about_copy_failure_keeps_copy_label", async () => {
  const writeText = vi.fn().mockRejectedValue(new Error("denied"));
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  try {
    await renderApp();
    await palette("About");
    await screen.findByTestId("build-id");
    fireEvent.click(button(/Copy/));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    await act(async () => {});
    expect(screen.queryByRole("button", { name: /Copied/ })).toBeNull();
    expect(button(/Copy/)).toBeTruthy();
  } finally {
    delete (navigator as { clipboard?: unknown }).clipboard;
  }
});

// --- Start state and toolbar ------------------------------------------------

test("start_state_review_opens_settings", async () => {
  await renderApp();
  expect(screen.queryByLabelText("Less respectful settings")).toBeNull();
  await palette("Speed: Fast");
  const box = await screen.findByLabelText("Less respectful settings");
  expect(box.textContent).toContain("Faster settings are intended for sites you control.");
  fireEvent.click(within(box).getByRole("button", { name: "Review" }));
  await waitFor(() => expect(sheetOpen()).toBeTruthy());
});

test("start_state_start_url_hint_focuses_the_field", async () => {
  await renderApp();
  const hint = within(screen.getByRole("region", { name: "Start" })).getByRole("button", { name: /^Start URL/ });
  hint.focus();
  expect(document.activeElement).toBe(hint);
  fireEvent.click(hint);
  expect(document.activeElement).toBe(screen.getByLabelText("Start URL"));
});

test("list_mode_toolbar_button_opens_the_url_list", async () => {
  await renderApp();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  await waitFor(() => expect(sheetOpen()).toBeTruthy());
  expect(selectedCategory()).toBe("URL list");
  expect(screen.getByRole("textbox", { name: "URL list" })).toBeTruthy();
});

test("warning_button_counts_and_opens_settings", async () => {
  await renderApp();
  expect(screen.queryByRole("button", { name: /less respectful settings/ })).toBeNull();
  await palette("Speed: Fast");
  fireEvent.click(await screen.findByRole("button", { name: "1 less respectful settings" }));
  await waitFor(() => expect(sheetOpen()).toBeTruthy());
});

test("disabled_start_explains_why_on_hover", async () => {
  await renderApp();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "ryware" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(true));
  await waitFor(() => expect(screen.getByLabelText("Start URL").getAttribute("aria-invalid")).toBe("true"));
  const trigger = button(/Start$/).parentElement!;
  fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
  fireEvent.mouseEnter(trigger);
  fireEvent.mouseMove(trigger);
  // The same message shows in the start state and in the tooltip.
  await waitFor(() => expect(screen.getAllByText('"ryware" is not a full web address.').length).toBeGreaterThanOrEqual(2), { timeout: 2000 });
});

test("stop_while_running_shows_stopping", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Stop/));
  expect(t.crawls.controls).toEqual(["stop 1"]);
  expect(button(/Stopping…/).disabled).toBe(true);
  expect(button(/Pause/).disabled).toBe(true);
  expect(within(screen.getByRole("contentinfo", { name: "Crawl status" })).getByRole("status").textContent).toBe("Stopping… example.com");
});

test("pausing_disables_both_controls", async () => {
  const t = await startCrawl();
  fireEvent.click(button(/Pause/));
  expect(button(/Pausing…/).disabled).toBe(true);
  expect(button(/Stop/).disabled).toBe(true);
  // A second pause from the palette is not offered.
  expect(await listed("Pause")).not.toContain("Pause crawl");
  expect(t.crawls.controls).toEqual(["pause 1"]);
});
