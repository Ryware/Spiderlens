// Settings › Storage (spec 019 US4.4, US5.2, US5.3, FR-022, FR-025) and the
// cleanup notices (FR-020, FR-021).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";

const GB = 1024 ** 3;

async function openStorage(t = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: /^Storage/ }));
  await screen.findByText("Remove unused unsaved crawls");
  return t;
}

const panel = () => screen.getByRole("tabpanel", { name: "Storage" });

test("fr025_storage_settings_show_the_defaults", async () => {
  await openStorage();
  expect(panel().textContent).toContain("Saved on this computer, not in the crawl profile");
  expect(within(panel()).getByRole("switch", { name: "Remove unused unsaved crawls" }).getAttribute("aria-checked")).toBe("true");
  expect((within(panel()).getByLabelText("Remove after") as HTMLSelectElement).value).toBe("30");
  expect((within(panel()).getByLabelText("Keep free (GB)") as HTMLInputElement).value).toBe("1");
  expect(panel().textContent).toContain("/data/SpiderLens/Unsaved");
});

test("us4_4_cleanup_age_and_off_are_saved", async () => {
  const t = await openStorage();
  fireEvent.change(within(panel()).getByLabelText("Remove after"), { target: { value: "7" } });
  await waitFor(() => expect(t.files.prefs.cleanup_days).toBe(7));
  fireEvent.click(within(panel()).getByRole("switch", { name: "Remove unused unsaved crawls" }));
  await waitFor(() => expect(t.files.prefs.cleanup_days).toBeNull());
  expect(within(panel()).queryByLabelText("Remove after")).toBeNull();
  fireEvent.click(within(panel()).getByRole("switch", { name: "Remove unused unsaved crawls" }));
  await waitFor(() => expect(t.files.prefs.cleanup_days).toBe(30));
});

test("us5_2_the_safety_reserve_is_saved_in_gb", async () => {
  const t = await openStorage();
  const input = within(panel()).getByLabelText("Keep free (GB)");
  fireEvent.change(input, { target: { value: "5" } });
  fireEvent.blur(input);
  await waitFor(() => expect(t.files.prefs.reserve_bytes).toBe(5 * GB));
  // The start screen's low-space warning uses the new value.
  expect(t.files.storage.reserve_bytes).toBe(5 * GB);
});

test("us5_3_a_reserve_out_of_range_is_refused_with_the_range", async () => {
  const t = await openStorage();
  const input = within(panel()).getByLabelText("Keep free (GB)");
  fireEvent.change(input, { target: { value: "80" } });
  fireEvent.blur(input);
  expect((await within(panel()).findByRole("alert")).textContent).toContain("between 256 MB and 50 GB");
  expect(t.files.prefs.reserve_bytes).toBe(1 * GB);
  fireEvent.change(input, { target: { value: "" } });
  fireEvent.blur(input);
  await waitFor(() => expect(within(panel()).getByRole("alert").textContent).toContain("between 0.25 and 50"));
  fireEvent.change(input, { target: { value: "2" } });
  fireEvent.blur(input);
  await waitFor(() => expect(within(panel()).queryByRole("alert")).toBeNull());
  expect(t.files.prefs.reserve_bytes).toBe(2 * GB);
});

test("fr022_the_cleanup_history_lists_what_was_removed", async () => {
  const t = fakeTransport();
  t.files.history = [
    { at: "2026-10-13T08:00:00Z", path: "/data/SpiderLens/Unsaved/old.spiderlens", bytes: 300 * 1024 ** 2, reason: "auto_age" },
    { at: "2026-10-12T08:00:00Z", path: "/data/SpiderLens/Unsaved/.x.spiderlens.partial", bytes: 1024, reason: "leftover" },
  ];
  await openStorage(t);
  fireEvent.click(within(panel()).getByRole("button", { name: "Show history" }));
  const table = await within(panel()).findByRole("table", { name: "Cleanup history" });
  const rows = within(table).getAllByRole("row");
  expect(rows).toHaveLength(2);
  expect(rows[0].textContent).toContain("old");
  expect(rows[0].textContent).toContain("Unsaved, not used for a while: moved to the Trash");
  expect(rows[0].textContent).toContain("300 MB");
  expect(rows[1].textContent).toContain("Leftover file from a crash");
});

test("the_folder_can_be_shown_in_the_file_manager", async () => {
  const t = await openStorage();
  fireEvent.click(within(panel()).getByRole("button", { name: "Show in folder" }));
  await waitFor(() => expect(t.files.revealed).toEqual(["/data/SpiderLens/Unsaved"]));
});

// ---------------------------------------------------------------------------
// Cleanup notices

async function renderApp(t = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

test("fr021_after_cleanup_a_notice_says_what_was_freed", async () => {
  const t = await renderApp();
  act(() => t.files.cleanupDone({ removed: ["/u/a.spiderlens", "/u/b.spiderlens"], leftovers: 3, freed_bytes: 2 * GB }));
  const notice = (await screen.findByText("Cleaned up unused files")).closest("[role=status]") as HTMLElement;
  expect(notice.textContent).toContain("2 unsaved crawls not used for a while were moved to the Trash.");
  expect(notice.textContent).toContain("3 leftover files from earlier crashes were removed.");
  expect(notice.textContent).toContain("2.0 GB freed.");
  fireEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
  await waitFor(() => expect(screen.queryByText("Cleaned up unused files")).toBeNull());
});

test("fr020_the_one_time_notice_names_the_crawls_and_links_to_the_setting", async () => {
  const t = await renderApp();
  act(() => t.files.cleanupDone({ announced: ["/u/first.spiderlens", "/u/second.spiderlens"] }));
  const notice = (await screen.findByText("Old unsaved crawls will be removed")).closest("[role=status]") as HTMLElement;
  expect(notice.textContent).toContain("These will go the next time it starts");
  expect(within(notice).getByText("first")).toBeTruthy();
  expect(within(notice).getByText("second")).toBeTruthy();
  fireEvent.click(within(notice).getByRole("button", { name: "Cleanup settings…" }));
  expect(await screen.findByRole("tabpanel", { name: "Storage" })).toBeTruthy();
});

test("the_cleanup_notice_opens_manage_crawls", async () => {
  const t = await renderApp();
  act(() => t.files.cleanupDone({ removed: ["/u/a.spiderlens"], freed_bytes: GB }));
  const notice = (await screen.findByText("Cleaned up unused files")).closest("[role=status]") as HTMLElement;
  expect(notice.textContent).toContain("1 unsaved crawl not used for a while was moved to the Trash.");
  fireEvent.click(within(notice).getByRole("button", { name: "Manage crawls…" }));
  expect(await screen.findByRole("dialog", { name: "Manage crawls" })).toBeTruthy();
  expect(await screen.findByText("No crawls here.")).toBeTruthy();
});

test("a_cleanup_notice_counts_scheduled_runs_moved_to_the_trash", async () => {
  const t = await renderApp();
  act(() => t.files.cleanupDone({ schedule_removed: ["/s/a.spiderlens", "/s/b.spiderlens", "/s/c.spiderlens"], freed_bytes: GB }));
  const notice = (await screen.findByText("Cleaned up unused files")).closest("[role=status]") as HTMLElement;
  expect(notice.textContent).toContain("3 older scheduled runs were moved to the Trash.");
  fireEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
  await waitFor(() => expect(screen.queryByText("Cleaned up unused files")).toBeNull());
  act(() => t.files.cleanupDone({ schedule_removed: ["/s/a.spiderlens"], freed_bytes: GB }));
  const one = (await screen.findByText("Cleaned up unused files")).closest("[role=status]") as HTMLElement;
  expect(one.textContent).toContain("1 older scheduled run was moved to the Trash.");
});

test("the_history_labels_a_scheduled_run_removal", async () => {
  const t = fakeTransport();
  t.files.history.unshift({ at: new Date().toISOString(), path: "/s/a.spiderlens", bytes: 10, reason: "schedule_retention" });
  await openStorage(t);
  expect(panel().textContent).toContain("Scheduled run beyond its keep-last limit: moved to the Trash");
});
