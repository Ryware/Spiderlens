// The AI checks in the UI (spec 015 US5): the AI search tab's overview with a
// run, its progress and cancel, the details, and the page's section in the
// details pane. The checks themselves are tested in Rust (crates/aeo/tests).
import { act, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { fakePageResult, fakeSummary } from "../testing/fakeAeo";
import { fakeTransport } from "../testing/fakeTransport";
import { AiOverview } from "./AiOverview";
import { Inspector } from "./Inspector";
import { resetAeoRules } from "./useAeo";
import { useResults } from "./useResults";

const PATH = "/crawls/example.com.spiderlens";

beforeEach(resetAeoRules);

const overview = () => screen.getByLabelText("AI checks");

test("offers_a_run_when_the_checks_have_not_run", async () => {
  const t = fakeTransport();
  render(<AiOverview transport={t} path={PATH} live={false} />);
  expect(await screen.findByText("AI checks have not run on this crawl.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Run AI checks" }));
  await waitFor(() => expect(t.aeo.runs).toEqual([PATH]));
});

test("the_run_waits_for_a_live_crawl_to_complete", async () => {
  render(<AiOverview transport={fakeTransport()} path={PATH} live />);
  expect(await screen.findByText("The AI checks run when the crawl completes.")).toBeTruthy();
  expect((screen.getByRole("button", { name: "Run AI checks" }) as HTMLButtonElement).disabled).toBe(true);
});

test("shows_progress_then_the_score_when_the_run_finishes", async () => {
  const t = fakeTransport();
  render(<AiOverview transport={t} path={PATH} live={false} />);
  fireEvent.click(await screen.findByRole("button", { name: "Run AI checks" }));
  await waitFor(() => expect(t.aeo.runs.length).toBe(1));
  act(() => t.aeo.progress({ pass: "pages", done: 120, total: 3000 }));
  expect(screen.getByRole("status", { name: "AI checks" }).textContent).toContain("Checking pages, 120 of 3,000");
  expect(screen.getByRole("meter", { name: "AI checks progress" }).getAttribute("aria-valuenow")).toBe("4");
  act(() => t.aeo.finish());
  expect((await screen.findByTestId("ai-site-score")).textContent).toBe("72");
  const parts = within(screen.getByRole("list", { name: "Score parts" }));
  expect(parts.getByText("Readable without JS")).toBeTruthy();
  expect(parts.getByText("18/25")).toBeTruthy();
  expect(parts.getByText("n/a")).toBeTruthy();
});

test("a_run_started_by_the_app_shows_its_progress_and_can_be_cancelled", async () => {
  const t = fakeTransport();
  render(<AiOverview transport={t} path={PATH} live={false} />);
  await screen.findByText("AI checks have not run on this crawl.");
  // The app starts the checks itself when a crawl completes.
  await act(async () => void (await t.aeoRun(PATH)));
  act(() => t.aeo.progress({ pass: "hosts", done: 1, total: 2 }));
  expect(screen.getByRole("status", { name: "AI checks" }).textContent).toContain("host 1 of 2");
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(t.aeo.cancelled).toEqual([1]));
  act(() => t.aeo.finish({ summary: null }));
  expect(await screen.findByText("Cancelled.")).toBeTruthy();
});

test("progress_for_another_file_is_ignored", async () => {
  const t = fakeTransport();
  render(<AiOverview transport={t} path={PATH} live={false} />);
  await screen.findByText("AI checks have not run on this crawl.");
  await act(async () => void (await t.aeoRun("/crawls/other.spiderlens")));
  act(() => t.aeo.progress({ pass: "site" }));
  expect(screen.queryByRole("status", { name: "AI checks" })).toBeNull();
});

test("a_failed_run_says_why_and_keeps_the_earlier_results", async () => {
  const t = fakeTransport();
  t.aeo.summary = fakeSummary({ score: 41 });
  render(<AiOverview transport={t} path={PATH} live={false} />);
  expect((await screen.findByTestId("ai-site-score")).textContent).toBe("41");
  expect(screen.getByTestId("ai-site-score").className).toContain("text-bad");
  fireEvent.click(screen.getByRole("button", { name: "Run again" }));
  await waitFor(() => expect(t.aeo.runs.length).toBe(1));
  act(() => t.aeo.finish({ summary: null, error: "The crawl file is in use." }));
  expect(await screen.findByText("The crawl file is in use.")).toBeTruthy();
  expect(screen.getByTestId("ai-site-score").textContent).toBe("41");
});

test("details_list_top_fixes_crawler_access_and_llms_txt", async () => {
  const t = fakeTransport();
  t.aeo.summary = fakeSummary();
  render(<AiOverview transport={t} path={PATH} live={false} />);
  fireEvent.click(await screen.findByRole("button", { name: "Details" }));
  const fixes = within(screen.getByRole("region", { name: "Top fixes" }));
  await waitFor(() => expect(fixes.getByText("No author")).toBeTruthy());
  expect(fixes.getByText("+4.2 points")).toBeTruthy();
  expect(fixes.getByText(/12 pages\. Name the author/)).toBeTruthy();
  expect(fixes.getByText(/Site check\. Publish \/llms\.txt/)).toBeTruthy();
  expect(fixes.getByText("Example (Organization)")).toBeTruthy();
  const access = within(screen.getByRole("region", { name: "Crawler access for https://example.com" }));
  const gpt = access.getByText("GPTBot").closest("tr")!;
  expect(within(gpt).getByText("blocked").className).toContain("text-bad");
  expect(gpt.textContent).toContain("30 of 30");
  expect(gpt.textContent).toContain("line 4: Disallow: /");
  expect(access.getByText("Not found")).toBeTruthy();
  expect(overview().textContent).not.toContain("provisional");
});

test("provisional_pages_are_explained", async () => {
  const t = fakeTransport();
  t.aeo.summary = fakeSummary({ provisional_pages: 5 });
  render(<AiOverview transport={t} path={PATH} live={false} />);
  expect(await screen.findByText(/5 of 30 pages have provisional scores/)).toBeTruthy();
});

test("results_read_the_data_list_again_when_the_checks_finish", async () => {
  const t = fakeTransport();
  const { result } = renderHook(() => useResults(t, PATH, false));
  await waitFor(() => expect(result.current.features?.available).toEqual(["base", "analysis"]));
  await t.aeoRun(PATH);
  t.results.features = ["base", "analysis", "ai_checks"];
  act(() => t.aeo.finish());
  await waitFor(() => expect(result.current.features?.available).toContain("ai_checks"));
});

test("details_pane_shows_the_page_checks_with_fixes", async () => {
  const t = fakeTransport();
  t.aeo.pages.set(2, fakePageResult("https://example.com/page-1"));
  render(<Inspector transport={t} path={PATH} id={2} aiChecks />);
  const ai = within(await screen.findByLabelText("AI search checks"));
  expect(ai.getByTestId("ai-page-score").textContent).toBe("64");
  expect(ai.getByText("Article page").getAttribute("title")).toBe("BlogPosting structured data");
  expect(ai.getByText("GPTBot").className).toContain("text-bad");
  const failed = within(ai.getByRole("list", { name: "Failed AI checks" }));
  await waitFor(() => expect(failed.getByText("No author")).toBeTruthy());
  expect(failed.getByText("−5")).toBeTruthy();
  expect(failed.getByText(/Name the author/)).toBeTruthy();
  expect(ai.getByText(/Not checked for lack of data: Structured data has errors/)).toBeTruthy();
});

test("details_pane_says_when_a_page_was_not_checked", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={3} aiChecks />);
  expect(await screen.findByText(/Not checked: the AI checks look at crawled HTML pages only/)).toBeTruthy();
});

test("details_pane_has_no_ai_section_before_the_checks_run", async () => {
  const t = fakeTransport();
  t.aeo.pages.set(2, fakePageResult("https://example.com/page-1"));
  render(<Inspector transport={t} path={PATH} id={2} />);
  await screen.findByRole("heading", { level: 2 });
  expect(screen.queryByLabelText("AI search checks")).toBeNull();
});
