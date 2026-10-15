// The crawl report screen and its exports (spec 010 US1 to US4). The report
// itself is built and tested in Rust (crates/results/tests/report.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { sampleReport } from "../testing/fakeAudit";

async function crawled(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

async function openReport(t = fakeTransport()) {
  await crawled(t);
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "View report" }));
  await screen.findByTestId("score");
  return t;
}

test("report_is_off_until_a_crawl_exists", async () => {
  render(<App transport={fakeTransport()} />);
  await screen.findByLabelText("Start URL");
  expect(screen.getByRole("button", { name: "Report" }).getAttribute("aria-disabled")).toBe("true");
});

test("finished_crawl_offers_the_report_with_score_and_sections", async () => {
  const t = await openReport();
  expect(screen.getByTestId("score").textContent).toBe("72");
  const card = screen.getByRole("region", { name: "Health score" });
  expect(within(card).getByText("Needs work")).toBeTruthy();
  expect(within(card).getByText("−20")).toBeTruthy();
  expect(within(card).getByRole("button", { name: "Missing page title" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Site structure" })).toBeTruthy();
  expect(screen.getByText("This crawl did not store page content.")).toBeTruthy();
  expect(t.audit.asked.at(-1)).toEqual({ path: "/crawls/example.com.spiderlens", running: false });
});

test("a_figure_opens_the_matching_rows", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("button", { name: "Client errors (4xx): 2" }));
  await waitFor(() => expect(t.results.opened.at(-1)).toMatchObject({ tab: "response-codes", filter: "client_error" }));
  expect(screen.getByRole("button", { name: "Response codes" }).getAttribute("aria-current")).toBe("page");
});

test("an_issue_explains_why_and_how_to_fix", async () => {
  await openReport();
  const issue = document.querySelector("[data-issue=title_missing]") as HTMLElement;
  expect(within(issue).getByText("Search results show the title as the link text.")).toBeTruthy();
  expect(within(issue).getByText("Give every page a unique title.")).toBeTruthy();
  expect(within(issue).getByText("High")).toBeTruthy();
});

test("running_crawl_shows_a_provisional_report_and_no_bulk_export", async () => {
  const t = await crawled();
  t.audit.report = sampleReport({ provisional: true });
  fireEvent.click(screen.getByRole("button", { name: "Report" }));
  await screen.findByTestId("score");
  expect(screen.getAllByText("Provisional").length).toBeGreaterThan(0);
  expect(t.audit.asked.at(-1)?.running).toBe(true);
  expect((screen.getByRole("button", { name: /Bulk export/ }) as HTMLButtonElement).disabled).toBe(true);
});

test("incomplete_crawl_says_what_the_report_leaves_out", async () => {
  const t = fakeTransport();
  t.audit.report = sampleReport({ incomplete: { not_crawled: 40, reasons: [{ label: "Over URL limit", count: 40 }] } });
  await openReport(t);
  expect(screen.getByText(/40 URLs were not crawled \(Over URL limit: 40\)/)).toBeTruthy();
});

test("report_opens_by_itself_when_the_setting_is_on", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("switch", { name: "Show the report when a crawl finishes" }));
  fireEvent.click(screen.getByRole("button", { name: "Overview" }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.org" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  expect(screen.queryByTestId("score")).toBeNull();
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await screen.findByTestId("score");
});

test("save_report_writes_html_with_title_and_asks_before_replacing", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("button", { name: /Save report/ }));
  const dialog = await screen.findByRole("dialog", { name: "Save report" });
  fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Acme audit" } });
  t.audit.existing.add("/exports/example.com-report.html");
  fireEvent.click(within(dialog).getByRole("button", { name: "Save HTML…" }));
  await within(dialog).findByText(/already exists/);
  expect(t.audit.written).toEqual([]);
  fireEvent.click(within(dialog).getByRole("button", { name: "Replace it" }));
  await within(dialog).findByText("Saved to /exports/example.com-report.html");
  expect(t.audit.written).toEqual([{ dest: "/exports/example.com-report.html", file: { title: "Acme audit", note: "", urls_per_issue: 100 }, overwrite: true }]);
});

test("print_report_and_urls_per_issue_limit", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("button", { name: /Save report/ }));
  const dialog = await screen.findByRole("dialog", { name: "Save report" });
  const urls = within(dialog).getByLabelText("URLs listed per issue");
  fireEvent.change(urls, { target: { value: "20000" } });
  expect((within(dialog).getByRole("button", { name: "Print or PDF…" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(urls, { target: { value: "25" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Print or PDF…" }));
  await waitFor(() => expect(t.audit.printed).toEqual([{ title: "", note: "", urls_per_issue: 25 }]));
});

test("bulk_export_shows_progress_and_a_summary", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("button", { name: /Bulk export/ }));
  const dialog = await screen.findByRole("dialog", { name: "Bulk export" });
  fireEvent.click(within(dialog).getByLabelText("Response headers"));
  fireEvent.click(within(dialog).getByRole("radio", { name: "CSV folder" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Export…" }));
  await waitFor(() => expect(t.audit.bulk).toHaveLength(1));
  expect(t.audit.bulk[0]).toEqual({
    tables: ["summary", "tabs", "issues", "links", "images", "redirect_chains"],
    format: "csv",
    out: "/exports/example.com-export",
    overwrite: false,
  });
  act(() => t.audit.progress({ table: "All links", tables_done: 3, tables_total: 12, rows: 20000 }));
  expect(within(dialog).getByText("All links: 20,000 rows")).toBeTruthy();
  act(() => t.audit.finishBulk({ tables: [{ table: "Internal", rows: 30 }, { table: "All links", rows: 70 }], split_sheets: 1 }));
  await within(dialog).findByText(/Exported 100 rows in 2 tables/);
  expect(within(dialog).getByText(/continue on extra sheets/)).toBeTruthy();
});

test("bulk_export_can_be_cancelled", async () => {
  const t = await openReport();
  fireEvent.click(screen.getByRole("button", { name: /Bulk export/ }));
  const dialog = await screen.findByRole("dialog", { name: "Bulk export" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Export…" }));
  fireEvent.click(await within(dialog).findByRole("button", { name: "Cancel export" }));
  await within(dialog).findByText("Export cancelled; nothing was written.");
  expect(t.audit.bulk[0].format).toBe("xlsx");
});
