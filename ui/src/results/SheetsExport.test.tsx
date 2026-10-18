// Export to Google Sheets and Looker Studio (spec 024 US6): turning Sheets
// on and signing in from the dialog, what is sent (the view's visible
// columns), the 10M-cell warning with truncate or split (FR-030), and the
// Looker Studio link (FR-031). The Sheets API itself is tested in Rust
// (crates/integrations/tests/sheets.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

const rows = () => within(screen.getByRole("grid")).getAllByRole("row").slice(1);

async function openInternal(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Internal" }));
  await waitFor(() => expect(rows().length).toBeGreaterThan(0));
  return t;
}

const signedIn = (t: ReturnType<typeof fakeTransport>) => {
  t.integrations.connections = [{ kind: "google", label: "ana@example.com", scopes: ["sheets"], created_at: "", store: "keychain" }];
  t.integrations.settings.sheets = { enabled: true, per_minute: null, concurrency: null };
};

async function openDialog(item: RegExp) {
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: item }));
  return screen.findByRole("dialog");
}

test("us6_sheets_is_off_then_needs_a_sign_in_then_exports_the_visible_columns", async () => {
  const t = await openInternal();
  const dialog = await openDialog(/Export to Google Sheets/);
  // Off by default: nothing can be sent until it is turned on.
  expect((within(dialog).getByRole("button", { name: "Export" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(await within(dialog).findByRole("button", { name: "Turn on Google Sheets" }));
  fireEvent.click(await within(dialog).findByRole("button", { name: "Connect Google for Sheets" }));
  await within(dialog).findByText(/will be sent to a new spreadsheet in ana@example.com/);
  expect(t.integrations.calls).toContain("connect sheets");

  fireEvent.click(within(dialog).getByRole("button", { name: "Export" }));
  const list = await within(dialog).findByRole("list", { name: "Spreadsheets" });
  expect(list.querySelector("a")?.getAttribute("href")).toContain("docs.google.com/spreadsheets/d/");
  expect(t.linkMetrics.exports).toHaveLength(1);
  expect(t.linkMetrics.exports[0].columns[0]).toBe("address");
  expect(t.linkMetrics.exports[0].overflow).toBe("refuse");
  expect(within(dialog).getByText(/Sent 1,200 rows to a new spreadsheet/)).toBeTruthy();
});

test("fr_030_a_view_over_ten_million_cells_warns_before_sending_and_can_split", async () => {
  const t = fakeTransport();
  signedIn(t);
  t.linkMetrics.sheetRows = 2_500_000;
  t.linkMetrics.capacity = 10_000_000;
  await openInternal(t);
  const dialog = await openDialog(/Export to Google Sheets/);
  const warning = await within(dialog).findByRole("alert");
  expect(warning.textContent).toMatch(/A spreadsheet holds 10,000,000/);
  expect(warning.textContent).toMatch(/split it across/);
  // Nothing was sent by looking at the warning.
  expect(t.linkMetrics.exports).toHaveLength(0);

  fireEvent.click(within(dialog).getByRole("radio", { name: /Split into/ }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Export" }));
  const list = await within(dialog).findByRole("list", { name: "Spreadsheets" });
  expect(list.querySelectorAll("li").length).toBeGreaterThan(1);
  expect(t.linkMetrics.exports[0].overflow).toBe("split");
});

test("fr_030_truncate_sends_only_the_rows_that_fit_and_says_so", async () => {
  const t = fakeTransport();
  signedIn(t);
  t.linkMetrics.sheetRows = 2_500_000;
  await openInternal(t);
  const dialog = await openDialog(/Export to Google Sheets/);
  await within(dialog).findByRole("alert");
  fireEvent.click(within(dialog).getByRole("button", { name: "Export" }));
  await within(dialog).findByText(/Sent the first .* of 2,500,000 rows/);
  expect(t.linkMetrics.exports[0].overflow).toBe("truncate");
});

test("fr_031_open_in_looker_studio_from_the_menu_and_after_an_export", async () => {
  const t = fakeTransport();
  signedIn(t);
  await openInternal(t);
  const dialog = await openDialog(/Open in Looker Studio/);
  fireEvent.click(await within(dialog).findByRole("button", { name: "Export and open in Looker Studio" }));
  await within(dialog).findByRole("list", { name: "Spreadsheets" });
  expect(t.linkMetrics.opened).toHaveLength(1);
  expect(t.linkMetrics.opened[0]).toContain("lookerstudio.google.com/reporting/create");
  expect(t.linkMetrics.opened[0]).toContain("ds.ds0.spreadsheetId=fake-sheet-1");
  fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  // A plain Sheets export offers the same link afterwards.
  const again = await openDialog(/Export to Google Sheets/);
  fireEvent.click(await within(again).findByRole("button", { name: "Export" }));
  fireEvent.click(await within(again).findByRole("button", { name: "Open in Looker Studio" }));
  await waitFor(() => expect(t.linkMetrics.opened).toHaveLength(2));
});
