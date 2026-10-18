// URL Inspection and Analytics for the open crawl (spec 024 US2 and US4):
// the count against today's allowance, the multi-day warning, run progress,
// the quota pause and resume, asking for Analytics access, the stream pick,
// the query-string option and orphan candidates. The provider calls are
// tested in Rust (crates/integrations/tests).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

async function openIntegrations(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Integrations" }));
  await screen.findByRole("heading", { name: "Integrations" });
  return t;
}

const google = (scopes: string[]) => [{ kind: "google", label: "ana@example.com", scopes, created_at: "", store: "keychain" as const }];
const on = { enabled: true, per_minute: null, concurrency: null };

test("us2_off_until_turned_on", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console"]);
  await openIntegrations(t);
  expect(screen.getByText(/Turn on URL Inspection above/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Inspect index status" })).toBeNull();
  expect(t.integrations.inspectRequests).toHaveLength(0);
});

test("us2_1_count_against_todays_allowance_and_a_multi_day_warning", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console"]);
  t.integrations.settings.url_inspection = on;
  t.integrations.plan = { selected: 5000, remaining_today: 1500, used_today: 500, days_needed: 3 };
  await openIntegrations(t);

  const allowance = await screen.findByRole("status", { name: "Inspection allowance" });
  expect(allowance.textContent).toContain("5,000 URLs selected");
  expect(allowance.textContent).toContain("1,500 left of 2,000");
  // More URLs than the allowance: told it takes more than one day before starting.
  expect(allowance.textContent).toContain("This takes 3 days at the daily allowance");

  // The property covering the crawled site is chosen; the user caps the set.
  const select = screen.getByLabelText("Property to inspect") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("sc-domain:example.com"));
  fireEvent.change(screen.getByLabelText("At most"), { target: { value: "100" } });
  fireEvent.click(screen.getByRole("button", { name: "Inspect index status" }));
  await screen.findByRole("button", { name: "Pause" });
  expect(t.integrations.inspectRequests[0]).toMatchObject({ property: "sc-domain:example.com", urls: "indexable", max: 100 });

  act(() => t.integrations.progress({ provider: "url_inspection", done: 30, requested: 120 }));
  expect(screen.getByRole("status", { name: "Inspection progress" }).textContent).toContain("30 of 120 URLs inspected");
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Inspection runs" });
  await waitFor(() => expect(runs.textContent).toContain("Finished"));
  expect(runs.textContent).toContain("120 of 120 URLs inspected");
  // Search Console's own list does not show this run.
  expect(screen.queryByRole("list", { name: "Runs" })).toBeNull();
});

test("us2_3_the_daily_allowance_pauses_the_run_and_it_resumes_later", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console"]);
  t.integrations.settings.url_inspection = on;
  t.integrations.quotaNext = true;
  await openIntegrations(t);
  await screen.findByRole("status", { name: "Inspection allowance" });
  fireEvent.click(screen.getByRole("button", { name: "Inspect index status" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Inspection runs" });
  await waitFor(() => expect(runs.textContent).toContain("Paused: allowance used"));
  expect(runs.textContent).toContain("2,000 of 3,000 URLs inspected");
  expect(runs.textContent).toContain("Resume after");
  expect(runs.textContent).toContain("1000 URLs are left");

  fireEvent.click(screen.getByRole("button", { name: "Resume inspection run 1" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  await waitFor(() => expect(screen.getByRole("list", { name: "Inspection runs" }).textContent).toContain("Finished"));
  expect(t.integrations.calls.filter((c) => c.startsWith("resume"))).toHaveLength(1);
});

test("fr_011_inspection_data_can_be_removed_from_the_crawl", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console"]);
  t.integrations.settings.url_inspection = on;
  t.integrations.runs = [
    { id: 1, provider: "url_inspection", account_label: "ana@example.com", property: "sc-domain:example.com", params: {}, state: "finished", requested: 10, done: 10, failed: 0, matched: null, unmatched: null, usage: 10, resume_at: null, message: null, created_at: "", finished_at: "" },
  ];
  await openIntegrations(t);
  fireEvent.click(await screen.findByRole("button", { name: /Remove URL Inspection data/ }));
  await waitFor(() => expect(screen.queryByRole("list", { name: "Inspection runs" })).toBeNull());
  expect(t.integrations.calls.some((c) => c.startsWith("remove url_inspection"))).toBe(true);
});

test("us4_1_analytics_asks_for_its_own_access_then_offers_the_matching_stream", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console"]);
  t.integrations.settings.analytics = on;
  await openIntegrations(t);
  // The Search Console sign-in does not cover Analytics.
  expect(screen.getByText(/Your Search Console sign-in does not include it/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Allow Google Analytics access" }));
  const stream = (await screen.findByLabelText("Data stream")) as HTMLSelectElement;
  expect(t.integrations.calls).toContain("connect analytics");
  // The stream whose domain is the crawled site is pre-selected, with the last 28 days.
  await waitFor(() => expect(stream.value).toBe("properties/123/456"));
  expect(within(stream).getByText(/matches this crawl/)).toBeTruthy();
  expect((screen.getByLabelText("Analytics from") as HTMLInputElement).value).toBe("2026-09-16");
});

test("us4_2_fetch_with_query_strings_kept_apart_lists_orphans", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console", "analytics"]);
  t.integrations.settings.analytics = on;
  t.integrations.analyticsOrphans = ["https://example.com/old-landing"];
  await openIntegrations(t);
  await waitFor(() => expect((screen.getByLabelText("Data stream") as HTMLSelectElement).value).toBe("properties/123/456"));

  // Combined by default; the option keeps them apart.
  const keep = screen.getByLabelText("Keep landing pages that differ by query string apart") as HTMLInputElement;
  expect(keep.checked).toBe(false);
  fireEvent.click(keep);
  fireEvent.click(screen.getByRole("button", { name: "Fetch Analytics data" }));
  await screen.findByRole("button", { name: "Pause" });
  expect(t.integrations.analyticsRequests[0]).toMatchObject({ property: "properties/123", stream: "456", keep_query: true });

  act(() => t.integrations.progress({ provider: "analytics", done: 100, requested: 480 }));
  expect(screen.getByRole("status", { name: "Analytics progress" }).textContent).toContain("100 landing pages read");
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Analytics runs" });
  await waitFor(() => expect(runs.textContent).toContain("Finished"));
  expect(runs.textContent).toContain("480 landing pages, 450 matched crawled pages, 30 not in the crawl");
  expect(screen.getByText("1 landing pages with sessions were not found by the crawl")).toBeTruthy();
});

test("us4_3_the_token_allowance_pauses_the_fetch_and_it_resumes", async () => {
  const t = fakeTransport();
  t.integrations.connections = google(["search_console", "analytics"]);
  t.integrations.settings.analytics = on;
  t.integrations.quotaNext = true;
  await openIntegrations(t);
  await waitFor(() => expect((screen.getByLabelText("Data stream") as HTMLSelectElement).value).toBe("properties/123/456"));
  fireEvent.click(screen.getByRole("button", { name: "Fetch Analytics data" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Analytics runs" });
  await waitFor(() => expect(runs.textContent).toContain("Paused: allowance used"));
  expect(runs.textContent).toContain("hourly token allowance");
  fireEvent.click(screen.getByRole("button", { name: "Resume Analytics run 1" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  await waitFor(() => expect(screen.getByRole("list", { name: "Analytics runs" }).textContent).toContain("Finished"));
});
