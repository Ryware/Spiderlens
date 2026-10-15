// Manage crawls and Settings › Storage in a real browser (spec 019): the
// sheet fits the window, rows are chosen with keyboard and pointer, deleting
// asks first, and the storage settings are reachable from the keyboard.
import { expect, test, type Page } from "@playwright/test";
import type { E2eHandle } from "./app";

declare global {
  interface Window {
    spiderlens: E2eHandle;
  }
}

const errors: string[] = [];

test.beforeEach(async ({ page }) => {
  errors.length = 0;
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("./");
  await expect(page.getByLabel("Start URL")).toBeFocused();
  await page.evaluate(() => {
    const f = window.spiderlens.files;
    const MB = 1024 ** 2;
    f.seedUnsaved("/data/Unsaved/quick.spiderlens", 300 * MB, { name: "quick", last_used: "2026-10-12T10:00:00Z" });
    f.seedUnsaved("/data/Unsaved/old.spiderlens", 50 * MB, { name: "old", last_used: "2026-09-01T10:00:00Z" });
    f.sizes.set("/data/Unsaved/quick.spiderlens", 300 * MB);
    f.html.set("/data/Unsaved/quick.spiderlens", { bytes: 200 * MB, pages: 100 });
  });
});

test.afterEach(() => {
  expect(errors).toEqual([]);
});

async function openManager(page: Page) {
  await page.keyboard.press("Control+k");
  // Keys typed before the search box has focus are lost (seen on macOS),
  // and "crawls" alone also matches Compare crawls….
  await expect(page.getByPlaceholder("Type a command or a tab name")).toBeFocused();
  await page.keyboard.type("Manage crawls");
  await expect(page.locator("[cmdk-item][data-selected=true]")).toContainText("Manage crawls");
  await page.keyboard.press("Enter");
  const sheet = page.getByRole("dialog", { name: "Manage crawls" });
  await expect(sheet.getByRole("table", { name: "Crawls" })).toBeVisible();
  return sheet;
}

test("manage_crawls_fits_the_window_and_closes_with_escape", async ({ page }) => {
  const sheet = await openManager(page);
  const box = (await sheet.boundingBox())!;
  const view = page.viewportSize()!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(view.width);
  expect(box.y + box.height).toBeLessThanOrEqual(view.height);
  await expect(sheet.getByTestId("html-size").first()).toHaveText("200 MB");
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
});

test("select_with_keyboard_and_pointer_then_delete_to_the_trash", async ({ page }) => {
  const sheet = await openManager(page);
  await sheet.getByRole("checkbox", { name: "Select quick" }).focus();
  await page.keyboard.press("Space");
  await sheet.getByRole("checkbox", { name: "Select old" }).click();
  const bar = sheet.getByRole("toolbar", { name: "Selected crawls" });
  await expect(bar).toContainText("2 selected");
  await bar.getByRole("button", { name: "Delete…" }).click();
  const confirm = page.getByRole("dialog", { name: "Move 2 crawls to the Trash?" });
  await expect(confirm).toContainText("Frees 350 MB");
  await confirm.getByRole("button", { name: "Move to Trash" }).click();
  await expect(sheet.getByText("No crawls here.")).toBeVisible();
  expect(await page.evaluate(() => window.spiderlens.files.trashed.slice().sort())).toEqual([
    "/data/Unsaved/old.spiderlens",
    "/data/Unsaved/quick.spiderlens",
  ]);
});

test("sorting_by_size_from_the_column_header", async ({ page }) => {
  const sheet = await openManager(page);
  await sheet.getByRole("button", { name: "Size" }).click();
  await expect(sheet.getByRole("checkbox", { name: /^Select (?!all )/ }).first()).toHaveAccessibleName("Select quick");
  await sheet.getByRole("button", { name: "Size" }).click();
  await expect(sheet.getByRole("checkbox", { name: /^Select (?!all )/ }).first()).toHaveAccessibleName("Select old");
});

test("storage_settings_from_the_keyboard", async ({ page }) => {
  await page.keyboard.press("Control+k");
  await page.keyboard.type("Settings: Storage");
  await expect(page.locator("[cmdk-item][data-selected=true]")).toContainText("Storage");
  await page.keyboard.press("Enter");
  const panel = page.getByRole("tabpanel", { name: "Storage" });
  await expect(panel.getByRole("switch", { name: "Remove unused unsaved crawls" })).toBeVisible();
  const reserve = panel.getByLabel("Keep free (GB)");
  await reserve.fill("3");
  await page.keyboard.press("Tab");
  await expect.poll(() => page.evaluate(() => window.spiderlens.files.prefs.reserve_bytes)).toBe(3 * 1024 ** 3);
});
