// Opening, resuming and saving crawl files (spec 006 US2, US3, US5, US6):
// the four ways in, damaged files, resume checks, crawl information, Save As,
// disk problems and quitting.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { defaultConfig, fakeLocalError, fakeReport, fakeTransport } from "../testing/fakeTransport";
import { localErrorTitle } from "./LocalErrorNotice";

type T = ReturnType<typeof fakeTransport>;
const MB = 1024 ** 2;
const statusText = () => within(screen.getByRole("contentinfo", { name: "Crawl status" })).getByRole("status").textContent;
const dialog = () => screen.findByRole("dialog");

async function renderApp(t: T = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

async function startCrawl(t: T = fakeTransport()) {
  await renderApp(t);
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

/** Opens `path` as the OS would and waits until it shows. */
async function openFromOs(t: T, path: string) {
  act(() => t.files.openFile(path));
  await waitFor(() => expect(t.files.opened).toContain(path));
  await waitFor(() => expect(statusText()).not.toBe("Idle"));
}

const palette = async (query: string) => {
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: query } });
  fireEvent.keyDown(input, { key: "Enter" });
};

// Opening ------------------------------------------------------------------

test("opening_while_a_crawl_runs_asks_to_pause_it_first", async () => {
  const t = await startCrawl();
  t.files.pickPath = "/crawls/other.spiderlens";
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  let d = await dialog();
  expect(within(d).getByText("Pause the running crawl and open other?")).toBeTruthy();
  fireEvent.click(within(d).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(t.crawls.controls).toEqual([]);
  expect(t.files.opened).toEqual([]);

  act(() => t.files.drop(["/crawls/other.spiderlens"]));
  d = await dialog();
  fireEvent.click(within(d).getByRole("button", { name: "Pause and open" }));
  await waitFor(() => expect(t.crawls.controls).toEqual(["pause 1"]));
  // Nothing opens until the crawl has actually paused.
  expect(t.files.opened).toEqual([]);
  act(() => t.crawls.finish({ report: fakeReport("paused", 5) }));
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/other.spiderlens"]));
});

test("a_damaged_file_offers_to_recover_what_can_be_read", async () => {
  const t = await renderApp();
  t.files.damaged.set("/crawls/broken.spiderlens", { message: "The file ends in the middle of a page.", salvageable: true });
  act(() => t.files.openFile("/crawls/broken.spiderlens"));
  const d = await dialog();
  expect(within(d).getByText("This crawl file is damaged")).toBeTruthy();
  expect(within(d).getByText("The file ends in the middle of a page.")).toBeTruthy();
  fireEvent.click(within(d).getByRole("button", { name: "Recover what can be read" }));
  await waitFor(() => expect(t.files.salvaged).toEqual(["/crawls/broken.spiderlens"]));
  await waitFor(() => expect(t.files.opened).toEqual(["/data/SpiderLens/Unsaved/broken-recovered.spiderlens"]));
  await waitFor(() => expect(statusText()).toContain("Paused"));
});

test("a_file_that_is_not_a_crawl_only_shows_the_error", async () => {
  const t = await renderApp();
  t.files.damaged.set("/crawls/notes.spiderlens", { message: "This is not a SpiderLens crawl file.", salvageable: false });
  act(() => t.files.openFile("/crawls/notes.spiderlens"));
  const d = await dialog();
  expect(within(d).getByText("Can't open notes")).toBeTruthy();
  expect(within(d).getByText("This is not a SpiderLens crawl file.")).toBeTruthy();
  expect(within(d).queryByRole("button", { name: /Recover/ })).toBeNull();
  fireEvent.click(within(d).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(statusText()).toBe("Idle");
});

test("dropping_a_crawl_file_opens_it_and_other_files_are_ignored", async () => {
  const t = await renderApp();
  act(() => t.files.drop(["/Users/me/notes.txt"]));
  await new Promise((r) => setTimeout(r, 20));
  expect(t.files.opened).toEqual([]);
  act(() => t.files.drop(["/Users/me/urls.csv", "/Users/me/shop.spiderlens", "/Users/me/blog.spiderlens"]));
  await waitFor(() => expect(t.files.opened).toEqual(["/Users/me/shop.spiderlens"]));
  await waitFor(() => expect(statusText()).toContain("Completed"));
});

test("a_file_opened_from_the_file_manager_opens", async () => {
  const t = await renderApp();
  await openFromOs(t, "/Users/me/shop.spiderlens");
  expect(statusText()).toBe("Completed example.com");
  // It now leads the recent list.
  expect(t.files.recent[0].path).toBe("/Users/me/shop.spiderlens");
});

test("a_file_the_os_sent_before_the_window_was_ready_opens_on_start", async () => {
  const t = fakeTransport();
  t.files.pending = ["/Users/me/launch.spiderlens"];
  await renderApp(t);
  await waitFor(() => expect(t.files.opened).toEqual(["/Users/me/launch.spiderlens"]));
});

test("open_dialog_goes_through_the_same_flow", async () => {
  const t = await renderApp();
  t.files.pickPath = "/crawls/picked.spiderlens";
  fireEvent.keyDown(window, { key: "o", code: "KeyO", ctrlKey: true });
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/picked.spiderlens"]));
});

// Resuming -----------------------------------------------------------------

async function openPaused(t: T, path: string, patch: Parameters<T["files"]["seedInfo"]>[1]) {
  t.files.seedInfo(path, { status: "paused", ...patch });
  await openFromOs(t, path);
  await waitFor(() => expect(statusText()).toContain("Paused"));
  fireEvent.click(screen.getByRole("button", { name: /Resume/ }));
}

test("resuming_an_older_file_asks_and_keeps_a_copy_by_default", async () => {
  const t = await renderApp();
  await openPaused(t, "/crawls/old.spiderlens", { access: { kind: "needs_upgrade", from: 1 } });
  const d = await dialog();
  expect(within(d).getByText("Upgrade this crawl file?")).toBeTruthy();
  expect(within(d).getByText(/may no longer open/)).toBeTruthy();
  const keep = within(d).getByRole("checkbox", { name: "Keep a copy of the original" }) as HTMLInputElement;
  expect(keep.checked).toBe(true);
  expect(t.files.resumes).toEqual([]);
  fireEvent.click(within(d).getByRole("button", { name: "Upgrade and resume" }));
  await waitFor(() => expect(t.files.resumes).toEqual([{ path: "/crawls/old.spiderlens", options: { upgrade: "keep_copy" } }]));
  await screen.findByRole("button", { name: /Pause/ });
});

test("unchecking_keep_a_copy_upgrades_without_one", async () => {
  const t = await renderApp();
  await openPaused(t, "/crawls/old.spiderlens", { access: { kind: "needs_upgrade", from: 1 } });
  const d = await dialog();
  fireEvent.click(within(d).getByRole("checkbox", { name: "Keep a copy of the original" }));
  fireEvent.click(within(d).getByRole("button", { name: "Upgrade and resume" }));
  await waitFor(() => expect(t.files.resumes).toEqual([{ path: "/crawls/old.spiderlens", options: { upgrade: "no_copy" } }]));
});

test("missing_secrets_are_asked_for_and_sent_with_resume", async () => {
  const t = await renderApp();
  await openPaused(t, "/crawls/staging.spiderlens", { missing_secrets: ["Authorization", "X-Api-Key"] });
  const d = await dialog();
  expect(within(d).getByText("Enter secrets to resume")).toBeTruthy();
  const resume = within(d).getByRole("button", { name: "Resume" }) as HTMLButtonElement;
  const auth = within(d).getByLabelText("Authorization") as HTMLInputElement;
  expect(auth.type).toBe("password");
  fireEvent.change(auth, { target: { value: "Bearer abc" } });
  expect(resume.disabled).toBe(true);
  fireEvent.change(within(d).getByLabelText("X-Api-Key"), { target: { value: "k-123" } });
  fireEvent.click(resume);
  await waitFor(() =>
    expect(t.files.resumes).toEqual([{ path: "/crawls/staging.spiderlens", options: { secrets: { Authorization: "Bearer abc", "X-Api-Key": "k-123" } } }]),
  );
});

test("an_older_file_with_secrets_asks_both_and_sends_both", async () => {
  const t = await renderApp();
  await openPaused(t, "/crawls/old.spiderlens", { access: { kind: "needs_upgrade", from: 1 }, missing_secrets: ["Authorization"] });
  fireEvent.click(within(await dialog()).getByRole("button", { name: "Upgrade and resume" }));
  await screen.findByText("Enter secrets to resume");
  const d = screen.getByRole("dialog");
  fireEvent.change(within(d).getByLabelText("Authorization"), { target: { value: "x" } });
  fireEvent.click(within(d).getByRole("button", { name: "Resume" }));
  await waitFor(() => expect(t.files.resumes[0]?.options).toEqual({ secrets: { Authorization: "x" }, upgrade: "keep_copy" }));
});

test("a_read_only_file_is_saved_elsewhere_before_it_resumes", async () => {
  const t = await renderApp();
  await openPaused(t, "/Volumes/DVD/archive.spiderlens", { access: { kind: "read_only_media" } });
  const d = await dialog();
  expect(within(d).getByText("This file can't be changed here.")).toBeTruthy();
  expect(within(d).getByText(/Save a copy to resume it/)).toBeTruthy();
  expect(t.files.resumes).toEqual([]);
  fireEvent.click(within(d).getByRole("button", { name: "Save As…" }));
  await waitFor(() => expect(t.files.savedAs).toEqual([{ path: "/Volumes/DVD/archive.spiderlens", dest: "/saved/archive.spiderlens" }]));
  await waitFor(() => expect(t.files.resumes.map((r) => r.path)).toEqual(["/saved/archive.spiderlens"]));
});

test("a_file_in_use_or_from_a_newer_version_is_explained_not_resumed", async () => {
  const t = await renderApp();
  await openPaused(t, "/crawls/shared.spiderlens", { access: { kind: "in_use", by: { app_version: "0.6.1", pid: 42, host: "office-mac", since: "2026-10-13T08:00:00Z" } } });
  let d = await dialog();
  expect(within(d).getByText("This crawl is open somewhere else")).toBeTruthy();
  expect(d.textContent).toContain("SpiderLens 0.6.1 on office-mac");
  fireEvent.click(within(d).getByRole("button", { name: "OK" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  t.files.seedInfo("/crawls/shared.spiderlens", { status: "paused", access: { kind: "newer_format", needs: "0.9.0" } });
  fireEvent.click(screen.getByRole("button", { name: /Resume/ }));
  d = await dialog();
  expect(within(d).getByText("Made by a newer SpiderLens")).toBeTruthy();
  expect(d.textContent).toContain("needs SpiderLens 0.9.0");
  expect(t.files.resumes).toEqual([]);
});

test("resume_command_uses_the_same_checks", async () => {
  const t = await renderApp();
  t.files.seedInfo("/crawls/old.spiderlens", { status: "paused", access: { kind: "needs_upgrade", from: 1 } });
  await openFromOs(t, "/crawls/old.spiderlens");
  await palette("Resume crawl");
  expect(within(await dialog()).getByText("Upgrade this crawl file?")).toBeTruthy();
});

// Crawl information ---------------------------------------------------------

test("crawl_information_explains_the_file_and_starts_a_new_crawl_with_its_settings", async () => {
  const t = await renderApp();
  const config = defaultConfig();
  config.limits.max_urls = 500;
  config.scope.include = ["/blog/"];
  config.scope.exclude = ["/tag/"];
  config.identity.headers = [
    { name: "Authorization", value: { secret: true }, send_to: "internal" },
    { name: "X-Env", value: { plain: "staging" }, send_to: "internal" },
  ];
  t.files.seedInfo("/crawls/shop.spiderlens", {
    name: "shop",
    start_url: "https://shop.example/",
    config,
    size_bytes: 120 * MB,
    pause_reason: "Disk is full",
    status: "paused",
    versions: [
      { kind: "created", app_version: "0.5.0", os: "Windows 11", at: "2026-09-01T10:00:00Z" },
      { kind: "upgraded", app_version: "0.6.0", os: "macOS 15.1", at: "2026-10-01T10:00:00Z" },
      { kind: "resumed", app_version: "0.6.0", os: "macOS 15.1", at: "2026-10-01T10:01:00Z" },
    ],
    events: [{ at: "2026-10-01T10:30:00Z", kind: "paused", app_version: "0.6.0", os: "macOS 15.1", detail: "Disk is full" }],
  });
  await openFromOs(t, "/crawls/shop.spiderlens");
  await palette("Crawl information");
  const sheet = await dialog();
  await within(sheet).findByText("Created with SpiderLens 0.5.0");
  expect(within(sheet).getByText("Upgraded with SpiderLens 0.6.0")).toBeTruthy();
  expect(within(sheet).getByText("Resumed with SpiderLens 0.6.0")).toBeTruthy();
  expect(within(sheet).getByText("on Windows 11")).toBeTruthy();
  expect(within(sheet).getByRole("list", { name: "History" }).textContent).toContain("Paused: Disk is full");
  expect(sheet.textContent).toContain("120 MB");
  expect(sheet.textContent).toContain("Paused (Disk is full)");
  const conf = within(sheet).getByLabelText("Configuration");
  expect(within(conf).getByText("500")).toBeTruthy();
  expect(within(conf).getByText("/blog/")).toBeTruthy();
  expect(within(conf).getByText("/tag/")).toBeTruthy();
  expect(within(conf).getByText("Header Authorization").nextSibling?.textContent).toBe("secret, not stored");
  expect(within(conf).getByText("Header X-Env").nextSibling?.textContent).toBe("staging");

  fireEvent.click(within(sheet).getByRole("button", { name: "New crawl with these settings" }));
  await screen.findByRole("heading", { name: "Crawl a website" });
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).value).toBe("https://shop.example/");
  expect(statusText()).toBe("Idle");
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.limits.max_urls).toBe(500);
  expect(t.crawls.started[0].config.scope.include).toEqual(["/blog/"]);
});

test("crawl_information_opens_from_the_overview", async () => {
  await startCrawl();
  fireEvent.click(screen.getByRole("button", { name: "Crawl information" }));
  const sheet = await dialog();
  await within(sheet).findByText("Created with SpiderLens 0.6.0");
});

test("crawl_information_is_off_without_a_crawl", async () => {
  await renderApp();
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: "Crawl information" } });
  expect(screen.queryByText("Crawl information")).toBeNull();
  fireEvent.change(input, { target: { value: "Save crawl as" } });
  expect(screen.queryByText("Save crawl as…")).toBeNull();
});

// Save As ------------------------------------------------------------------

test("save_as_moves_the_crawl_and_a_move_event_updates_the_path", async () => {
  const t = await startCrawl();
  fireEvent.keyDown(window, { key: "S", code: "KeyS", ctrlKey: true, shiftKey: true });
  await waitFor(() => expect(t.files.savedAs).toEqual([{ path: "/crawls/example.com.spiderlens", dest: "/saved/example.com.spiderlens" }]));
  await screen.findByText("/saved/example.com.spiderlens");
  expect(screen.getAllByRole("status").some((s) => s.textContent === "Saved to /saved/example.com.spiderlens")).toBe(true);

  act(() => t.files.moved({ id: 1, from: "/saved/example.com.spiderlens", to: "/Volumes/Big/example.com.spiderlens" }));
  await screen.findByText("/Volumes/Big/example.com.spiderlens");
  // Results keep going to the new file: resume uses it.
  fireEvent.click(screen.getByRole("button", { name: /Pause/ }));
  act(() => t.crawls.finish({ path: "/Volumes/Big/example.com.spiderlens", report: fakeReport("paused", 5) }));
  fireEvent.click(screen.getByRole("button", { name: /Resume/ }));
  await waitFor(() => expect(t.crawls.resumed).toEqual(["/Volumes/Big/example.com.spiderlens"]));
});

test("cancelling_the_save_dialog_changes_nothing", async () => {
  const t = await startCrawl();
  t.files.saveDest = null;
  await palette("Save crawl as");
  await new Promise((r) => setTimeout(r, 20));
  expect(t.files.savedAs).toEqual([]);
  expect(screen.getByText("/crawls/example.com.spiderlens")).toBeTruthy();
});

// Disk problems and quitting -------------------------------------------------

test("a_disk_problem_shows_cause_path_free_space_and_actions", async () => {
  const t = await startCrawl();
  act(() =>
    t.crawls.finish({
      report: fakeReport("paused", 5),
      local: fakeLocalError("disk_low", { message: "Less than 2 GB is free.", free_bytes: 50 * MB }),
    }),
  );
  const notice = screen.getByRole("alert", { name: "Disk space is low" });
  expect(notice.textContent).toContain("Less than 2 GB is free.");
  expect(within(notice).getByText("/crawls/example.com.spiderlens")).toBeTruthy();
  expect(within(notice).getByText("50 MB")).toBeTruthy();
  expect(within(notice).getAllByRole("button").map((b) => b.textContent)).toEqual(["Resume", "Save As elsewhere…", "Keep paused"]);

  fireEvent.click(within(notice).getByRole("button", { name: "Save As elsewhere…" }));
  await waitFor(() => expect(t.files.savedAs).toHaveLength(1));

  fireEvent.click(within(notice).getByRole("button", { name: "Keep paused" }));
  expect(screen.queryByRole("alert", { name: "Disk space is low" })).toBeNull();
  expect(statusText()).toContain("Paused");
});

test("resume_from_the_disk_notice_continues_the_crawl", async () => {
  const t = await startCrawl();
  act(() => t.crawls.finish({ report: fakeReport("paused", 5), local: fakeLocalError("drive_gone", { free_bytes: null }) }));
  const notice = screen.getByRole("alert", { name: "The drive was disconnected" });
  expect(within(notice).queryByText("Free space")).toBeNull();
  fireEvent.click(within(notice).getByRole("button", { name: "Resume" }));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.crawls.resumed).toEqual(["/crawls/example.com.spiderlens"]);
  expect(screen.queryByRole("alert", { name: "The drive was disconnected" })).toBeNull();
});

test("disk_problem_titles", () => {
  expect(localErrorTitle).toEqual({
    disk_full: "Disk is full",
    disk_low: "Disk space is low",
    drive_gone: "The drive was disconnected",
    permission: "Permission denied",
    network: "The network seems to be down",
    other: "Couldn't write the crawl file",
  });
});

test("quitting_during_a_crawl_says_it_is_being_paused", async () => {
  const t = await startCrawl();
  act(() => t.files.quitting(false));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  act(() => t.files.quitting(true));
  const notice = screen.getByRole("alertdialog", { name: "Quitting" });
  expect(notice.textContent).toBe("Pausing the crawl so you can resume it later…");
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.keyDown(notice, { key: "Escape" });
  expect(screen.getByRole("alertdialog", { name: "Quitting" })).toBeTruthy();
});
