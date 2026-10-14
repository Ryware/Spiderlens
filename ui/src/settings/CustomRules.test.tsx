// Custom extraction and search settings (spec 013): adding, editing and
// removing rules, the engine's issues in the rule rows, testing on a URL,
// rule files, and running the rules again on the open crawl. The rules
// themselves are tested in Rust (crates/engine/src/custom).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

type T = ReturnType<typeof fakeTransport>;

const nav = () => screen.queryByRole("navigation", { name: "Settings categories" });
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const button = (name: string | RegExp) => screen.getByRole("button", { name }) as HTMLButtonElement;

async function openRules(t: T = fakeTransport()) {
  if (!screen.queryByLabelText("Start URL")) {
    render(<App transport={t} />);
    await screen.findByLabelText("Start URL");
  }
  let n = nav();
  if (!n) {
    fireEvent.click(screen.getByRole("button", { name: "Configure" }));
    n = await screen.findByRole("navigation", { name: "Settings categories" });
  }
  fireEvent.click(within(n).getByRole("tab", { name: /^Custom rules/ }));
  await screen.findByRole("tabpanel", { name: "Custom rules" });
  return t;
}

async function closeSettings() {
  if (!nav()) return;
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
}

/** Starts a crawl of example.com and finishes it, leaving its file open. */
async function finishedCrawl(t: T) {
  await closeSettings();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await screen.findByRole("button", { name: "Internal" });
}

/** Adds an extractor and a search with names and terms, as a user types them. */
async function addBoth() {
  fireEvent.click(button(/Add extractor/));
  fireEvent.change(await screen.findByLabelText("Extractor 1 name"), { target: { value: "Price" } });
  fireEvent.change(input("Extractor 1 expression"), { target: { value: ".price" } });
  fireEvent.click(button(/Add search/));
  fireEvent.change(await screen.findByLabelText("Search 1 name"), { target: { value: "Consent" } });
  fireEvent.change(input("Search 1 term"), { target: { value: "consent-v2.js" } });
}

test("rules_typed_in_settings_reach_the_crawl", async () => {
  const t = await openRules();
  await addBoth();
  fireEvent.change(screen.getByLabelText("Extractor 1 type"), { target: { value: "xpath" } });
  fireEvent.change(input("Extractor 1 expression"), { target: { value: "//span[@class='price']" } });
  fireEvent.change(screen.getByLabelText("Search 1 condition"), { target: { value: "not_contains" } });
  fireEvent.click(screen.getByRole("switch", { name: "Search 1 ignore case" }));
  await closeSettings();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  const custom = t.crawls.started[0].config.custom;
  expect(custom.extractors).toEqual([expect.objectContaining({ name: "Price", kind: "xpath", expr: "//span[@class='price']", mode: "text" })]);
  expect(custom.searches).toEqual([expect.objectContaining({ name: "Consent", term: "consent-v2.js", condition: "not_contains", ignore_case: true, target: "html" })]);
});

test("new_rules_get_names_that_are_not_taken", async () => {
  await openRules();
  fireEvent.click(button(/Add extractor/));
  fireEvent.click(button(/Add extractor/));
  fireEvent.click(button(/Add search/));
  expect(input("Extractor 1 name").value).not.toBe(input("Extractor 2 name").value);
  expect(input("Search 1 name").value).toMatch(/^Search/);
});

test("removing_a_rule_keeps_the_others", async () => {
  await openRules();
  fireEvent.click(button(/Add extractor/));
  fireEvent.click(button(/Add extractor/));
  fireEvent.change(input("Extractor 1 name"), { target: { value: "First" } });
  fireEvent.change(input("Extractor 2 name"), { target: { value: "Second" } });
  fireEvent.click(button("Remove extractor 1"));
  expect(input("Extractor 1 name").value).toBe("Second");
  expect(screen.queryByLabelText("Extractor 2 name")).toBeNull();
});

test("engine_issues_show_in_the_rule_and_the_category", async () => {
  // A rule with nothing to extract is an error in its row, and the category counts it.
  await openRules();
  fireEvent.click(button(/Add extractor/));
  const row = within(screen.getByRole("list", { name: "Extractors" })).getAllByRole("listitem")[0];
  await within(row).findByText("Enter what to extract.");
  await waitFor(() => expect(within(nav()!).getByRole("tab", { name: /^Custom rules/ }).textContent).toContain("1 error"));
  fireEvent.change(input("Extractor 1 expression"), { target: { value: "h1" } });
  await waitFor(() => expect(within(row).queryByText("Enter what to extract.")).toBeNull());
});

test("xpath_shows_what_it_returns_and_json_ld_its_item_type", async () => {
  await openRules();
  fireEvent.click(button(/Add extractor/));
  expect(screen.getByLabelText("Extractor 1 returns")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Extractor 1 type"), { target: { value: "json_ld" } });
  expect(await screen.findByLabelText("Extractor 1 item type")).toBeTruthy();
});

test("search_can_be_limited_to_one_element", async () => {
  const t = await openRules();
  fireEvent.click(button(/Add search/));
  fireEvent.change(input("Search 1 term"), { target: { value: "Sold out" } });
  fireEvent.change(screen.getByLabelText("Search 1 only inside"), { target: { value: "css" } });
  fireEvent.change(await screen.findByLabelText("Search 1 element"), { target: { value: "main" } });
  await finishedCrawlThenStart(t);
  expect(t.crawls.started[0].config.custom.searches[0].within).toEqual({ kind: "css", expr: "main" });
});

async function finishedCrawlThenStart(t: T) {
  await closeSettings();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
}

test("testing_on_a_url_shows_what_each_rule_finds", async () => {
  const t = await openRules();
  expect(button("Test rules").disabled).toBe(true);
  await addBoth();
  fireEvent.change(input("Page to test"), { target: { value: "https://example.com/product" } });
  fireEvent.click(button("Test rules"));
  const table = await screen.findByRole("table", { name: "Test results" });
  const [price, consent] = within(table).getAllByRole("row").slice(1);
  expect(within(price).getAllByRole("cell").map((c) => c.textContent)).toEqual(["Price", "1", "$10"]);
  expect(within(consent).getByText("Meets")).toBeTruthy();
  expect(t.custom.tested[0]).toEqual(expect.objectContaining({ url: "https://example.com/product" }));
  expect(t.custom.tested[0].rules.extractors[0].expr).toBe(".price");
});

test("a_test_url_that_fails_says_why", async () => {
  const t = fakeTransport();
  t.custom.failingUrl = "https://example.com/down";
  await openRules(t);
  await addBoth();
  fireEvent.change(input("Page to test"), { target: { value: "https://example.com/down" } });
  fireEvent.keyDown(input("Page to test"), { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toContain("The server did not answer.");
});

test("rules_import_from_and_export_to_a_file", async () => {
  const t = await openRules();
  expect(button(/Export…/).disabled).toBe(true);
  fireEvent.click(button(/Import…/));
  await waitFor(() => expect(input("Extractor 1 name").value).toBe("Imported price"));
  expect(screen.getByRole("status").textContent).toContain("Added 1 rule from /rules/shop.spiderlens-rules.json");
  fireEvent.click(button(/Export…/));
  await waitFor(() => expect(t.custom.exportedTo).toHaveLength(1));
  expect(t.custom.exportedTo[0].path).toBe("/rules/custom-rules.spiderlens-rules.json");
  expect(t.custom.exportedTo[0].rules.extractors[0].name).toBe("Imported price");
});

test("run_on_the_open_crawl_needs_a_crawl", async () => {
  await openRules();
  expect(screen.queryByRole("button", { name: /Run on the open crawl/ })).toBeNull();
});

test("run_on_the_open_crawl_shows_progress_stops_and_reports", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  await finishedCrawl(t);
  await openRules(t);
  const run = button(/Run on the open crawl/);
  expect(run.disabled).toBe(true); // no rules yet
  await addBoth();
  await waitFor(() => expect(button(/Run on the open crawl/).disabled).toBe(false));
  fireEvent.click(button(/Run on the open crawl/));
  await waitFor(() => expect(t.custom.runs).toHaveLength(1));
  expect(t.custom.runs[0].path).toBe("/crawls/example.com.spiderlens");
  expect(t.custom.runs[0].rules?.searches[0].term).toBe("consent-v2.js");
  act(() => t.custom.progress({ done: 120, total: 300, per_sec: 900 }));
  expect(await screen.findByText(/120 of 300/)).toBeTruthy();
  fireEvent.click(button("Stop"));
  await waitFor(() => expect(t.custom.cancelled).toEqual([1]));
  act(() => t.custom.done({ state: "partial", pages: 120 }));
  expect(await screen.findByText(/Stopped after 120 pages/)).toBeTruthy();
});

test("while_a_crawl_runs_changes_are_for_the_next_one", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  await openRules(t);
  expect(screen.getByText(/Changes apply to the next crawl/)).toBeTruthy();
});
