// The window in a real browser: layout, keys, pointer, scrolling and theme
// colors, which jsdom cannot show. Crawl and results events come from the
// in-memory transport (e2e/app.tsx) through window.spiderlens.
import { expect, test, type Page } from "@playwright/test";
import type { E2eHandle } from "./app";

declare global {
  interface Window {
    spiderlens: E2eHandle;
  }
}

// The Desktop Chrome device reports a Windows user agent, so the app listens
// for Ctrl shortcuts on every OS, the macOS runner included.

const errors: string[] = [];

/** Types into the open palette and runs the top match once the palette has
 * caught up with the typing; Enter before that runs nothing. Keys sent before
 * the input has focus are lost, which left the search empty on the macOS
 * runner, so the top match was the most recent command. */
async function runInPalette(page: Page, text: string, expected = text) {
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeFocused();
  await page.keyboard.type(text);
  await expect(page.locator("[cmdk-item][data-selected=true]")).toContainText(expected);
  await page.keyboard.press("Enter");
}

test.beforeEach(async ({ page }) => {
  errors.length = 0;
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("./");
  await expect(page.getByLabel("Start URL")).toBeFocused();
});

test.afterEach(() => {
  expect(errors).toEqual([]);
});

/** Starts a crawl of example.com from the keyboard and finishes it. */
async function crawl(page: Page) {
  await page.keyboard.type("example.com");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.crawls.progress({ discovered: 5000, crawled: 1200, queued: 3800, errors: 3 }));
  await expect(page.getByRole("contentinfo", { name: "Crawl status" })).toContainText("1,200 crawled");
  await page.evaluate(() => window.spiderlens.crawls.finish({ report: window.spiderlens.fakeReport("completed", 5000) }));
  await expect(page.getByRole("status").filter({ hasText: "Completed" })).toBeVisible();
}

async function openInternal(page: Page) {
  await crawl(page);
  await page.getByRole("button", { name: "Internal" }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("5,000 URLs");
  return page.getByRole("grid");
}

test("window_fills_the_viewport_without_page_scroll", async ({ page }) => {
  for (const region of [
    page.getByRole("banner"),
    page.getByRole("navigation", { name: "Sections" }),
    page.getByRole("main"),
    page.getByRole("complementary", { name: "Details" }),
    page.getByRole("contentinfo", { name: "Crawl status" }),
  ]) {
    await expect(region).toBeVisible();
  }
  const { scrollH, innerH, scrollW, innerW } = await page.evaluate(() => ({
    scrollH: document.documentElement.scrollHeight,
    innerH: window.innerHeight,
    scrollW: document.documentElement.scrollWidth,
    innerW: window.innerWidth,
  }));
  expect(scrollH).toBeLessThanOrEqual(innerH);
  expect(scrollW).toBeLessThanOrEqual(innerW);
  const status = await page.getByRole("contentinfo", { name: "Crawl status" }).boundingBox();
  expect(status!.y + status!.height).toBeCloseTo(innerH, 0);
});

test("crawl_from_the_keyboard_and_watch_it_finish", async ({ page }) => {
  await crawl(page);
  const overview = page.getByRole("region", { name: "Overview" });
  await expect(overview.getByText("/crawls/example.com.spiderlens")).toBeVisible();
  await expect(page.getByRole("button", { name: "Start" })).toBeEnabled();
});

test("command_palette_runs_commands_by_keyboard", async ({ page }) => {
  await page.keyboard.press("Control+K");
  const input = page.getByPlaceholder("Type a command or a tab name");
  await expect(input).toBeFocused();
  await runInPalette(page, "about", "About");
  await expect(page.getByTestId("build-id")).toHaveText("abc1234.7");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("build-id")).toBeHidden();
});

test("settings_sheet_fits_the_window_and_closes_with_escape", async ({ page }) => {
  await page.keyboard.press("Control+Comma");
  const nav = page.getByRole("navigation", { name: "Settings categories" });
  await expect(nav).toBeVisible();
  const dialog = page.getByRole("dialog");
  const box = (await dialog.boundingBox())!;
  const vp = page.viewportSize()!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
  expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
  await nav.getByRole("tab", { name: /^Scope/ }).click();
  await expect(page.getByRole("tabpanel", { name: "Scope" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(nav).toBeHidden();
});

test("narrow_window_collapses_details_then_sidebar", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await expect(page.getByRole("complementary", { name: "Details" })).toBeHidden();
  await expect(page.getByText("Overview", { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 860, height: 800 });
  await expect(page.getByRole("button", { name: "Overview" })).toBeVisible();
  await expect(page.getByText("Overview", { exact: true })).toBeHidden();
});

test("themes_change_the_real_colors", async ({ page }) => {
  const bg = () => page.evaluate(() => getComputedStyle(document.querySelector("main")!).backgroundColor);
  const palette = async (cmd: string) => {
    await page.keyboard.press("Control+K");
    await runInPalette(page, cmd);
    await expect(page.getByPlaceholder("Type a command or a tab name")).toBeHidden();
  };
  await palette("Brightness: Light");
  await expect(page.locator("html")).toHaveAttribute("data-level", "light");
  const light = await bg();
  await palette("Brightness: Dark");
  await expect(page.locator("html")).toHaveAttribute("data-level", "dark");
  const dark = await bg();
  expect(dark).not.toBe(light);
  await palette("Accent: Orchid");
  await expect(page.locator("html")).toHaveAttribute("data-accent", "orchid");

  await palette("Appearance: Follow system");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-tone", "dark");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-tone", "light");
  expect(await bg()).toBe(light);
});

test("results_scroll_to_the_last_of_5000_rows", async ({ page }) => {
  const grid = await openInternal(page);
  await expect(grid.getByRole("row").nth(1)).toContainText("https://example.com/");
  // Only the rows on screen exist in the page.
  expect(await grid.getByRole("row").count()).toBeLessThan(80);
  await grid.getByRole("row").nth(1).click();
  await page.keyboard.press("End");
  const last = grid.locator('[role="row"][aria-selected="true"]');
  await expect(last).toContainText("https://example.com/page-4999");
  const g = (await grid.boundingBox())!;
  const r = (await last.boundingBox())!;
  expect(r.y).toBeGreaterThanOrEqual(g.y);
  expect(r.y + r.height).toBeLessThanOrEqual(g.y + g.height + 1);
  await expect(page.getByRole("complementary", { name: "Details" }).getByRole("heading", { level: 2 })).toHaveText(
    "https://example.com/page-4999",
  );
});

test("links_tab_sorts_by_link_score", async ({ page }) => {
  await crawl(page);
  await page.getByRole("button", { name: "Links" }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("5,000 URLs");
  const grid = page.getByRole("grid");
  const header = grid.getByRole("columnheader", { name: /Link Score/ });
  await header.click();
  await header.click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(header).toHaveAttribute("aria-sort", "descending");
  await expect(grid.getByRole("row").nth(1)).toContainText("https://example.com/");
  await expect(grid.getByRole("row").nth(1)).toContainText("100");
});

test("wheel_scrolling_loads_rows_further_down", async ({ page }) => {
  const grid = await openInternal(page);
  await grid.hover();
  for (let i = 0; i < 10; i++) await page.mouse.wheel(0, 3000);
  // Rows below the first page come from the engine a page at a time.
  await expect(grid.getByRole("row").filter({ hasText: /page-1\d\d\d/ }).first()).toBeVisible();
});

test("column_resizes_with_the_mouse_and_is_remembered", async ({ page }) => {
  const grid = await openInternal(page);
  const header = grid.getByRole("columnheader", { name: /Title/ });
  const before = (await header.boundingBox())!.width;
  const handle = header.locator("div[aria-hidden]");
  const h = (await handle.boundingBox())!;
  await page.mouse.move(h.x + h.width / 2, h.y + h.height / 2);
  await page.mouse.down();
  await page.mouse.move(h.x + h.width / 2 + 120, h.y + h.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect.poll(async () => (await header.boundingBox())!.width).toBeGreaterThan(before + 100);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("spiderlens.columns.v1") ?? "{}"));
  expect(saved.internal.widths.title).toBeGreaterThan(before + 100);
});

test("shift_click_and_copy_from_the_keyboard", async ({ page }) => {
  const grid = await openInternal(page);
  await grid.getByRole("row").nth(1).click();
  await grid.getByRole("row").nth(5).click({ modifiers: ["Shift"] });
  await expect(page.getByTestId("row-count")).toHaveText("5,000 URLs · 5 selected");
  await page.keyboard.press("Control+C");
  await expect(page.getByText("Copied 5 rows.")).toBeVisible();
  const copied = await page.evaluate(() => window.spiderlens.results.copied[0].split("\n").length);
  expect(copied).toBe(6);
});

test("closing_the_palette_returns_focus_to_where_it_was", async ({ page }) => {
  await page.keyboard.press("Control+K");
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeHidden();
  await expect(page.getByLabel("Start URL")).toBeFocused();
});

test("closing_settings_returns_focus_to_where_it_was", async ({ page }) => {
  await page.keyboard.press("Control+Comma");
  await expect(page.getByRole("navigation", { name: "Settings categories" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Start URL")).toBeFocused();
});

test("a_command_that_opens_a_window_keeps_focus_there", async ({ page }) => {
  await page.keyboard.press("Control+K");
  await runInPalette(page, "Settings: Scope");
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeHidden();
  await expect(page.getByRole("tab", { name: /^Scope/ })).toBeFocused();
});

test("reopening_the_palette_starts_with_an_empty_search", async ({ page }) => {
  for (const wait of [0, 50, 150, 400]) {
    await page.keyboard.press("Control+K");
    await runInPalette(page, "Brightness: Light");
    await page.waitForTimeout(wait);
    await page.keyboard.press("Control+K");
    await expect(page.getByPlaceholder("Type a command or a tab name")).toHaveValue("");
    await page.keyboard.press("Escape");
    await expect(page.getByPlaceholder("Type a command or a tab name")).toBeHidden();
  }
});
