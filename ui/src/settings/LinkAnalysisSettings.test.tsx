// Settings › Link analysis (spec 023 US4, plan D9): the rules editor, the
// rules tester, weighting, and the open crawl file picking up changes.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

type T = ReturnType<typeof fakeTransport>;

const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const sw = (name: string) => screen.getByRole("switch", { name });
const value = (label: string) => (screen.getByLabelText(label) as HTMLInputElement).value;

async function renderApp(t: T = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

async function openLinkSettings() {
  fireEvent.click(button("Configure"));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: /^Link analysis/ }));
  return screen.getByRole("tabpanel", { name: "Link analysis" });
}

async function crawlAndFinish(t: T) {
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
}

test("rules_are_added_reordered_removed_and_reach_the_crawl", async () => {
  const t = await renderApp();
  await openLinkSettings();
  expect(sw("Work out Link Score when a crawl ends").getAttribute("aria-checked")).toBe("true");
  fireEvent.click(button(/Add rule/));
  fireEvent.click(button(/Add rule/));
  fireEvent.change(screen.getByLabelText("Rule 1 selector"), { target: { value: ".mega-menu" } });
  fireEvent.change(screen.getByLabelText("Rule 2 selector"), { target: { value: "#related" } });
  fireEvent.change(screen.getByLabelText("Rule 2 position"), { target: { value: "content" } });
  expect(button("Move rule 1 up").disabled).toBe(true);
  fireEvent.click(button("Move rule 2 up"));
  expect(value("Rule 1 selector")).toBe("#related");
  fireEvent.click(button(/Add rule/));
  fireEvent.change(screen.getByLabelText("Rule 3 selector"), { target: { value: "footer .x" } });
  fireEvent.click(button("Remove rule 3"));
  expect(screen.queryByLabelText("Rule 3 selector")).toBeNull();

  fireEvent.click(button(/Done/));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.links.position_rules).toEqual([
    { selector: "#related", position: "content" },
    { selector: ".mega-menu", position: "navigation" },
  ]);
});

test("an_invalid_selector_is_shown_on_its_rule_and_on_the_category", async () => {
  await renderApp();
  await openLinkSettings();
  fireEvent.click(button(/Add rule/));
  fireEvent.change(screen.getByLabelText("Rule 1 selector"), { target: { value: "div[" } });
  expect((await screen.findByRole("alert")).textContent).toContain("Not a valid CSS selector.");
  const nav = screen.getByRole("navigation", { name: "Settings categories" });
  expect(within(nav).getByRole("tab", { name: /^Link analysis/ }).textContent).toContain("1 error");
  fireEvent.click(button(/Done/));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await new Promise((r) => setTimeout(r, 50));
  expect(button(/Start$/).disabled).toBe(true);
});

test("weighting_shows_the_default_weights_and_resets_them", async () => {
  const t = await renderApp();
  await openLinkSettings();
  expect(screen.queryByLabelText("Footer")).toBeNull();
  fireEvent.click(sw("Weight Link Score by link position"));
  expect(value("Footer")).toBe("0.2");
  expect(value("Content")).toBe("1");
  fireEvent.change(screen.getByLabelText("Footer"), { target: { value: "0.1" } });
  expect(value("Footer")).toBe("0.1");
  fireEvent.click(button("Reset weights"));
  expect(value("Footer")).toBe("0.2");
  fireEvent.change(screen.getByLabelText("Navigation"), { target: { value: "0.5" } });
  fireEvent.click(button(/Done/));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(button(/Start$/).disabled).toBe(false));
  fireEvent.click(button(/Start$/));
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.links.weighting).toEqual({ navigation: 0.5, header: 0.3, content: 1, sidebar: 0.5, footer: 0.2, unknown: 1 });
});

test("the_tester_needs_an_open_crawl_then_shows_each_links_position_and_rule", async () => {
  const t = await renderApp();
  let panel = await openLinkSettings();
  expect(within(panel).getByText(/Open a crawl to try the rules/)).toBeTruthy();
  fireEvent.click(button(/Done/));

  await crawlAndFinish(t);
  panel = await openLinkSettings();
  fireEvent.click(button(/Add rule/));
  fireEvent.change(screen.getByLabelText("Rule 1 selector"), { target: { value: ".mega-menu" } });
  fireEvent.change(within(panel).getByLabelText("Page URL"), { target: { value: "https://example.com/" } });
  fireEvent.click(within(panel).getByRole("button", { name: "Test rules" }));
  const table = (await within(panel).findByText("/promo")).closest("table")!;
  const rows = within(table).getAllByRole("row").slice(1).map((r) => within(r).getAllByRole("cell").map((c) => c.textContent));
  expect(rows).toEqual([
    ["/ Home", "Header", "Built-in rule"],
    ["/promo Promo", "Navigation", "Rule 1"],
    ["/story Read the story", "Content", "Built-in rule"],
  ]);
  fireEvent.change(within(panel).getByLabelText("Page URL"), { target: { value: "https://example.com/missing" } });
  fireEvent.click(within(panel).getByRole("button", { name: "Test rules" }));
  expect(await within(panel).findByText(/not in this crawl, or its HTML was not stored/)).toBeTruthy();
});

test("changed_settings_apply_to_the_open_crawl_but_opening_does_not_override_it", async () => {
  const t = await renderApp();
  await crawlAndFinish(t);
  await new Promise((r) => setTimeout(r, 20));
  expect(t.results.linkSettings).toHaveLength(0);
  await openLinkSettings();
  fireEvent.click(sw("Weight Link Score by link position"));
  await waitFor(() => expect(t.results.linkSettings).toHaveLength(1));
  expect(t.results.linkSettings[0].weighting?.footer).toBe(0.2);
});
