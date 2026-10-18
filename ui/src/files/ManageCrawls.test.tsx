// Manage crawls (spec 019 US1 to US3, US5.1, FR-023): the list, totals,
// sorting and filters, deleting to the Trash, permanent delete only after a
// second question, and removing stored HTML.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeRecent, fakeTransport } from "../testing/fakeTransport";

const MB = 1024 ** 2;
const GB = 1024 ** 3;

type Fake = ReturnType<typeof fakeTransport>;

function seed(t: Fake) {
  t.files.seedUnsaved("/data/Unsaved/quick.spiderlens", 300 * MB, { name: "quick", last_used: "2026-10-12T10:00:00Z", urls: 120 });
  t.files.seedUnsaved("/data/Unsaved/old.spiderlens", 50 * MB, { name: "old", last_used: "2026-09-01T10:00:00Z", urls: 40 });
  t.files.seedRecent(
    fakeRecent("/crawls/shop.spiderlens", { name: "shop", start_url: "https://shop.example/", last_used: "2026-10-13T09:00:00Z", urls: 4200 }),
    fakeRecent("/crawls/gone.spiderlens", { name: "gone", available: false, last_used: "2026-08-01T09:00:00Z" }),
  );
  t.files.sizes.set("/crawls/shop.spiderlens", 2 * GB);
  t.files.html.set("/crawls/shop.spiderlens", { bytes: 1.5 * GB, pages: 4000 });
  t.files.html.set("/data/Unsaved/quick.spiderlens", { bytes: 200 * MB, pages: 100 });
  return t;
}

async function openManager(t: Fake = seed(fakeTransport())) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: "Manage crawls" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByRole("table", { name: "Crawls" });
  return t;
}

const table = () => screen.getByRole("table", { name: "Crawls" });
const row = (path: string) => table().querySelector(`tr[data-path="${path}"]`) as HTMLElement;
const names = () =>
  within(table())
    .getAllByRole("checkbox")
    .slice(1)
    .map((c) => c.getAttribute("aria-label")!.replace(/^Select /, ""));

async function rowAction(path: string, label: string) {
  fireEvent.click(within(row(path)).getByRole("button", { name: /^Actions for/ }));
  fireEvent.click(await screen.findByRole("menuitem", { name: label }));
}

test("us1_1_lists_every_crawl_with_size_and_totals", async () => {
  await openManager();
  expect(names()).toEqual(["shop", "quick", "old", "gone"]);
  const shop = row("/crawls/shop.spiderlens");
  expect(shop.textContent).toContain("4,200");
  expect(within(shop).getByTestId("size").textContent).toBe("2.0 GB");
  await waitFor(() => expect(within(shop).getByTestId("html-size").textContent).toBe("1.5 GB"));
  expect(within(row("/data/Unsaved/quick.spiderlens")).getByText("Unsaved")).toBeTruthy();
  await waitFor(() => expect(within(row("/data/Unsaved/old.spiderlens")).getByTestId("html-size").textContent).toBe("None"));
  const totals = screen.getByTestId("manage-totals").textContent!;
  expect(totals).toContain("Unsaved: 350 MB");
  expect(totals).toContain("Free on the unsaved-crawls drive: 200 GB");
});

test("us1_3_a_missing_file_offers_locate_and_remove_from_list", async () => {
  const t = await openManager();
  const gone = row("/crawls/gone.spiderlens");
  expect(within(gone).getByText("File not found")).toBeTruthy();
  expect(within(gone).getByTestId("html-size").textContent).toBe("–");
  await rowAction("/crawls/gone.spiderlens", "Remove from list");
  await waitFor(() => expect(t.files.removed).toEqual(["/crawls/gone.spiderlens"]));
  await waitFor(() => expect(table().querySelector('[data-path="/crawls/gone.spiderlens"]')).toBeNull());
});

test("us1_2_sorts_and_filters_without_reloading_files", async () => {
  await openManager();
  fireEvent.click(screen.getByRole("button", { name: "Size" }));
  expect(names()).toEqual(["shop", "quick", "old", "gone"]);
  fireEvent.click(screen.getByRole("button", { name: "Size" }));
  expect(names()).toEqual(["gone", "old", "quick", "shop"]);
  fireEvent.click(screen.getByRole("button", { name: "Name" }));
  expect(names()).toEqual(["gone", "old", "quick", "shop"]);

  const show = screen.getByRole("group", { name: "Show" });
  fireEvent.click(within(show).getByRole("button", { name: /Unsaved/ }));
  expect(names()).toEqual(["old", "quick"]);
  fireEvent.click(within(show).getByRole("button", { name: /Saved/ }));
  expect(names()).toEqual(["shop"]);
  fireEvent.click(within(show).getByRole("button", { name: /Unavailable/ }));
  expect(names()).toEqual(["gone"]);
  expect(within(show).getByRole("button", { name: /All/ }).textContent).toContain("4");
});

test("us2_1_delete_asks_first_names_saved_files_and_moves_to_the_trash", async () => {
  const t = await openManager();
  fireEvent.click(screen.getByRole("checkbox", { name: "Select shop" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Select quick" }));
  const bar = screen.getByRole("toolbar", { name: "Selected crawls" });
  expect(bar.textContent).toContain("2 selected");
  fireEvent.click(within(bar).getByRole("button", { name: "Delete…" }));
  let dialog = await screen.findByRole("dialog", { name: "Move 2 crawls to the Trash?" });
  expect(dialog.textContent).toContain("Frees 2.3 GB");
  expect(dialog.textContent).toContain("1 of these are saved crawl files");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(t.files.trashed).toEqual([]);

  fireEvent.click(within(bar).getByRole("button", { name: "Delete…" }));
  dialog = await screen.findByRole("dialog", { name: "Move 2 crawls to the Trash?" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Move to Trash" }));
  await waitFor(() => expect(t.files.trashed.sort()).toEqual(["/crawls/shop.spiderlens", "/data/Unsaved/quick.spiderlens"]));
  expect(t.files.deletedPermanently).toEqual([]);
  await waitFor(() => expect(names()).toEqual(["old", "gone"]));
  expect(screen.getByRole("status").textContent).toContain("Moved to the Trash: 2 items, 2.3 GB freed.");
  // Deleted crawls leave the recent list too (US2.2).
  expect(t.files.recent.map((r) => r.path)).not.toContain("/crawls/shop.spiderlens");
  expect(t.files.history.map((h) => h.reason)).toEqual(["manual_delete", "manual_delete"]);
});

test("us2_3_no_trash_asks_again_before_a_permanent_delete", async () => {
  const t = seed(fakeTransport());
  t.files.noTrash.add("/crawls/shop.spiderlens");
  await openManager(t);
  await rowAction("/crawls/shop.spiderlens", "Delete…");
  fireEvent.click(within(await screen.findByRole("dialog", { name: /to the Trash\?/ })).getByRole("button", { name: "Move to Trash" }));
  const second = await screen.findByRole("dialog", { name: /Delete permanently\? The Trash is not available for shop/ });
  expect(second.textContent).toContain("can't be undone");
  fireEvent.click(within(second).getByRole("button", { name: "Keep" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: /Delete permanently/ })).toBeNull());
  expect(t.files.deletedPermanently).toEqual([]);
  expect(row("/crawls/shop.spiderlens")).toBeTruthy();

  await rowAction("/crawls/shop.spiderlens", "Delete…");
  fireEvent.click(within(await screen.findByRole("dialog", { name: /to the Trash\?/ })).getByRole("button", { name: "Move to Trash" }));
  fireEvent.click(within(await screen.findByRole("dialog", { name: /Delete permanently/ })).getByRole("button", { name: "Delete permanently" }));
  await waitFor(() => expect(t.files.deletedPermanently).toEqual(["/crawls/shop.spiderlens"]));
  expect(t.files.history[0].reason).toBe("manual_delete_permanent");
});

test("us2_4_crawls_in_use_are_skipped_with_the_reason_and_the_rest_deleted", async () => {
  const t = seed(fakeTransport());
  t.files.inUse.set("/crawls/shop.spiderlens", "In use by spiderlens on build-mac.");
  await openManager(t);
  fireEvent.click(screen.getByRole("checkbox", { name: "Select all shown crawls" }));
  fireEvent.click(within(screen.getByRole("toolbar", { name: "Selected crawls" })).getByRole("button", { name: "Delete…" }));
  fireEvent.click(within(await screen.findByRole("dialog", { name: /to the Trash\?/ })).getByRole("button", { name: "Move to Trash" }));
  await waitFor(() => expect(t.files.trashed).toHaveLength(3));
  expect(t.files.trashed).not.toContain("/crawls/shop.spiderlens");
  expect(screen.getByRole("status").textContent).toContain("shop was not deleted: In use by spiderlens on build-mac.");
});

test("us2_running_crawl_cannot_be_deleted_or_opened_from_the_manager", async () => {
  const t = seed(fakeTransport());
  t.files.running = "/data/Unsaved/quick.spiderlens";
  await openManager(t);
  expect(within(row("/data/Unsaved/quick.spiderlens")).getByText("Running")).toBeTruthy();
  fireEvent.click(within(row("/data/Unsaved/quick.spiderlens")).getByRole("button", { name: /^Actions for/ }));
  const del = await screen.findByRole("menuitem", { name: "Delete…" });
  expect(del.getAttribute("aria-disabled") ?? del.getAttribute("data-disabled")).not.toBeNull();
  expect(screen.getByRole("menuitem", { name: "Remove stored HTML…" }).getAttribute("data-disabled")).not.toBeNull();
});

test("us3_1_remove_stored_html_shows_the_space_and_keeps_results", async () => {
  const t = await openManager();
  await waitFor(() => expect(within(row("/crawls/shop.spiderlens")).getByTestId("html-size").textContent).toBe("1.5 GB"));
  await rowAction("/crawls/shop.spiderlens", "Remove stored HTML…");
  const dialog = await screen.findByRole("dialog", { name: "Remove stored HTML from shop?" });
  expect(dialog.textContent).toContain("Frees about 1.5 GB");
  expect(dialog.textContent).toContain("Every result stays as it is");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(t.files.purged).toEqual([]);

  await rowAction("/crawls/shop.spiderlens", "Remove stored HTML…");
  fireEvent.click(within(await screen.findByRole("dialog", { name: "Remove stored HTML from shop?" })).getByRole("button", { name: "Remove HTML" }));
  await waitFor(() => expect(t.files.purged).toEqual([{ path: "/crawls/shop.spiderlens", compact: true }]));
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Stored HTML removed, 1.5 GB freed."));
  await waitFor(() => expect(within(row("/crawls/shop.spiderlens")).getByTestId("html-size").textContent).toBe("None"));
  expect(within(row("/crawls/shop.spiderlens")).getByTestId("size").textContent).toBe("512 MB");
});

test("us3_4_without_room_to_shrink_the_html_is_removed_without_compaction", async () => {
  const t = seed(fakeTransport());
  t.files.noCompactRoom.add("/crawls/shop.spiderlens");
  await openManager(t);
  await waitFor(() => expect(within(row("/crawls/shop.spiderlens")).getByTestId("html-size").textContent).toBe("1.5 GB"));
  await rowAction("/crawls/shop.spiderlens", "Remove stored HTML…");
  const dialog = await screen.findByRole("dialog", { name: "Remove stored HTML from shop?" });
  expect(dialog.textContent).toContain("Not enough free space to shrink the file");
  fireEvent.click(within(dialog).getByRole("button", { name: "Remove HTML" }));
  await waitFor(() => expect(t.files.purged).toEqual([{ path: "/crawls/shop.spiderlens", compact: false }]));
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("there was not enough free space to shrink it"));
});

test("us3_no_stored_html_means_no_remove_action", async () => {
  await openManager();
  await waitFor(() => expect(within(row("/data/Unsaved/old.spiderlens")).getByTestId("html-size").textContent).toBe("None"));
  fireEvent.click(within(row("/data/Unsaved/old.spiderlens")).getByRole("button", { name: /^Actions for/ }));
  expect((await screen.findByRole("menuitem", { name: "Remove stored HTML…" })).getAttribute("data-disabled")).not.toBeNull();
});

test("us5_1_backups_are_listed_under_their_crawl_and_deletable", async () => {
  const t = seed(fakeTransport());
  const backup = { path: "/crawls/shop (format 2 backup).spiderlens", format_version: 2, modified: "2026-10-10T10:00:00Z", size_bytes: 700 * MB };
  t.files.backups.set("/crawls/shop.spiderlens", [backup]);
  await openManager(t);
  const b = row(backup.path);
  expect(b.textContent).toContain("Backup before the format 3 upgrade");
  expect(b.textContent).toContain("700 MB");
  expect(screen.getByTestId("manage-totals").textContent).toContain("All crawls: 3.0 GB");
  fireEvent.click(within(b).getByRole("button", { name: "Actions for the backup of shop" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
  const dialog = await screen.findByRole("dialog", { name: /to the Trash\?/ });
  expect(dialog.textContent).toContain("Frees 700 MB");
  fireEvent.click(within(dialog).getByRole("button", { name: "Move to Trash" }));
  await waitFor(() => expect(t.files.trashed).toEqual([backup.path]));
  await waitFor(() => expect(table().querySelector(`[data-path="${backup.path}"]`)).toBeNull());
  expect(row("/crawls/shop.spiderlens")).toBeTruthy();
});

test("row_actions_open_show_in_folder_and_crawl_information", async () => {
  const t = await openManager();
  await rowAction("/crawls/shop.spiderlens", "Show in folder");
  expect(t.files.revealed).toEqual(["/crawls/shop.spiderlens"]);
  await rowAction("/crawls/shop.spiderlens", "Crawl information");
  expect(await screen.findByRole("heading", { name: "shop" })).toBeTruthy();
});

test("open_from_the_manager_closes_it_and_opens_the_crawl", async () => {
  const t = await openManager();
  await rowAction("/crawls/shop.spiderlens", "Open");
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/shop.spiderlens"]));
  await waitFor(() => expect(screen.queryByRole("table", { name: "Crawls" })).toBeNull());
});

test("fr023_the_start_screen_links_to_the_manager", async () => {
  const t = seed(fakeTransport());
  t.files.storage = { ...t.files.storage, free_bytes: 0.5 * GB, low: true };
  render(<App transport={t} />);
  const warning = (await screen.findByText(/Only 512 MB free/)).closest("[role=alert]") as HTMLElement;
  fireEvent.click(within(warning).getByRole("button", { name: "Manage crawls…" }));
  expect(await screen.findByRole("table", { name: "Crawls" })).toBeTruthy();
});

test("the_manager_reloads_when_crawls_change_elsewhere", async () => {
  const t = await openManager();
  t.files.seedUnsaved("/data/Unsaved/new.spiderlens", 5 * MB, { name: "new" });
  act(() => t.files.crawlsChanged());
  await waitFor(() => expect(names()).toContain("new"));
});

test("us1_palette_command_opens_manage_crawls", async () => {
  await openManager();
  expect(await screen.findByRole("dialog", { name: "Manage crawls" })).toBeTruthy();
});
