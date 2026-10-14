// Issues in the app (spec 009 US1, US2, US4): the sidebar group, an issue's
// URLs with its explanation, the inspector's issues and Settings › Issues.
// The rules themselves are tested in Rust (crates/results/tests/rules.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { withOverride } from "./IssueSettingsPanel";
import { applySettings } from "../testing/fakeIssues";

const grid = () => screen.getByRole("grid");
const rows = () => within(grid()).getAllByRole("row").slice(1);
const rowCount = () => screen.getByTestId("row-count").textContent;
const issuesNav = () => screen.getByRole("list", { name: "Issues" });

async function crawled(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await screen.findByRole("list", { name: "Issues" });
  return t;
}

async function openSettings(category: string) {
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const n = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(n).getByRole("tab", { name: new RegExp(`^${category}`) }));
}

test("issues_group_appears_only_once_a_crawl_exists", async () => {
  render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  expect(screen.queryByRole("list", { name: "Issues" })).toBeNull();
});

test("issues_group_shows_priorities_with_counts", async () => {
  await crawled();
  const high = await within(issuesNav()).findByRole("button", { name: /^High/ });
  expect(high.getAttribute("aria-expanded")).toBe("true");
  await waitFor(() => expect(within(issuesNav()).getByRole("button", { name: /Internal client error/ }).textContent).toContain("3"));
  const medium = within(issuesNav()).getByRole("button", { name: /^Medium/ });
  expect(medium.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(medium);
  expect(medium.getAttribute("aria-expanded")).toBe("true");
  expect(within(issuesNav()).getByRole("button", { name: /Page title: below 30 characters/ }).textContent).toContain("27");
});

test("selecting_an_issue_lists_its_urls_with_why_and_how_to_fix", async () => {
  await crawled();
  fireEvent.click(await within(issuesNav()).findByRole("button", { name: /Internal client error/ }));
  await waitFor(() => expect(rowCount()).toContain("3 URLs"));
  const header = screen.getByTestId("issue-header");
  expect(header.textContent).toContain("High");
  expect(header.textContent).toContain("reach an error page");
  expect(header.textContent).toContain("Restore the page");
  // The explanation folds away.
  fireEvent.click(within(header).getByRole("button", { name: "Why and how to fix" }));
  expect(header.textContent).not.toContain("Restore the page");
  fireEvent.click(within(header).getByRole("button", { name: "Why and how to fix" }));
  expect(rows().map((r) => within(r).getAllByRole("gridcell")[0].textContent)).toEqual([
    "https://example.com/page-9",
    "https://example.com/page-19",
    "https://example.com/page-29",
  ]);
});

test("inspector_lists_the_urls_issues_and_a_click_opens_one", async () => {
  await crawled();
  fireEvent.click(screen.getByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(1));
  fireEvent.click(rows()[1]);
  const issues = await screen.findByRole("heading", { name: "Issues (1)" });
  const section = issues.closest("section")!;
  expect(section.textContent).toContain("Medium");
  expect(section.textContent).toContain("6 characters");
  fireEvent.click(within(section).getByRole("button", { name: /Page title: below 30 characters/ }));
  await waitFor(() => expect(screen.getByTestId("issue-header")).toBeTruthy());
  await waitFor(() => expect(rowCount()).toContain("27 URLs"));
});

test("a_threshold_change_applies_to_counts_and_rows", async () => {
  const t = await crawled();
  await openSettings("Issues");
  const threshold = (await screen.findByLabelText("Page title: below 30 characters threshold")) as HTMLInputElement;
  fireEvent.change(threshold, { target: { value: "7" } });
  await waitFor(() => expect(t.issues.settings.rules["title.below"]?.threshold).toBe(7));
  // "Page 0" to "Page 8" have 6-character titles.
  fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
  const medium = await within(issuesNav()).findByRole("button", { name: /^Medium/ });
  if (medium.getAttribute("aria-expanded") === "false") fireEvent.click(medium);
  await waitFor(() => expect(within(issuesNav()).getByRole("button", { name: /Page title: below 7 characters/ }).textContent).toContain("9"));
});

test("an_out_of_range_threshold_is_reported_and_not_applied", async () => {
  const t = await crawled();
  await openSettings("Issues");
  const threshold = (await screen.findByLabelText("Page title: below 30 characters threshold")) as HTMLInputElement;
  fireEvent.change(threshold, { target: { value: "5000" } });
  await screen.findByText(/must be between 1 and 1000/);
  expect(t.issues.settings.rules["title.below"]).toBeUndefined();
});

test("a_rule_turned_off_leaves_the_counts_and_reset_brings_it_back", async () => {
  const t = await crawled();
  await openSettings("Issues");
  fireEvent.click(await screen.findByRole("switch", { name: "Internal client error (4xx)" }));
  await waitFor(() => expect(t.issues.settings.rules["response.internal_4xx"]?.enabled).toBe(false));
  const reset = screen.getByRole("button", { name: "Reset to defaults" }) as HTMLButtonElement;
  expect(reset.disabled).toBe(false);
  fireEvent.click(reset);
  await waitFor(() => expect(t.issues.settings.rules).toEqual({}));
});

test("search_narrows_the_rules_in_settings", async () => {
  await crawled();
  await openSettings("Issues");
  fireEvent.change(await screen.findByLabelText("Search rules"), { target: { value: "title" } });
  expect(screen.queryByTestId("rule-response.internal_4xx")).toBeNull();
  expect(screen.getByTestId("rule-title.below")).toBeTruthy();
});

test("overrides_keep_only_values_that_differ_from_the_catalogue", () => {
  const rule = applySettings({ rules: {} }).rules.find((r) => r.id === "title.below")!;
  const off = withOverride({ rules: {} }, rule, { enabled: false });
  expect(off.rules["title.below"]).toEqual({ enabled: false, priority: null, threshold: null });
  expect(withOverride(off, rule, { enabled: true })).toEqual({ rules: {} });
  expect(withOverride({ rules: {} }, rule, { threshold: 30 })).toEqual({ rules: {} });
  expect(withOverride({ rules: {} }, rule, { priority: "high" }).rules["title.below"]?.priority).toBe("high");
});
