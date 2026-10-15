// Scheduled crawls in the app (spec 021 US1, US2, US5): the editor, the list,
// the detail with its history, and the actions on a schedule.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
import { fakeRun } from "../testing/fakeSchedules";

type Fake = ReturnType<typeof fakeTransport>;

async function renderApp(t: Fake = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

const sidebar = () => screen.getByRole("navigation", { name: "Sections" });

async function goToSchedules() {
  fireEvent.click(within(sidebar()).getByRole("button", { name: "Schedules" }));
  await screen.findByRole("heading", { name: "Schedules", level: 1 });
}

async function palette(query: string) {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  fireEvent.keyDown(input, { key: "Enter" });
}

const editor = () => screen.findByRole("dialog", { name: /schedule/i });
const nextRuns = (root: HTMLElement) => within(root).getByRole("list", { name: "Next runs" });

test("sidebar_opens_schedules_with_an_empty_state", async () => {
  const t = await renderApp();
  await goToSchedules();
  expect(await screen.findByText("No schedules yet")).toBeTruthy();
  // Registrations are checked once when the app starts (FR-025).
  expect(t.schedules.repairs).toBe(1);
});

test("schedule_this_crawl_from_the_palette_takes_five_interactions_or_fewer", async () => {
  const t = fakeTransport();
  await renderApp(t);
  // SC-006: 1 open the palette, 2 type, 3 Enter, 4 Create. The start URL was already typed.
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "https://example.com/" } });
  await palette("Schedule this crawl");
  const sheet = await editor();
  expect((within(sheet).getByLabelText("Name") as HTMLInputElement).value).toBe("example.com weekly");
  expect((within(sheet).getByLabelText("Start URL") as HTMLInputElement).value).toBe("https://example.com/");
  await waitFor(() => expect(within(nextRuns(sheet)).getAllByRole("listitem")).toHaveLength(5));
  const create = within(sheet).getByRole("button", { name: "Create schedule" });
  await waitFor(() => expect((create as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(create);

  await waitFor(() => expect(t.schedules.stored.size).toBe(1));
  const s = [...t.schedules.stored.values()][0];
  expect(s).toMatchObject({
    name: "example.com weekly",
    enabled: true,
    input: { spider: { start_url: "https://example.com/" } },
    frequency: { kind: "weekly", days: [1], time: { hour: 6, minute: 0 } },
    output_dir: "/Users/me/Documents/SpiderLens schedules",
    catch_up: true,
    notify: "problems",
    source_profile: null,
  });
  // The list shows it, with its next run.
  const list = await screen.findByRole("list", { name: "Schedules" });
  await waitFor(() => expect(within(list).getByText("example.com weekly")).toBeTruthy());
  expect(within(list).getByText(/^Next: /)).toBeTruthy();
});

test("settings_sheet_offers_schedule_this_crawl", async () => {
  const t = fakeTransport();
  await renderApp(t);
  await t.saveProfile("Shop", (await t.defaults(false)), "https://shop.example/", false);
  await palette("Crawl settings");
  fireEvent.click(await screen.findByRole("button", { name: "Schedule this crawl…" }));
  const sheet = await editor();
  expect(within(sheet).getByText(/current crawl settings/)).toBeTruthy();
});

test("frequency_presets_show_the_next_five_runs_and_refuse_more_than_hourly", async () => {
  const t = fakeTransport();
  await renderApp(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "https://example.com/" } });
  await palette("Schedule this crawl");
  const sheet = await editor();

  fireEvent.click(within(sheet).getByRole("radio", { name: "Daily" }));
  fireEvent.change(within(sheet).getByLabelText("At"), { target: { value: "23:15" } });
  await waitFor(() => expect(within(sheet).getByText("Every day at 23:15")).toBeTruthy());
  expect(within(nextRuns(sheet)).getAllByRole("listitem")).toHaveLength(5);

  fireEvent.click(within(sheet).getByRole("radio", { name: "Weekly" }));
  fireEvent.click(within(sheet).getByRole("button", { name: "Friday" }));
  await waitFor(() => expect(within(sheet).getByText("Every Monday, Friday at 23:15")).toBeTruthy());

  fireEvent.click(within(sheet).getByRole("radio", { name: "Monthly" }));
  fireEvent.change(within(sheet).getByLabelText("Day of the month"), { target: { value: "last" } });
  await waitFor(() => expect(within(sheet).getByText("Monthly on the last day at 23:15")).toBeTruthy());

  fireEvent.click(within(sheet).getByRole("radio", { name: "Advanced" }));
  fireEvent.change(within(sheet).getByLabelText("Cron expression"), { target: { value: "*/5 * * * *" } });
  const alert = await within(sheet).findByText(/more often than once an hour/);
  expect(alert.closest("[role=alert]")).toBeTruthy();
  await waitFor(() => expect((within(sheet).getByRole("button", { name: "Create schedule" }) as HTMLButtonElement).disabled).toBe(true));
});

test("a_crawl_with_a_secret_header_cannot_be_scheduled", async () => {
  const t = fakeTransport();
  const base = t.defaults;
  t.defaults = async (b) => {
    const c = await base(b);
    c.identity.headers = [{ name: "Authorization", value: { secret: true }, send_to: "start_host" } as never];
    return c;
  };
  await renderApp(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "https://example.com/" } });
  await palette("Schedule this crawl");
  const sheet = await editor();
  expect(await within(sheet).findByText(/"Authorization" is marked secret/)).toBeTruthy();
  expect((within(sheet).getByRole("button", { name: "Create schedule" }) as HTMLButtonElement).disabled).toBe(true);
});

test("export_tabs_are_saved_as_after_run_steps", async () => {
  const t = fakeTransport();
  await renderApp(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "https://example.com/" } });
  await palette("Schedule this crawl");
  const sheet = await editor();
  const group = await within(sheet).findByRole("group", { name: "Tabs to export" });
  await waitFor(() => expect(within(group).getAllByRole("checkbox").length).toBeGreaterThan(1));
  fireEvent.click(within(group).getAllByRole("checkbox")[0]);
  fireEvent.click(within(sheet).getByRole("radio", { name: "Always" }));
  fireEvent.click(within(sheet).getByRole("button", { name: "Create schedule" }));
  await waitFor(() => expect(t.schedules.stored.size).toBe(1));
  const s = [...t.schedules.stored.values()][0];
  expect(s.after).toEqual([{ kind: "export", tabs: [{ tab: (await t.resultsTabs())[0].id, filter: "all" }] }]);
  expect(s.notify).toBe("always");
});

test("detail_shows_next_runs_and_history_with_reasons_and_opens_a_run", async () => {
  const t = fakeTransport();
  t.schedules.seed({ name: "Blog" }, [
    fakeRun(1),
    fakeRun(2, { status: "failed", reason: "The output folder is not writable.", counts: null, file: null }),
    fakeRun(3, { trigger: "caught_up", steps: [{ kind: "export", status: "failed", outputs: [], reason: "Disk full" }] }),
  ]);
  await renderApp(t);
  await goToSchedules();
  const detail = await screen.findByRole("region", { name: "Blog" });
  expect(within(nextRuns(detail)).getAllByRole("listitem")).toHaveLength(5);
  const history = within(detail).getByRole("table", { name: "Run history" });
  const runs = within(history).getAllByRole("row").slice(1);
  // Newest first.
  expect(runs.map((r) => r.dataset.run)).toEqual(["3", "2", "1"]);
  expect(within(runs[0]).getByText("Caught up")).toBeTruthy();
  expect(within(runs[0]).getByText(/CSV export: failed · Disk full/)).toBeTruthy();
  expect(within(runs[1]).getByText("Failed")).toBeTruthy();
  expect(within(runs[1]).getByText("The output folder is not writable.")).toBeTruthy();
  // The list shows the last run's status.
  const list = screen.getByRole("list", { name: "Schedules" });
  expect(within(list).getByText("Finished")).toBeTruthy();

  fireEvent.click(within(runs[2]).getByRole("button", { name: "Weekly site check 2026-10-12 0600.spiderlens" }));
  await waitFor(() => expect(t.files.opened).toEqual([fakeRun(1).file]));
});

test("turning_a_schedule_off_suspends_it_and_a_refusal_shows_the_reason", async () => {
  const t = fakeTransport();
  const s = t.schedules.seed({ name: "Shop" });
  await renderApp(t);
  await goToSchedules();
  const detail = await screen.findByRole("region", { name: "Shop" });
  fireEvent.click(within(detail).getByRole("switch", { name: "Enabled" }));
  await waitFor(() => expect(t.schedules.stored.get(s.id)!.enabled).toBe(false));
  const list = screen.getByRole("list", { name: "Schedules" });
  await waitFor(() => expect(within(list).getByText("Off")).toBeTruthy());
  expect(within(list).getByText("No upcoming runs")).toBeTruthy();

  t.schedules.refuse = "Access is denied.";
  fireEvent.click(within(detail).getByRole("switch", { name: "Enabled" }));
  expect(await within(detail).findByText(/refused this schedule: Access is denied/)).toBeTruthy();
  await waitFor(() => expect(within(list).getByText("Not registered")).toBeTruthy());
});

test("run_now_then_watch_and_stop_a_running_run", async () => {
  const t = fakeTransport();
  const s = t.schedules.seed({ name: "News" });
  await renderApp(t);
  await goToSchedules();
  let detail = await screen.findByRole("region", { name: "News" });
  fireEvent.click(within(detail).getByRole("button", { name: "Run now" }));
  await waitFor(() => expect(t.schedules.ranNow).toEqual([s.id]));
  expect(await within(detail).findByText(/Started/)).toBeTruthy();

  // The run process reports live counts; the list refreshes.
  t.schedules.running.set(s.id, { run: 1, counts: { discovered: 300, crawled: 120, not_crawled: 0, failed: 2 }, file: "/out/News 2026-10-14 0930.spiderlens" });
  fireEvent.click(within(sidebar()).getByRole("button", { name: "Overview" }));
  await goToSchedules();
  detail = await screen.findByRole("region", { name: "News" });
  expect(await within(detail).findByText(/Run #1 is running: 120 crawled, 300 found, 2 failed/)).toBeTruthy();
  fireEvent.click(within(detail).getByRole("button", { name: "Watch" }));
  await waitFor(() => expect(t.files.opened).toEqual(["/out/News 2026-10-14 0930.spiderlens"]));
  await goToSchedules();
  detail = await screen.findByRole("region", { name: "News" });
  fireEvent.click(within(detail).getByRole("button", { name: "Stop" }));
  await waitFor(() => expect(t.schedules.stopped).toEqual([s.id]));
});

test("delete_asks_first_and_keeps_the_crawl_files", async () => {
  const t = fakeTransport();
  const s = t.schedules.seed({ name: "Old" });
  await renderApp(t);
  await goToSchedules();
  const detail = await screen.findByRole("region", { name: "Old" });
  fireEvent.click(within(detail).getByRole("button", { name: "More actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
  const dialog = await screen.findByRole("dialog", { name: 'Delete "Old"?' });
  expect(within(dialog).getByText(/crawl files and exports stay where they are/)).toBeTruthy();
  fireEvent.click(within(dialog).getByRole("button", { name: "Delete schedule" }));
  await waitFor(() => expect(t.schedules.deleted).toEqual([s.id]));
  expect(await screen.findByText("No schedules yet")).toBeTruthy();
});

test("duplicate_makes_a_copy_that_starts_off", async () => {
  const t = fakeTransport();
  t.schedules.seed({ name: "Main" });
  await renderApp(t);
  await goToSchedules();
  const detail = await screen.findByRole("region", { name: "Main" });
  fireEvent.click(within(detail).getByRole("button", { name: "More actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Duplicate…" }));
  const dialog = await screen.findByRole("dialog", { name: "Duplicate schedule" });
  expect((within(dialog).getByLabelText("Name of the copy") as HTMLInputElement).value).toBe("Main copy");
  fireEvent.click(within(dialog).getByRole("button", { name: "Duplicate" }));
  const list = screen.getByRole("list", { name: "Schedules" });
  await waitFor(() => expect(within(list).getByText("Main copy")).toBeTruthy());
  expect([...t.schedules.stored.values()].find((s) => s.name === "Main copy")!.enabled).toBe(false);
});

test("edit_changes_the_schedule_and_update_from_profile_copies_the_new_settings", async () => {
  const t = fakeTransport();
  const config = await t.defaults(false);
  await t.saveProfile("Shop", { ...config, speed: { ...config.speed, max_concurrent: 2 } }, null, false);
  const s = t.schedules.seed({ name: "Shop", source_profile: "Shop" });
  t.schedules.changedProfiles.add("Shop");
  await renderApp(t);
  await goToSchedules();
  const detail = await screen.findByRole("region", { name: "Shop" });
  expect(within(detail).getByText(/The profile "Shop" changed/)).toBeTruthy();
  fireEvent.click(within(detail).getByRole("button", { name: "Update from profile" }));
  await waitFor(() => expect(t.schedules.stored.get(s.id)!.config.speed.max_concurrent).toBe(2));
  await waitFor(() => expect(within(detail).queryByText(/The profile "Shop" changed/)).toBeNull());

  fireEvent.click(within(detail).getByRole("button", { name: "Edit…" }));
  const sheet = await editor();
  expect(within(sheet).getByText(/copied from the profile "Shop"/)).toBeTruthy();
  fireEvent.change(within(sheet).getByLabelText("Name"), { target: { value: "Shop nightly" } });
  fireEvent.click(within(sheet).getByRole("radio", { name: "Daily" }));
  fireEvent.click(within(sheet).getByRole("button", { name: "Save" }));
  await waitFor(() => expect(t.schedules.stored.get(s.id)!.name).toBe("Shop nightly"));
  expect(t.schedules.stored.get(s.id)!.frequency.kind).toBe("daily");
  expect(await screen.findByRole("region", { name: "Shop nightly" })).toBeTruthy();
});

test("start_up_repair_report_is_shown_once", async () => {
  const t = fakeTransport();
  t.schedules.seed({ name: "Blog" });
  t.schedules.repair = { repaired: ["Blog"], failed: [["Shop", "The task folder is read-only."]], orphans: [] };
  await renderApp(t);
  await goToSchedules();
  expect(await screen.findByText("Updated the system scheduler for: Blog.")).toBeTruthy();
  expect(screen.getByText("Could not register Shop: The task folder is read-only.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  await waitFor(() => expect(screen.queryByText(/Updated the system scheduler/)).toBeNull());
});

test("remove_all_asks_first_and_clears_the_list", async () => {
  const t = fakeTransport();
  t.schedules.seed({ name: "A" });
  t.schedules.seed({ name: "B" });
  await renderApp(t);
  await goToSchedules();
  fireEvent.click(await screen.findByRole("button", { name: "Remove all…" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove all schedules?" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Remove all" }));
  expect(await screen.findByText("No schedules yet")).toBeTruthy();
  expect(t.schedules.stored.size).toBe(0);
});
