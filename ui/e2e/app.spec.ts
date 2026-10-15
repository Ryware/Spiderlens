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
  await page.getByRole("button", { name: "Internal", exact: true }).click();
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

/** Where a scene node is on the page, from the canvas camera. */
async function nodeOnPage(page: Page, key: string) {
  const canvas = page.getByRole("application");
  // The camera fits the scene over a few frames: read it once it holds still.
  await expect
    .poll(async () => {
      const a = await canvas.getAttribute("data-camera");
      await page.waitForTimeout(100);
      return a === (await canvas.getAttribute("data-camera"));
    })
    .toBe(true);
  const box = (await canvas.boundingBox())!;
  const [cx, cy, k] = (await canvas.getAttribute("data-camera"))!.split(",").map(Number);
  const state = await page.evaluate(() => window.spiderlens.results.viz[window.spiderlens.results.viz.length - 1]);
  const scene = await page.evaluate((s) => window.spiderlens.vizScene("/crawls/example.com.spiderlens", s), state);
  const n = scene.nodes.find((x) => x.key === key)!;
  return { x: box.x + n.x * k + cx, y: box.y + n.y * k + cy, node: n, scene };
}

test("crawl_tree_draws_and_selecting_a_node_shows_it_in_the_details", async ({ page }) => {
  await crawl(page);
  await page.getByRole("navigation", { name: "Sections" }).getByText("Crawl tree").click();
  const canvas = page.getByRole("application");
  await expect(canvas).toHaveAttribute("aria-label", /^Crawl tree: \d+ nodes/);
  // The canvas takes its size from its box once laid out.
  await expect.poll(async () => (await canvas.boundingBox())?.width ?? 0).toBeGreaterThan(300);
  const box = (await canvas.boundingBox())!;
  expect(box.width).toBeGreaterThan(300);
  expect(box.height).toBeGreaterThan(200);
  // Something was drawn: the canvas is not blank.
  await expect(canvas).toHaveAttribute("data-layout", "tree");
  // The tree is drawn on the next frames after its scene arrives.
  await expect
    .poll(() =>
      canvas.evaluate((c: HTMLCanvasElement) => {
        const d = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
        let n = 0;
        for (let i = 3; i < d.length; i += 4) if (d[i] > 0) n++;
        return n;
      }),
    )
    .toBeGreaterThan(100);
  // Hover shows a tooltip; a click selects and the details follow.
  const root = await nodeOnPage(page, "u:1");
  await page.mouse.move(root.x, root.y);
  await expect(page.getByRole("tooltip")).toContainText("https://example.com/");
  await page.mouse.click(root.x, root.y);
  await expect(page.getByRole("complementary", { name: "Details" })).toContainText("example.com");
  // Double-click a folded branch to open it; it stays where it was.
  const folded = root.scene.nodes.find((n) => n.expandable && !n.expanded)!;
  const before = await nodeOnPage(page, folded.key);
  await page.mouse.dblclick(before.x, before.y);
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.expanded ?? [])).toContain(folded.key);
  const after = await nodeOnPage(page, folded.key);
  expect(Math.abs(after.x - before.x)).toBeLessThan(2);
  expect(Math.abs(after.y - before.y)).toBeLessThan(2);
  expect(after.node.expanded).toBe(true);
});

test("tree_views_switch_layout_and_keep_working_from_the_keyboard", async ({ page }) => {
  await crawl(page);
  await page.keyboard.press("Control+K");
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeFocused();
  await runInPalette(page, "Directory tree");
  const canvas = page.getByRole("application");
  await expect(canvas).toHaveAttribute("aria-label", /^Directory tree/);
  await page.getByRole("radio", { name: "Radial" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.layout)).toBe("radial");
  // The radial scene is drawn and fitted before the camera is read.
  await expect(canvas).toHaveAttribute("data-layout", "radial");
  await canvas.focus();
  await page.keyboard.press("0");
  const cam = await canvas.getAttribute("data-camera");
  await page.keyboard.press("+");
  await expect(canvas).not.toHaveAttribute("data-camera", cam!);
  await page.keyboard.press("0");
  await expect(canvas).toHaveAttribute("data-camera", cam!);
  // In the crawl tree every node is a URL: the arrows select, the details follow.
  await page.getByRole("navigation", { name: "Sections" }).getByText("Crawl tree").click();
  await page.getByRole("application").focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("complementary", { name: "Details" })).toContainText("example.com/page-");
});

test("link_graph_drags_a_page_and_centres_on_one", async ({ page }) => {
  await crawl(page);
  await page.getByRole("navigation", { name: "Sections" }).getByText("Link graph").click();
  const canvas = page.getByRole("application");
  await expect(canvas).toHaveAttribute("aria-label", /^Link graph: \d+ nodes, \d+ links/);
  await expect(canvas).toHaveAttribute("data-layout", "force");
  // Drag a page: it is pinned where it was dropped, and stays under the mouse.
  const before = await nodeOnPage(page, "u:4");
  await page.mouse.move(before.x, before.y);
  await page.mouse.down();
  await page.mouse.move(before.x + 30, before.y + 20, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => Object.keys(window.spiderlens.results.viz.at(-1)?.pinned ?? {}))).toEqual(["u:4"]);
  const after = await nodeOnPage(page, "u:4");
  expect(Math.abs(after.x - (before.x + 30))).toBeLessThan(2);
  expect(Math.abs(after.y - (before.y + 20))).toBeLessThan(2);
  await page.getByRole("button", { name: "Reset layout" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.pinned)).toEqual({});
  // Double-click a page to centre on it; the details follow the click.
  const root = await nodeOnPage(page, "u:1");
  await page.mouse.dblclick(root.x, root.y);
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.focus)).toBe("u:1");
  await expect(page.getByText(/^Around /)).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Details" })).toContainText("example.com");
  await page.getByRole("radio", { name: "2 links" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.distance)).toBe(2);
});

test("anchor_text_cloud_from_the_details_lists_a_terms_links", async ({ page }) => {
  await crawl(page);
  // Select a page in the crawl tree, then open its cloud from the details.
  await page.getByRole("navigation", { name: "Sections" }).getByText("Crawl tree").click();
  const tree = page.getByRole("application");
  await tree.focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  const details = page.getByRole("complementary", { name: "Details" });
  await details.getByRole("button", { name: "Anchor text cloud" }).click();
  const canvas = page.getByRole("application");
  await expect(canvas).toHaveAttribute("aria-label", /^Anchor text: \d+ nodes/);
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.focus)).toMatch(/^u:\d+$/);
  await expect(canvas).toHaveAttribute("data-layout", "cloud");
  // The cloud is drawn on the next frames after its scene arrives.
  await expect
    .poll(() =>
      canvas.evaluate((c: HTMLCanvasElement) => {
        const d = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
        let n = 0;
        for (let i = 3; i < d.length; i += 4) if (d[i] > 0) n++;
        return n;
      }),
    )
    .toBeGreaterThan(100);
  // Click the biggest term: its links open; a source drives the details.
  const term = await nodeOnPage(page, "t:running");
  await page.mouse.click(term.x, term.y);
  const list = page.getByRole("region", { name: "Links using running" });
  await expect(list).toContainText("9 links");
  await list.getByRole("listitem").first().getByRole("button").click();
  await expect(details).toContainText("example.com");
  await page.getByRole("radio", { name: "Whole site" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.viz.at(-1)?.focus)).toBeNull();
});

test("export_a_view_from_the_toolbar", async ({ page }) => {
  await crawl(page);
  await page.getByRole("navigation", { name: "Sections" }).getByText("Directory tree").click();
  await expect(page.getByRole("application")).toHaveAttribute("aria-label", /^Directory tree/);
  await page.getByRole("button", { name: "Export" }).click();
  const dialog = page.getByRole("dialog", { name: "Export view" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("radio", { name: "2x" }).click();
  await dialog.getByRole("button", { name: "Export…" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("status").filter({ hasText: /^Saved / })).toContainText(".png");
  const sent = await page.evaluate(() => window.spiderlens.results.vizExports.at(-1));
  expect(sent?.options).toMatchObject({ format: "png", scale: 2 });
  // The colours sent are the ones the window resolved.
  expect(sent?.options.palette["--viz-ok"]).toMatch(/^(oklch|rgb|#)/);
  expect(sent?.view.view).toBe("directory_tree");
});

test("link_position_tester_runs_the_rules_on_a_crawled_page", async ({ page }) => {
  await crawl(page);
  await page.getByRole("button", { name: "Configure" }).click();
  await page.getByRole("tab", { name: /^Link analysis/ }).click();
  const panel = page.getByRole("tabpanel", { name: "Link analysis" });
  await panel.getByRole("button", { name: "Add rule" }).click();
  await panel.getByLabel("Rule 1 selector").fill(".mega-menu");
  await panel.getByLabel("Page URL").fill("https://example.com/");
  await panel.getByLabel("Page URL").press("Enter");
  const promo = panel.getByRole("row").filter({ hasText: "/promo" });
  await expect(promo).toContainText("Navigation");
  await expect(promo).toContainText("Rule 1");
  await expect(panel.getByRole("row").filter({ hasText: "/story" })).toContainText("Built-in rule");
});

test("link_score_runs_after_the_crawl_with_progress_and_cancel", async ({ page }) => {
  await crawl(page);
  const bar = page.getByRole("contentinfo", { name: "Crawl status" });
  await expect(bar).toContainText("Analysing: Link positions");
  await page.evaluate(() => window.spiderlens.results.analysisProgress({ phase: "link_score", done: 3, total: 4 }));
  await expect(bar).toContainText("Analysing: Link Score 75%");
  await bar.getByRole("button", { name: "Cancel analysis" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.analysisCancels)).toBe(1);
  await page.evaluate(() => window.spiderlens.results.analysisDone({ cancelled: true }));
  await expect(bar).toContainText("Analysis cancelled");
  await bar.getByRole("button", { name: "Run analysis again" }).click();
  await page.evaluate(() => window.spiderlens.results.analysisDone());
  await expect(bar).toContainText("Link Score ready");
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

// Spec 027 part E: the AMP tab lists AMP pages and says its checks are a subset.
test("amp_tab_lists_amp_pages_with_the_core_subset_note", async ({ page }) => {
  await page.evaluate(() => {
    const s = window.spiderlens;
    s.results.features.push("amp");
    s.results.urls[1].ampFailures = 0;
    s.results.urls[2].ampFailures = 3;
  });
  await crawl(page);
  await page.getByRole("button", { name: "AMP" }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  const tab = page.getByRole("region", { name: "AMP" });
  await expect(tab.getByText("These checks are a core subset of AMP's rules, not the official AMP validator.")).toBeVisible();
  await expect(page.getByTestId("row-count")).toHaveText("2 URLs");
  await tab.getByRole("button", { name: /^Failing a check/ }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1 URL");
});

// Spec 027 part D: spelling is checked after the crawl, from the Spelling tab.
test("check_spelling_from_the_spelling_tab", async ({ page }) => {
  await crawl(page);
  await page.getByRole("button", { name: "Spelling" }).click();
  const tab = page.getByRole("region", { name: "Spelling" });
  await tab.getByRole("button", { name: "Check spelling" }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.spelling.started.length)).toBe(1);
  await page.evaluate(() => window.spiderlens.spelling.progress({ checked: 1200, total: 5000 }));
  await expect(tab.getByText("Checking spelling: 1,200 of 5,000 pages")).toBeVisible();
  await page.evaluate(() => {
    const s = window.spiderlens;
    s.results.features.push("spelling");
    s.results.urls[3].spellErrors = 2;
    s.spelling.finish({ pages: 5000, errors: 2, distinct_words: 2 });
  });
  await expect(tab.getByTestId("spell-summary")).toContainText("2 errors (2 different words) in 5,000 pages");
  await tab.getByRole("button", { name: /^Spelling errors/ }).first().click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1 URL");
});

// Spec 027 part C: PDFs have their own tab, and the details say what was read.
test("pdfs_tab_lists_pdfs_and_details_show_what_was_read", async ({ page }) => {
  await page.evaluate(() => {
    const s = window.spiderlens;
    s.results.features.push("pdf");
    s.results.add([
      { id: 9001, url: "https://example.com/annual-report.pdf", status: 200, title: null, depth: 1, internal: true, pdf: s.fakePdf() },
      { id: 9002, url: "https://example.com/scan.pdf", status: 200, title: null, depth: 1, internal: true, pdf: s.fakePdf({ title: null, words: 0, no_text: true }) },
    ]);
  });
  await crawl(page);
  await page.getByRole("button", { name: "PDFs" }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("2 URLs");
  const grid = page.getByRole("grid");
  await expect(grid.getByRole("row").nth(1)).toContainText("Annual report");
  await grid.getByRole("row").nth(2).click();
  const details = page.getByRole("complementary", { name: "Details" });
  await expect(details.getByRole("heading", { name: "PDF", exact: true })).toBeVisible();
  await expect(details.getByText("No text (scanned)")).toBeVisible();
  await page.getByRole("button", { name: /Missing title/ }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1 URL");
});

test("compare_two_crawls_and_open_a_changed_url", async ({ page }) => {
  // Spec 012: setup sheet, progress, overview, a group list and the
  // side-by-side detail, in a real browser.
  await crawl(page);
  await page.keyboard.press("Control+K");
  await runInPalette(page, "Compare crawls");
  const sheet = page.getByRole("dialog", { name: "Compare crawls" });
  await expect(sheet.getByLabel("Previous file", { exact: true })).toHaveText("example.com-old.spiderlens");
  await expect(sheet.getByLabel("Previous crawl", { exact: true })).toContainText("Complete");
  await sheet.getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByRole("meter", { name: "Comparison progress" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.compare.finish());

  const filters = page.getByRole("table", { name: "Filter changes" });
  await expect(filters).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Sections" }).getByRole("button", { name: "Comparison", exact: true })).toHaveAttribute("aria-current", "page");
  await page.getByRole("button", { name: "Missing URLs: 10" }).click();
  const list = page.getByRole("grid", { name: "Missing URLs URLs" });
  await expect(page.getByTestId("group-count")).toHaveText("10 URLs");
  await list.getByRole("row").nth(1).click();
  const detail = page.getByRole("table", { name: "Previous and current" });
  await expect(detail).toContainText("Not in this crawl");

  const { scrollH, innerH } = await page.evaluate(() => ({ scrollH: document.documentElement.scrollHeight, innerH: window.innerHeight }));
  expect(scrollH).toBeLessThanOrEqual(innerH);

  await page.getByRole("button", { name: "Close comparison" }).click();
  await expect(page.getByRole("navigation", { name: "Sections" }).getByRole("button", { name: "Comparison", exact: true })).toHaveCount(0);
});

test("issue_to_rows_to_inspector_and_a_threshold_change_updates_counts", async ({ page }) => {
  await crawl(page);
  const issues = page.getByRole("list", { name: "Issues" });
  const fourxx = issues.getByRole("button", { name: /Internal client error/ });
  await expect(fourxx).toContainText("500");
  await fourxx.click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("500 URLs");
  await expect(page.getByTestId("issue-header")).toContainText("Restore the page");

  // On the Internal tab, a broken page's inspector lists its issue, and
  // clicking the issue goes back to its URLs.
  await page.getByRole("button", { name: "Internal", exact: true }).click();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("issue-header")).toHaveCount(0);
  await page.getByRole("gridcell", { name: "https://example.com/page-9", exact: true }).click();
  const section = page.getByRole("complementary", { name: "Details" }).locator("section").filter({ has: page.getByRole("heading", { name: /^Issues/ }) });
  await expect(section).toContainText("Internal client error");
  await section.getByRole("button", { name: /Internal client error/ }).click();
  await expect(page.getByTestId("issue-header")).toBeVisible();

  // A stricter threshold in Settings › Issues changes the count.
  await page.getByRole("button", { name: "Configure" }).click();
  await page.getByRole("navigation", { name: "Settings categories" }).getByRole("tab", { name: /^Issues/ }).click();
  await page.getByLabel("Page title: below 30 characters threshold").fill("7");
  await page.keyboard.press("Escape");
  await issues.getByRole("button", { name: /^Medium/ }).click();
  await expect(issues.getByRole("button", { name: /Page title: below 7 characters/ })).toContainText("9");
});

test("custom_rule_is_tested_run_on_the_crawl_and_gets_its_tab", async ({ page }) => {
  // Spec 013: add a rule, try it on a page, run it on the open crawl from
  // stored HTML, then read its column in the new Custom extraction tab.
  await crawl(page);
  await page.keyboard.press("Control+Comma");
  const nav = page.getByRole("navigation", { name: "Settings categories" });
  await nav.getByRole("tab", { name: /^Custom rules/ }).click();
  const panel = page.getByRole("tabpanel", { name: "Custom rules" });
  await panel.getByRole("button", { name: "Add extractor" }).click();
  await panel.getByLabel("Extractor 1 name").fill("Price");
  await panel.getByLabel("Extractor 1 expression").fill(".price");
  await panel.getByLabel("Page to test").fill("https://example.com/product");
  await page.keyboard.press("Enter");
  await expect(panel.getByRole("table", { name: "Test results" }).getByRole("row").nth(1)).toContainText("$10");

  const run = panel.getByRole("button", { name: /Run on the open crawl/ });
  await run.scrollIntoViewIfNeeded();
  await run.click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.custom.runs.length)).toBe(1);
  await page.evaluate(() => window.spiderlens.custom.progress({ done: 2500, total: 5000, per_sec: 4000 }));
  await expect(panel.getByText("2,500 of 5,000 pages")).toBeVisible();
  await page.evaluate(() => {
    const s = window.spiderlens;
    s.results.customTabs = [
      {
        id: "custom_extraction",
        title: "Custom extraction",
        columns: [
          { id: "address", label: "Address", kind: "url", width: 420, default_visible: true, need: "base" },
          { id: "x0", label: "Price", kind: "text", width: 200, default_visible: true, need: "base" },
        ],
        filters: [
          { id: "all", label: "All", need: "base" },
          { id: "x0_has", label: "Price: has a value", need: "base" },
          { id: "x0_none", label: "Price: no value", need: "base" },
        ],
        search_column: "x0",
        scoped: false,
      },
    ];
    for (let id = 1; id <= 5000; id++) s.results.customCells.set(id, { x0: id % 4 ? `$${id}` : null });
    s.custom.done({ state: "complete", pages: 5000 });
  });
  await expect(panel.getByText(/Rules ran on 5,000 pages/)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(nav).toBeHidden();

  await page.getByRole("navigation", { name: "Sections" }).getByRole("button", { name: "Custom extraction" }).click();
  await expect(page.getByRole("grid")).toBeVisible();
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("5,000 URLs");
  const grid = page.getByRole("grid");
  await expect(grid.getByRole("columnheader", { name: /Price/ })).toBeVisible();
  await expect(grid.getByRole("row").nth(1)).toContainText("$1");
  await page.getByRole("button", { name: /^Price: no value/ }).click();
  await expect.poll(() => page.evaluate(() => window.spiderlens.results.opened.at(-1)?.filter)).toBe("x0_none");
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1,250 URLs");
});

test("integrations_fetch_search_console_for_the_open_crawl", async ({ page }) => {
  await crawl(page);
  await page.getByRole("button", { name: "Integrations" }).click();
  await expect(page.getByRole("heading", { name: "Integrations" })).toBeVisible();
  await page.getByRole("button", { name: "Connect Google account" }).click();
  await expect(page.getByRole("list", { name: "Google accounts" })).toContainText("ana@example.com");
  await page.locator('[data-provider="search_console"]').getByRole("switch").click();
  await expect(page.getByLabel("Property")).toHaveValue("sc-domain:example.com");
  await page.getByRole("button", { name: "Fetch Search Console data" }).click();
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.integrations.finishRun());
  await expect(page.getByRole("list", { name: "Runs" })).toContainText("Finished");
  // The screen scrolls inside the main pane, never the window.
  const { scrollW, innerW, scrollH, innerH } = await page.evaluate(() => ({
    scrollW: document.documentElement.scrollWidth,
    innerW: window.innerWidth,
    scrollH: document.documentElement.scrollHeight,
    innerH: window.innerHeight,
  }));
  expect(scrollW).toBeLessThanOrEqual(innerW);
  expect(scrollH).toBeLessThanOrEqual(innerH);
});

test("generate_sitemap_sheet_fits_and_writes_into_the_chosen_folder", async ({ page }) => {
  await crawl(page);
  await page.evaluate(() => (window.spiderlens.picks.folder = "/site/public"));
  await page.keyboard.press("Control+K");
  await runInPalette(page, "sitemap", "Generate XML sitemap…");
  const dialog = page.getByRole("dialog", { name: "Generate XML sitemap" });
  await expect(dialog).toBeVisible();
  const box = (await dialog.boundingBox())!;
  const vp = page.viewportSize()!;
  expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
  expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
  await expect(dialog.getByText(/4,500 pages in 1 file/)).toBeVisible();
  await dialog.getByLabel("URLs per file").fill("1000");
  await expect(dialog.getByText(/4,500 pages in 6 files/)).toBeVisible();
  await dialog.getByRole("button", { name: /Choose folder/ }).click();
  await dialog.getByRole("button", { name: "Generate", exact: true }).click();
  await expect(dialog.getByText(/Wrote 6 files with 4,500 pages to \/site\/public/)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("sitemap_files_tree_opens_from_the_sidebar_and_works_by_keyboard", async ({ page }) => {
  await page.evaluate(() => {
    const s = window.spiderlens;
    s.sitemaps.files = {
      available: true,
      files: [
        s.fakeSitemapFile(1, "https://example.com/index.xml", { kind: "index", entries: 2, children: 2 }),
        s.fakeSitemapFile(2, "https://example.com/a.xml", { parent_id: 1, level: 1 }),
        s.fakeSitemapFile(3, "https://example.com/b.xml", { parent_id: 1, level: 1, kind: null, error_class: "invalid_xml", error: "unexpected end", error_line: 3 }),
      ],
    };
  });
  await crawl(page);
  await page.getByRole("navigation", { name: "Sections" }).getByRole("button", { name: "Sitemap files" }).click();
  const tree = page.getByRole("tree", { name: "Sitemap files" });
  await expect(tree.getByRole("treeitem")).toHaveCount(3);
  await tree.getByRole("treeitem").first().click();
  await page.keyboard.press("ArrowLeft");
  await expect(tree.getByRole("treeitem")).toHaveCount(1);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowDown");
  await expect(tree.getByRole("treeitem").nth(2)).toBeFocused();
  await expect(page.getByRole("region", { name: "Selected sitemap file" })).toContainText("unexpected end (line 3)");

});

test("crawl_speed_popover_explains_the_limit_and_fits_the_window", async ({ page }) => {
  // Spec 008 FR-028, FR-029: the reason in the status bar, every site's pace on click.
  await page.keyboard.type("example.com");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() =>
    window.spiderlens.crawls.progress({
      crawled: 1200,
      queued: 3800,
      limit: { kind: "site_overload", host: "https://example.com:443", seconds_left: null, message: "https://example.com:443 answered 429 or 503 (overloaded), so it gets 4 at once." },
      sites: [
        { host: "https://example.com:443", state: "slowed_down", allowance: 4, max: 32, in_flight: 4, response_ms: 240, crawled: 1100, waiting: 3700, waiting_more: false, reason: "Answered 429 or 503 (overloaded), so cut to 4 at once.", seconds_left: null },
        { host: "https://cdn.example.com:443", state: "speeding_up", allowance: 6, max: 32, in_flight: 6, response_ms: 35, crawled: 100, waiting: 100, waiting_more: false, reason: "6 of 32 at once, one more each time a round of answers comes back steady.", seconds_left: null },
      ],
      sites_total: 2,
    }),
  );
  const trigger = page.getByRole("contentinfo", { name: "Crawl status" }).locator('[data-figure="speed"]');
  await expect(trigger).toHaveText("Site overloaded · example.com");
  await trigger.click();
  const table = page.getByRole("table", { name: "Sites" });
  await expect(table.getByRole("row")).toHaveCount(3);
  const box = (await page.getByRole("dialog").boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(1400);
  await page.keyboard.press("Escape");
  await expect(table).toBeHidden();
});

test("columns_reorder_from_the_keyboard_in_the_columns_menu", async ({ page }) => {
  const grid = await openInternal(page);
  const headers = () => grid.getByRole("columnheader").allTextContents();
  await page.getByRole("button", { name: "Columns" }).click();
  const menu = page.getByRole("menu");
  // Arrow down to Title, then move it up twice. Where the first arrow lands
  // after a click-open differs by platform (macOS Chromium starts one item
  // earlier), so step until Title has focus rather than counting presses.
  const title = menu.getByRole("menuitemcheckbox", { name: /^Title/ });
  await expect(menu).toBeVisible();
  for (let i = 0; i < 6 && !(await title.evaluate((el) => el === document.activeElement)); i++) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(title).toBeFocused();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(page.getByRole("status").filter({ hasText: "Title moved to position 2 of 4" })).toHaveCount(1);
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(headers).toEqual(["Title", "Address", "Status code", "H1"]);
  await expect(menu.getByRole("menuitemcheckbox", { name: /^Title/ })).toBeFocused();
  // The menu's own arrows carry on from the moved item's new place.
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitemcheckbox", { name: /^Address/ })).toBeFocused();
  await expect(menu.getByRole("menuitemcheckbox", { name: /^Address/ })).toHaveAttribute("data-highlighted", "");
  await page.keyboard.press("Escape");
  await page.reload();
  await expect(page.getByLabel("Start URL")).toBeFocused();
  const again = await openInternal(page);
  await expect.poll(() => again.getByRole("columnheader").allTextContents()).toEqual(["Title", "Address", "Status code", "H1"]);
});

test("mod_digit_switches_tabs_and_enter_opens_the_inspector", async ({ page }) => {
  const grid = await openInternal(page);
  await grid.getByRole("row").nth(3).click();
  await page.keyboard.press("Enter");
  const details = page.getByRole("complementary", { name: "Details" });
  await expect(details.getByRole("heading", { level: 2 })).toHaveText("https://example.com/page-2");
  await expect.poll(() => page.evaluate(() => document.activeElement?.contains(document.querySelector('aside[aria-label="Details"]')))).toBe(true);
  await page.keyboard.press("Escape");
  await expect(grid).toBeFocused();
  // Internal, Links (Link Score), External, Response codes.
  await page.keyboard.press("Control+4");
  await expect(page.getByRole("region", { name: "Response codes" })).toBeVisible();
  await page.keyboard.press("Control+1");
  await expect(page.getByRole("region", { name: "Internal" })).toBeVisible();
});

test("pasted_cookies_reach_the_crawl_and_the_cookie_audit_fits_the_window", async ({ page }) => {
  // Spec 025 US3 and US5: cookies from a signed-in browser, then the audit.
  await page.getByLabel("Start URL").fill("example.com");
  await page.keyboard.press("Control+Comma");
  const nav = page.getByRole("navigation", { name: "Settings categories" });
  await nav.getByRole("tab", { name: /^Sign-in and cookies/ }).click();
  await page.getByRole("button", { name: "Paste a Cookie line" }).click();
  await page.getByLabel("Cookie line").fill("Cookie: sid=abc; theme=dark");
  await page.getByRole("button", { name: "Add cookies" }).click();
  await expect(page.getByLabel("Cookie 1 value")).toHaveAttribute("type", "password");
  await expect(page.getByLabel("Cookie 2 domain")).toHaveValue("example.com");
  await page.getByRole("radiogroup", { name: "Cookies the site sets" }).getByRole("radio", { name: "Keep" }).click();
  await page.keyboard.press("Escape");
  await expect(nav).toBeHidden();

  await page.evaluate(() => {
    const s = window.spiderlens;
    s.results.features = ["base", "extraction", "cookies"];
    s.results.cookies = [
      s.fakeCookie("sid", Array.from({ length: 300 }, (_, i) => i + 1)),
      s.fakeCookie("ad_id", [7], { domain: "ads.example.net", third_party: true, same_site: "none" }),
    ];
  });
  await page.getByLabel("Start URL").focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  const config = await page.evaluate(() => window.spiderlens.crawls.started[0].config.access);
  expect(config.cookie_policy).toBe("keep");
  expect(config.cookies.map((c) => [c.name, c.value])).toEqual([
    ["sid", { secret: true, value: "abc" }],
    ["theme", { secret: true, value: "dark" }],
  ]);
  await page.evaluate(() => window.spiderlens.crawls.finish({ report: window.spiderlens.fakeReport("completed", 5000) }));
  await expect(page.getByRole("status").filter({ hasText: "Completed" })).toBeVisible();

  await page.getByRole("button", { name: "Cookies" }).click();
  const table = page.getByRole("table", { name: /^Cookies/ });
  await expect(table.getByRole("row")).toHaveCount(3);
  await table.getByRole("row").nth(1).focus();
  await page.keyboard.press("Enter");
  const urls = page.getByRole("region", { name: "URLs that set sid" });
  await expect(urls).toContainText("(300)");
  // The URL list scrolls inside its pane; the window itself never scrolls.
  const vp = page.viewportSize()!;
  const box = (await urls.boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
  expect(await page.evaluate(() => document.scrollingElement!.scrollHeight <= window.innerHeight)).toBe(true);
  await urls.getByText("https://example.com/page-6", { exact: true }).click();
  const details = page.getByRole("complementary", { name: "Details" });
  await expect(details.getByRole("heading", { level: 2 })).toHaveText("https://example.com/page-6");
  await expect(details.getByRole("list", { name: "Cookies set" })).toContainText("sid");
});

test("page_html_viewer_windows_searches_and_copies", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.evaluate(() => {
    window.spiderlens.results.features = ["base", "extraction", "timing", "raw_html"];
  });
  const grid = await openInternal(page);
  await grid.getByRole("row").nth(2).click();
  await page.getByRole("complementary", { name: "Details" }).getByRole("button", { name: "View source" }).click();
  const dialog = page.getByRole("dialog");
  const source = dialog.getByRole("region", { name: "Page source" });
  await expect(source).toContainText("<!doctype html>");
  await expect(dialog.getByLabel("Find in source")).toBeFocused();
  const box = (await dialog.boundingBox())!;
  const vp = page.viewportSize()!;
  expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
  expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
  // 2,008 lines, but only those on screen exist in the page.
  expect(await source.locator("[data-row]").count()).toBeLessThan(80);
  await page.keyboard.type("paragraph 1999");
  await expect(dialog.getByTestId("match-count")).toHaveText("1 of 1");
  await expect(source.locator("mark[data-current]")).toBeInViewport();
  await dialog.getByRole("button", { name: "Copy" }).click();
  await expect(dialog.getByText("Copied.")).toBeVisible();
  // The Windows clipboard hands text back with CRLF line ends.
  const copied = (await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, "\n");
  expect(copied.startsWith("<!doctype html>\n")).toBe(true);
  expect(copied.split("\n").length).toBe(2009);
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("inspector_rows_copy_and_show_in_table", async ({ page }) => {
  const grid = await openInternal(page);
  await grid.getByRole("row").nth(1).click();
  await page.keyboard.press("End");
  const details = page.getByRole("complementary", { name: "Details" });
  await expect(details.getByRole("heading", { level: 2 })).toHaveText("https://example.com/page-4999");
  // The actions show on hover and on keyboard focus.
  const show = details.getByRole("button", { name: "Show https://example.com/ in table" });
  const shown = show.locator("..");
  await expect(shown).toHaveCSS("opacity", "0");
  await show.locator("xpath=ancestor::li").hover();
  await expect(shown).toHaveCSS("opacity", "1");
  const copy = details.getByRole("button", { name: "Copy https://example.com/", exact: true });
  await copy.focus();
  await page.keyboard.press("Enter");
  await expect(details.getByRole("status")).toHaveText("Copied.");
  expect(await page.evaluate(() => window.spiderlens.results.copied)).toEqual(["https://example.com/"]);
  // Right-click offers the same; Show in table scrolls back to the top row.
  await copy.locator("xpath=ancestor::li").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Show in table" }).click();
  const selected = grid.locator('[role="row"][aria-selected="true"]');
  await expect(selected).toHaveAttribute("data-id", "1");
  await expect(selected).toBeInViewport();
  await expect(grid).toBeFocused();
  await expect(details.getByRole("heading", { level: 2 })).toHaveText("https://example.com/");
});

test("create_a_schedule_see_its_next_run_open_history_and_turn_it_off", async ({ page }) => {
  await page.keyboard.type("example.com");
  await page.keyboard.press("Control+K");
  await runInPalette(page, "Schedule this crawl");
  const sheet = page.getByRole("dialog", { name: "New schedule" });
  await expect(sheet.getByLabel("Name")).toBeFocused();
  // The sheet fits the window, footer included.
  const box = (await sheet.boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(860);
  await sheet.getByRole("radio", { name: "Daily" }).click();
  await sheet.getByLabel("At", { exact: true }).fill("07:30");
  await expect(sheet.getByText("Every day at 07:30")).toBeVisible();
  await expect(sheet.getByRole("list", { name: "Next runs" }).getByRole("listitem")).toHaveCount(5);
  await sheet.getByRole("button", { name: "Create schedule" }).click();
  await expect(sheet).toBeHidden();

  const list = page.getByRole("list", { name: "Schedules" });
  await expect(list.getByText("example.com weekly")).toBeVisible();
  await expect(list.getByText("Every day at 07:30")).toBeVisible();
  await expect(list.getByText(/^Next: /)).toBeVisible();

  const detail = page.getByRole("region", { name: "example.com weekly" });
  await expect(detail.getByRole("heading", { name: "History" })).toBeVisible();
  await expect(detail.getByText("No runs yet.")).toBeVisible();

  await detail.getByRole("switch", { name: "Enabled" }).click();
  await expect(list.getByText("Off")).toBeVisible();
  await expect(list.getByText("No upcoming runs")).toBeVisible();
});

test("segments_defined_in_settings_filter_every_tab", async ({ page }) => {
  // Spec 027 part B: define two segments, filter by one, switch tab.
  await openInternal(page);
  await page.keyboard.press("Control+,");
  const sheet = page.getByRole("dialog");
  await sheet.getByRole("tab", { name: /^Segments/ }).click();
  await sheet.getByRole("button", { name: "Add segment" }).click();
  await sheet.getByLabel("Segment name").fill("Ones");
  await sheet.getByLabel("Ones condition 1: value").fill("/page-1");
  await sheet.getByRole("button", { name: "Add segment" }).click();
  await sheet.getByLabel("Segment name").nth(1).fill("Twos");
  await sheet.getByLabel("Twos condition 1: value").fill("/page-2");
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();

  const filter = page.getByLabel("Segment", { exact: true });
  // page-1, page-10..19, page-100..199, page-1000..1999.
  await expect(filter.locator("option", { hasText: "Ones" })).toHaveText("Ones (1,111)");
  await filter.selectOption({ label: "Ones (1,111)" });
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1,111 URLs");
  await expect(page.getByRole("grid").getByRole("row").nth(1)).toContainText("page-1");

  await page.getByRole("button", { name: "Response codes" }).click();
  await expect(filter).toHaveValue("1");
  await page.evaluate(() => window.spiderlens.results.ready());
  await expect(page.getByTestId("row-count")).toHaveText("1,111 URLs");
});

test("search_preview_measures_while_typing", async ({ page }) => {
  // Spec 027 US A2: the preview in the detail pane, a real font in a real
  // browser, the width and cut changing as the user types.
  const grid = await openInternal(page);
  await grid.getByRole("row").nth(2).click();
  const details = page.getByRole("complementary", { name: "Details" });
  await expect(details.getByText("Search preview")).toBeVisible();
  await expect(details.getByTestId("preview-title")).toHaveText("Page 1");
  await details.getByRole("button", { name: "Try other wording" }).click();
  await details
    .getByRole("textbox", { name: "Title" })
    .fill("The Complete Beginner Guide To Choosing The Right Running Shoes For Every Kind Of Runner");
  await expect(details.getByTestId("preview-title")).toHaveText(/…$/);
  await expect(details.getByText("Cut")).toBeVisible();
  await details.getByRole("radio", { name: "Mobile" }).click();
  await expect(details.getByRole("figure", { name: "Mobile search result" })).toBeVisible();
});

test("report_fits_the_window_and_its_figures_open_rows", async ({ page }) => {
  await crawl(page);
  await page.getByRole("button", { name: "View report" }).click();
  await expect(page.getByTestId("score")).toHaveText("72");
  // The report scrolls inside the main area; the window never scrolls.
  const scroll = await page.evaluate(() => [document.documentElement.scrollHeight, window.innerHeight]);
  expect(scroll[0]).toBeLessThanOrEqual(scroll[1]);
  await page.getByRole("button", { name: "Client errors (4xx): 2" }).click();
  await expect(page.getByRole("button", { name: "Response codes" })).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("grid")).toBeVisible();
});

test("report_from_the_palette_and_bulk_export_by_keyboard", async ({ page }) => {
  await crawl(page);
  await page.keyboard.press("Control+K");
  await runInPalette(page, "Crawl report");
  await expect(page.getByTestId("score")).toBeVisible();
  await page.getByRole("button", { name: /Bulk export/ }).focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Bulk export" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Export…" }).click();
  await page.evaluate(() => window.spiderlens.audit.progress({ table: "All links", tables_done: 3, tables_total: 12, rows: 20000 }));
  await expect(dialog.getByRole("meter", { name: "Bulk export progress" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.audit.finishBulk());
  await expect(dialog).toContainText("Exported 30 rows in 1 tables");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});
