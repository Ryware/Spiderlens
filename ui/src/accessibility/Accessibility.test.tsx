// Spec 026 in the window: the Accessibility and Mobile sections of the
// details pane, the status bar figures, and "Run accessibility checks".
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { Inspector } from "../results/Inspector";
import { fakeA11y, fakeMobile, FAKE_A11Y_DISCLAIMER } from "../testing/fakeResults";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

const PATH = "/crawls/example.com.spiderlens";
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const status = () => screen.getByRole("contentinfo", { name: "Crawl status" });
const figure = (label: string) => status().querySelector(`[data-figure="${label}"]`)?.textContent;

const section = async (name: RegExp) => {
  const details = screen.getByRole("complementary", { name: "Details" });
  return (await within(details).findByRole("heading", { name })).closest("section") as HTMLElement;
};

function withPage(a11y = fakeA11y(), mobile = fakeMobile()) {
  const t = fakeTransport();
  t.results.urls[2].a11y = a11y;
  t.results.urls[2].mobile = mobile;
  return { t, id: t.results.urls[2].id };
}

test("accessibility_section_shows_impact_labels_wcag_and_disclaimer", async () => {
  const { t, id } = withPage();
  render(<Inspector transport={t} path={PATH} id={id} />);
  const s = within(await section(/^Accessibility \(2 failed\)/));
  expect(s.getByText("Critical")).toBeTruthy();
  expect(s.getByText("Moderate")).toBeTruthy();
  expect(s.getByText("Minor")).toBeTruthy();
  expect(s.getByText("Text has too little contrast")).toBeTruthy();
  expect(s.getByText("Needs review")).toBeTruthy();
  expect(s.getByText("Best practice", { exact: false })).toBeTruthy();
  // One link for each check that has a help page (the static check has none).
  const fixes = s.getAllByRole("link", { name: "How to fix" });
  expect(fixes).toHaveLength(2);
  for (const a of fixes) expect(a.getAttribute("href")).toContain("dequeuniversity.com");
  expect(s.getByText(FAKE_A11Y_DISCLAIMER)).toBeTruthy();
});

test("accessibility_section_says_when_only_some_elements_are_stored", async () => {
  const a11y = fakeA11y();
  a11y.checks[0].nodes_total = 50;
  a11y.capped = true;
  const { t, id } = withPage(a11y);
  render(<Inspector transport={t} path={PATH} id={id} />);
  const s = within(await section(/^Accessibility/));
  expect(s.getByText("Showing the first 2 elements of 50")).toBeTruthy();
});

test("accessibility_section_with_no_checks_says_nothing_was_found", async () => {
  const { t, id } = withPage(fakeA11y({ checks: [] }));
  render(<Inspector transport={t} path={PATH} id={id} />);
  const s = within(await section(/^Accessibility \(no failures\)/));
  expect(s.getByText("No problems found by the checks that ran.")).toBeTruthy();
});

test("full_check_reason_is_shown", async () => {
  const { t, id } = withPage(fakeA11y({ full_state: "not_checked", full_reason: "rendering is off" }));
  render(<Inspector transport={t} path={PATH} id={id} />);
  expect(within(await section(/^Accessibility/)).getByText("Full checks: rendering is off")).toBeTruthy();
});

test("mobile_section_lists_measurements_and_small_targets", async () => {
  const { t, id } = withPage();
  render(<Inspector transport={t} path={PATH} id={id} />);
  const s = within(await section(/^Mobile$/));
  expect(s.getByText("Small tap targets")).toBeTruthy();
  expect(s.getByText("55% of text")).toBeTruthy();
  expect(s.getByText("480 px")).toBeTruthy();
  expect(s.getByText("nav a.tiny")).toBeTruthy();
});

test("no_sections_when_the_page_has_no_accessibility_data", async () => {
  const t = fakeTransport();
  render(<Inspector transport={t} path={PATH} id={t.results.urls[2].id} />);
  const details = screen.getByRole("complementary", { name: "Details" });
  await waitFor(() => expect(details.textContent).toContain(t.results.urls[2].url));
  expect(screen.queryByRole("heading", { name: /^Accessibility/ })).toBeNull();
  expect(screen.queryByRole("heading", { name: /^Mobile$/ })).toBeNull();
});

async function startCrawl(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

test("status_bar_shows_checked_pages_and_average_time", async () => {
  const t = await startCrawl();
  act(() => t.crawls.progress({ crawled: 20, a11y_checked: 12, a11y_avg_ms: null }));
  expect(figure("accessibility")).toBe("Accessibility: 12 checked");
  act(() => t.crawls.progress({ crawled: 30, a11y_checked: 25, a11y_avg_ms: 340 }));
  expect(figure("accessibility")).toBe("Accessibility: 25 checked · avg 340 ms");
});

test("status_bar_hides_the_figure_when_accessibility_is_off", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: /^Accessibility/ }));
  fireEvent.click(screen.getByRole("radio", { name: "Off" }));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.progress({ crawled: 20, a11y_checked: 12 }));
  expect(figure("accessibility")).toBeUndefined();
});

const palette = async (query: string) => {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  fireEvent.keyDown(input, { key: "Enter" });
};

test("run_accessibility_checks_confirms_then_shows_the_summary", async () => {
  const t = await startCrawl();
  t.crawls.a11ySummary = { pages: 40, with_findings: 12, skipped_no_html: 3 };
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await waitFor(() => expect(screen.queryByRole("button", { name: /Pause/ })).toBeNull());
  await palette("Run accessibility checks");
  const dialog = await screen.findByRole("dialog", { name: "Run accessibility checks" });
  expect(within(dialog).getByText("Re-run the static accessibility checks on this crawl's stored HTML? Earlier static results are replaced.")).toBeTruthy();
  expect(t.crawls.a11yRuns).toHaveLength(0);
  fireEvent.click(within(dialog).getByRole("button", { name: "Run checks" }));
  expect(await within(dialog).findByText("Checked 40 pages, 12 with findings, 3 without stored HTML")).toBeTruthy();
  expect(t.crawls.a11yRuns).toEqual([PATH]);
});

test("cancelling_the_confirmation_runs_nothing", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await waitFor(() => expect(screen.queryByRole("button", { name: /Pause/ })).toBeNull());
  await palette("Run accessibility checks");
  const dialog = await screen.findByRole("dialog", { name: "Run accessibility checks" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Run accessibility checks" })).toBeNull());
  expect(t.crawls.a11yRuns).toHaveLength(0);
});
