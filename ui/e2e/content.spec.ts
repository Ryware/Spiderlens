// Near duplicate content (spec 020) in a real browser: the Content section,
// clusters, and the comparison sheet's layout, keys and colours in light and
// dark. Events come from the in-memory transport (e2e/app.tsx).
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
});

test.afterEach(() => {
  expect(errors).toEqual([]);
});

async function openContent(page: Page, scheme: "light" | "dark") {
  await page.emulateMedia({ colorScheme: scheme });
  await page.goto("./");
  await expect(page.getByLabel("Start URL")).toBeFocused();
  await page.keyboard.type("example.com");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.crawls.finish({ report: window.spiderlens.fakeReport("completed", 5000) }));
  await page.getByRole("button", { name: "Content" }).click();
  await page.getByRole("button", { name: "Find near duplicates" }).click();
  await page.evaluate(() => window.spiderlens.results.nearDupProgress({ done: 2, total: 3, step: "Comparing pages" }));
  await expect(page.getByRole("meter", { name: "Analysis progress" })).toBeVisible();
  await page.evaluate(() => window.spiderlens.results.finishAnalysis());
  await expect(page.getByTestId("analysis-summary")).toContainText("near duplicates at 90%");
}

for (const scheme of ["light", "dark"] as const) {
  test(`clusters_and_comparison_in_${scheme}`, async ({ page }) => {
    await openContent(page, scheme);
    await expect(page.locator("html")).toHaveAttribute("data-tone", scheme);
    await page.getByRole("radio", { name: "Clusters" }).click();
    const clusters = page.getByRole("listbox", { name: "Clusters" });
    await expect(clusters.getByRole("option")).toHaveCount(2);
    // Arrow keys move between clusters and show each one's pages.
    await clusters.getByRole("option").first().focus();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("list", { name: "Pages in cluster 2" })).toBeVisible();
    await page.keyboard.press("ArrowUp");
    const members = page.getByRole("list", { name: "Pages in cluster 1" });
    await members.getByRole("button", { name: /^Compare https:\/\/example.com\/page-2/ }).click();

    const sheet = page.getByRole("dialog");
    await expect(sheet.getByText("97% similar")).toBeVisible();
    const box = (await sheet.boundingBox())!;
    const vp = page.viewportSize()!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
    expect(box.y + box.height).toBeLessThanOrEqual(vp.height);

    // Changed words carry a visible fill, different from the sheet's surface.
    const mark = sheet.locator('mark[data-op="changed"]').first();
    await expect(mark).toBeVisible();
    const [fill, surface] = await mark.evaluate((m) => [getComputedStyle(m).backgroundColor, getComputedStyle(m.closest("[role=dialog]")!).backgroundColor]);
    expect(fill).not.toBe(surface);
    expect(fill).not.toBe("rgba(0, 0, 0, 0)");

    // The two sides of an aligned paragraph share a row.
    const cells = sheet.getByRole("row").first().getByRole("cell");
    const [a, b] = [(await cells.nth(0).boundingBox())!, (await cells.nth(1).boundingBox())!];
    expect(Math.abs(a.y - b.y)).toBeLessThan(1);
    expect(b.x).toBeGreaterThan(a.x + a.width);

    await page.keyboard.press("]");
    await expect(sheet.getByText("2 of 3")).toBeVisible();
    await expect(sheet.getByText("93% similar")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(sheet).toBeHidden();
  });
}

test("inspector_duplicates_open_the_comparison", async ({ page }) => {
  await openContent(page, "light");
  await page.getByRole("grid").getByRole("row").nth(2).click();
  const matches = page.getByRole("complementary", { name: "Details" }).getByRole("list", { name: "Closest matches" });
  await expect(matches.getByRole("listitem")).toHaveCount(3);
  await matches.getByRole("button", { name: "Compare with https://example.com/page-3" }).click();
  await expect(page.getByRole("dialog").getByText("2 of 3")).toBeVisible();
});
