// External data (spec 024): connecting Google, turning a source on, what it
// sends, fetching Search Console data for the open crawl, pause, quota
// pause and resume, orphan candidates, removal, and forgetting the account.
// The provider calls themselves are tested in Rust (crates/integrations/tests).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { formatCell } from "../results/format";

async function openIntegrations(t = fakeTransport(), crawl = true) {
  render(<App transport={t} />);
  if (crawl) {
    fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
    await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
    await screen.findByRole("button", { name: /Pause/ });
    act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  } else {
    await screen.findByLabelText("Start URL");
  }
  fireEvent.click(await screen.findByRole("button", { name: "Integrations" }));
  await screen.findByRole("heading", { name: "Integrations" });
  return t;
}

const source = (id: string) => screen.getByRole("list", { name: "Data sources" }).querySelector(`[data-provider="${id}"]`) as HTMLElement;

test("us7_every_source_is_off_and_says_what_it_sends", async () => {
  await openIntegrations(fakeTransport(), false);
  const sc = source("search_console");
  expect(within(sc).getByRole("switch").getAttribute("aria-checked")).toBe("false");
  expect(sc.textContent).toContain("The list of crawled URLs is not sent.");
  // Sources not in this build are shown but cannot be turned on.
  const ahrefs = source("ahrefs");
  expect(within(ahrefs).getByText("Coming later")).toBeTruthy();
  expect(within(ahrefs).getByRole("switch").hasAttribute("data-disabled")).toBe(true);
  expect(screen.getByText(/Open or finish a crawl/)).toBeTruthy();
});

test("fr_014_no_keychain_is_said_plainly", async () => {
  const t = fakeTransport();
  t.integrations.store = "memory_only";
  await openIntegrations(t, false);
  expect(screen.getByText(/kept only until SpiderLens quits/)).toBeTruthy();
});

test("fr_016_own_google_client_when_the_build_has_none", async () => {
  const t = fakeTransport();
  t.integrations.googleClient = false;
  await openIntegrations(t, false);
  expect(screen.queryByRole("button", { name: "Connect Google account" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /Use your own Google client/ }));
  const form = screen.getByRole("form", { name: "Your Google client" });
  fireEvent.change(within(form).getByLabelText("Client ID"), { target: { value: "123.apps.googleusercontent.com" } });
  fireEvent.click(within(form).getByRole("button", { name: "Save to keychain" }));
  await screen.findByRole("button", { name: "Connect Google account" });
  expect(t.integrations.ownClient).toBe("123.apps.googleusercontent.com");
});

test("us1_connect_turn_on_fetch_and_see_orphans", async () => {
  const t = await openIntegrations();
  fireEvent.click(screen.getByRole("button", { name: "Connect Google account" }));
  await screen.findByRole("list", { name: "Google accounts" });
  expect(t.integrations.calls).toContain("connect search_console");

  fireEvent.click(within(source("search_console")).getByRole("switch"));
  const fetch = await screen.findByRole("button", { name: "Fetch Search Console data" });
  // The property covering the crawled site is chosen, with the last 28 days.
  const select = screen.getByLabelText("Property") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("sc-domain:example.com"));
  expect((screen.getByLabelText("From") as HTMLInputElement).value).toBe("2026-09-14");

  t.integrations.orphans = ["https://example.com/old-page", "https://example.com/promo"];
  fireEvent.click(fetch);
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.progress({ done: 25000, requested: 0 }));
  expect(screen.getByRole("status", { name: "Run progress" }).textContent).toContain("25,000 rows read");
  fireEvent.click(screen.getByRole("button", { name: "Pause" }));
  expect(t.integrations.calls).toContain("pause");

  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Runs" });
  await waitFor(() => expect(runs.textContent).toContain("Finished"));
  expect(runs.textContent).toContain("10 matched crawled pages");
  expect(t.integrations.requests[0]).toMatchObject({ property: "sc-domain:example.com", search_type: "web" });
  expect(screen.getByText("2 pages with search traffic were not found by the crawl")).toBeTruthy();
});

test("fr_006_quota_pause_shows_when_to_resume_and_resumes", async () => {
  const t = fakeTransport();
  t.integrations.connections = [{ kind: "google", label: "ana@example.com", scopes: ["search_console"], created_at: "", store: "keychain" }];
  t.integrations.settings.search_console = { enabled: true, per_minute: null, concurrency: null };
  t.integrations.quotaNext = true;
  await openIntegrations(t);
  fireEvent.click(await screen.findByRole("button", { name: "Fetch Search Console data" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Runs" });
  await waitFor(() => expect(runs.textContent).toContain("Paused: daily allowance used"));
  expect(runs.textContent).toContain("Resume after");

  fireEvent.click(screen.getByRole("button", { name: "Resume run 1" }));
  await screen.findByRole("button", { name: "Pause" });
  act(() => t.integrations.finishRun());
  await waitFor(() => expect(screen.getByRole("list", { name: "Runs" }).textContent).toContain("Finished"));
  expect(t.integrations.calls.filter((c) => c.startsWith("resume"))).toHaveLength(1);
});

test("fr_011_remove_data_and_fr_017_forget_the_account", async () => {
  const t = fakeTransport();
  t.integrations.connections = [{ kind: "google", label: "ana@example.com", scopes: ["search_console"], created_at: "", store: "keychain" }];
  t.integrations.settings.search_console = { enabled: true, per_minute: null, concurrency: null };
  t.integrations.runs = [
    { id: 1, provider: "search_console", account_label: "ana@example.com", property: "sc-domain:example.com", params: {}, state: "finished", requested: 0, done: 10, failed: 0, matched: 10, unmatched: 0, usage: null, resume_at: null, message: null, created_at: "", finished_at: "" },
  ];
  await openIntegrations(t);
  fireEvent.click(await screen.findByRole("button", { name: /Remove Search Console data/ }));
  await waitFor(() => expect(screen.queryByRole("list", { name: "Runs" })).toBeNull());
  expect(t.integrations.calls.some((c) => c.startsWith("remove search_console"))).toBe(true);

  fireEvent.click(screen.getByRole("button", { name: "Forget ana@example.com" }));
  await waitFor(() => expect(screen.queryByRole("list", { name: "Google accounts" })).toBeNull());
  expect(t.integrations.calls).toContain("forget google/ana@example.com");
});

test("search_console_values_read_as_percent_and_position", () => {
  expect(formatCell("percent", 0.0345)).toBe("3.5%");
  expect(formatCell("decimal", 4.25)).toBe("4.3");
  expect(formatCell("percent", null)).toBe("");
});
