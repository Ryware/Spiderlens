// The start screen's crawl files (spec 006 US4): recent crawls, unsaved
// crawls and the space they use.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeRecent, fakeTransport } from "../testing/fakeTransport";

const GB = 1024 ** 3;
const recentList = () => screen.getByRole("list", { name: "Recent crawls" });
const row = (path: string) => recentList().querySelector(`[data-path="${path}"]`) as HTMLElement;
const statusText = () => within(screen.getByRole("contentinfo", { name: "Crawl status" })).getByRole("status").textContent;

async function renderStart(t = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

test("recent_crawls_show_details_most_recent_first_and_open_with_one_click", async () => {
  const t = fakeTransport();
  t.files.seedRecent(
    fakeRecent("/crawls/blog.spiderlens", { status: "stopped", last_used: "2026-10-01T10:00:00Z", urls: 4200 }),
    fakeRecent("/crawls/shop.spiderlens", { status: "paused", recovered: true, last_used: "2026-10-12T10:00:00Z", urls: 1500, start_url: "https://shop.example/" }),
    fakeRecent("/data/Unsaved/news.spiderlens", { status: "completed", unsaved: true, last_used: "2026-10-05T10:00:00Z" }),
  );
  await renderStart(t);
  await screen.findByRole("list", { name: "Recent crawls" });
  const items = within(recentList()).getAllByRole("listitem");
  expect(items.map((i) => i.dataset.path)).toEqual(["/crawls/shop.spiderlens", "/data/Unsaved/news.spiderlens", "/crawls/blog.spiderlens"]);

  const shop = row("/crawls/shop.spiderlens");
  expect(shop.textContent).toContain("shop");
  expect(shop.textContent).toContain("https://shop.example/");
  expect(shop.textContent).toContain("1,500 URLs");
  expect(within(shop).getByText("Paused")).toBeTruthy();
  expect(within(shop).getByText("Recovered")).toBeTruthy();
  expect(within(row("/data/Unsaved/news.spiderlens")).getByText("Unsaved")).toBeTruthy();
  expect(within(row("/data/Unsaved/news.spiderlens")).getByText("Completed")).toBeTruthy();
  expect(within(row("/crawls/blog.spiderlens")).getByText("Stopped")).toBeTruthy();

  fireEvent.click(within(shop).getByRole("button"));
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/shop.spiderlens"]));
  await waitFor(() => expect(statusText()).toContain("Paused"));
});

test("missing_file_can_be_removed_from_the_list_without_deleting_it", async () => {
  const t = fakeTransport();
  t.files.seedRecent(fakeRecent("/Volumes/USB/old.spiderlens", { available: false }));
  await renderStart(t);
  const r = await screen.findByText("File not found");
  const item = r.closest("li")!;
  // Not one button: a missing file cannot be opened.
  expect(within(item).getAllByRole("button").map((b) => b.textContent)).toEqual(["Locate…", "Remove"]);
  expect(within(item).getByRole("button", { name: "Remove" }).getAttribute("aria-description")).toContain("not deleted");

  fireEvent.click(within(item).getByRole("button", { name: "Remove" }));
  await waitFor(() => expect(screen.queryByRole("list", { name: "Recent crawls" })).toBeNull());
  expect(t.files.removed).toEqual(["/Volumes/USB/old.spiderlens"]);
  expect(t.files.deleted).toEqual([]);
});

test("locate_points_a_missing_entry_at_the_moved_file", async () => {
  const t = fakeTransport();
  t.files.seedRecent(fakeRecent("/Volumes/USB/old.spiderlens", { available: false }));
  t.files.pickPath = "/crawls/old.spiderlens";
  await renderStart(t);
  fireEvent.click(await screen.findByRole("button", { name: "Locate…" }));
  await waitFor(() => expect(row("/crawls/old.spiderlens")).toBeTruthy());
  expect(screen.queryByText("File not found")).toBeNull();
  fireEvent.click(within(row("/crawls/old.spiderlens")).getByRole("button"));
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/old.spiderlens"]));
});

test("unsaved_crawls_show_their_size_and_delete_asks_first", async () => {
  const t = fakeTransport();
  t.files.seedUnsaved("/data/Unsaved/big.spiderlens", 1 * GB);
  t.files.seedUnsaved("/data/Unsaved/small.spiderlens", 0.2 * GB);
  await renderStart(t);
  expect((await screen.findByTestId("unsaved-total")).textContent).toBe("Unsaved crawls use 1.2 GB");
  const list = screen.getByRole("list", { name: "Unsaved crawls" });
  const big = list.querySelector('[data-path="/data/Unsaved/big.spiderlens"]') as HTMLElement;
  expect(big.textContent).toContain("1.0 GB");

  fireEvent.click(within(big).getByRole("button", { name: "Delete…" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText("Delete big? This can't be undone.")).toBeTruthy();
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(t.files.deleted).toEqual([]);
  expect(list.querySelector('[data-path="/data/Unsaved/big.spiderlens"]')).toBeTruthy();

  fireEvent.click(within(big).getByRole("button", { name: "Delete…" }));
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(t.files.deleted).toEqual(["/data/Unsaved/big.spiderlens"]));
  await waitFor(() => expect(screen.getByTestId("unsaved-total").textContent).toBe("Unsaved crawls use 205 MB"));
  expect(screen.queryByText("big")).toBeNull();
});

test("low_space_for_unsaved_crawls_is_a_warning_at_the_top", async () => {
  const t = fakeTransport();
  t.files.storage = { ...t.files.storage, free_bytes: 1.5 * GB, reserve_bytes: 2 * GB, low: true };
  await renderStart(t);
  const alert = await screen.findByText(/Only 1.5 GB free on the drive for unsaved crawls/);
  expect(alert.textContent).toContain("keep at least 2.0 GB");
});

test("no_warning_when_there_is_room", async () => {
  await renderStart();
  await waitFor(() => expect(screen.queryByText(/free on the drive/)).toBeNull());
});

test("recovered_crawls_appear_when_the_app_reports_them", async () => {
  const t = await renderStart();
  expect(screen.queryByRole("list", { name: "Recent crawls" })).toBeNull();
  const entry = fakeRecent("/data/Unsaved/crashed.spiderlens", { status: "paused", recovered: true, unsaved: true });
  t.files.recent = [entry];
  act(() => t.files.recovered([entry]));
  await screen.findByRole("list", { name: "Recent crawls" });
  expect(within(row("/data/Unsaved/crashed.spiderlens")).getByText("Recovered")).toBeTruthy();
});

test("recent_list_refreshes_after_a_crawl_finishes", async () => {
  const t = await renderStart();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  const calls = vi.spyOn(t, "recentList");
  act(() => t.crawls.finish({}));
  expect(calls).toHaveBeenCalled();
});

test("palette_offers_the_first_five_recent_crawls", async () => {
  const t = fakeTransport();
  const names = ["a", "b", "c", "d", "e", "f"];
  t.files.seedRecent(...names.map((n, i) => fakeRecent(`/crawls/${n}.spiderlens`, { last_used: `2026-10-0${9 - i}T10:00:00Z` })));
  await renderStart(t);
  await screen.findByRole("list", { name: "Recent crawls" });
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: "Open recent" } });
  const palette = screen.getByRole("dialog", { name: "Command palette" });
  expect(within(palette).getAllByText(/^Open recent:/).map((e) => e.textContent)).toEqual(["Open recent: a", "Open recent: b", "Open recent: c", "Open recent: d", "Open recent: e"]);
  fireEvent.change(input, { target: { value: "Open recent: c" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(t.files.opened).toEqual(["/crawls/c.spiderlens"]));
});
