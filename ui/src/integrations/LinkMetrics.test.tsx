// Link metrics (spec 024 US5): saving a key, the cost in the provider's own
// units shown before anything is sent, the cap, and the stop at the cap.
// The providers are tested in Rust (crates/integrations/tests/links.rs).
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
  await screen.findByRole("heading", { name: "Link metrics for this crawl" });
  return t;
}

const keyed = (t: ReturnType<typeof fakeTransport>, kind: string) => {
  t.integrations.connections = [{ kind, label: "Agency", scopes: [], created_at: "", store: "keychain" }];
  t.integrations.settings[kind as "ahrefs"] = { enabled: true, per_minute: null, concurrency: null };
};

test("us5_a_key_is_saved_to_the_keychain_and_the_provider_turned_on", async () => {
  const t = await openIntegrations();
  const form = screen.getByRole("form", { name: "Ahrefs key" });
  const save = within(form).getByRole("button", { name: "Save to keychain" }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.change(within(form).getByLabelText("Name for this key"), { target: { value: "Agency" } });
  fireEvent.change(within(form).getByLabelText("API key"), { target: { value: "k-1" } });
  fireEvent.click(save);
  // Off until turned on; the key is never shown again.
  fireEvent.click(await screen.findByRole("button", { name: "Turn on Ahrefs" }));
  await screen.findByRole("button", { name: "Estimate cost" });
  expect(t.integrations.connections[0]).toMatchObject({ kind: "ahrefs", label: "Agency" });
  expect(document.body.textContent).not.toContain("k-1");
});

test("us5_1_cost_is_shown_in_provider_units_and_nothing_starts_without_a_go_ahead", async () => {
  const t = fakeTransport();
  keyed(t, "ahrefs");
  await openIntegrations(t);
  fireEvent.click(await screen.findByRole("button", { name: "Estimate cost" }));
  const dialog = await screen.findByRole("dialog", { name: "Confirm link metrics run" });
  // 120 pages at 4 units each; the default cap is the estimate plus 10%.
  expect(dialog.textContent).toContain("480 API units");
  expect((within(dialog).getByLabelText("Cap in API units") as HTMLInputElement).value).toBe("528");
  expect(t.linkMetrics.runs).toHaveLength(0);

  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(t.linkMetrics.runs).toHaveLength(0);

  fireEvent.click(screen.getByRole("button", { name: "Estimate cost" }));
  fireEvent.click(await screen.findByRole("button", { name: "Start run" }));
  await waitFor(() => expect(t.linkMetrics.runs).toHaveLength(1));
  expect(t.linkMetrics.runs[0]).toMatchObject({ provider: "ahrefs", url_set: "indexable", cap: 528 });
  await screen.findByText(/120 of 120 pages fetched/);
});

test("fr_029_a_run_stops_at_the_cap_the_person_set", async () => {
  const t = fakeTransport();
  keyed(t, "majestic");
  await openIntegrations(t);
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "majestic" } });
  fireEvent.click(await screen.findByRole("button", { name: "Estimate cost" }));
  const dialog = await screen.findByRole("dialog", { name: "Confirm link metrics run" });
  fireEvent.change(within(dialog).getByLabelText("Cap in units"), { target: { value: "100" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Start run" }));
  // 100 units at 4 per page: 25 of 120 pages.
  await screen.findByText(/25 of 120 pages fetched/);
  expect(screen.getByText(/Stopped at the cap of 100 units/)).toBeTruthy();
  expect(t.linkMetrics.runs[0].cap).toBe(100);
});

test("us5_chosen_metrics_and_page_set_are_what_is_asked_for", async () => {
  const t = fakeTransport();
  keyed(t, "moz");
  await openIntegrations(t);
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "moz" } });
  const spam = await screen.findByRole("checkbox", { name: "Spam Score" });
  fireEvent.click(spam);
  fireEvent.change(screen.getByLabelText("Pages"), { target: { value: "all" } });
  fireEvent.click(screen.getByRole("button", { name: "Estimate cost" }));
  await screen.findByRole("dialog", { name: "Confirm link metrics run" });
  expect(t.linkMetrics.estimates[0].url_set).toBe("all");
  expect(t.linkMetrics.estimates[0].metrics).toEqual(["page_authority", "domain_authority", "linking_domains", "backlinks"]);
});
