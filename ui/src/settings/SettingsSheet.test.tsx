// Spec 002's UI tests, ported to the settings sheet (spec 017 SC-009), plus
// the sheet's own scenarios (spec 017 US2).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { defaultConfig, fakeTransport } from "../testing/fakeTransport";

async function setup(transport = fakeTransport()) {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  return transport;
}

const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;

async function openSettings(category?: string) {
  // The open sheet is modal, so switch categories inside it instead of reopening.
  let nav = screen.queryByRole("navigation", { name: "Settings categories" });
  if (!nav) {
    fireEvent.click(screen.getByRole("button", { name: "Configure" }));
    nav = await screen.findByRole("navigation", { name: "Settings categories" });
  }
  if (category) fireEvent.click(within(nav).getByRole("tab", { name: new RegExp(`^${category}`) }));
  return nav;
}

test("start_adds_https_and_shows_completed_address", async () => {
  await setup();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  expect((screen.getByLabelText("Start URL") as HTMLInputElement).value).toBe("https://example.com/");
  expect(screen.getByText(/Ready to crawl https:\/\/example.com\//)).toBeTruthy();
});

test("invalid_start_url_blocks_start_with_message", async () => {
  await setup();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "ht!tp//bad" } });
  expect((await screen.findByRole("alert")).textContent).toContain("start URL is not usable");
  expect(start().disabled).toBe(true);
  expect(screen.getByLabelText("Start URL").getAttribute("aria-invalid")).toBe("true");
});

test("less_respectful_settings_are_labelled", async () => {
  // Spec 002 SC-009: every less-respectful choice shows a visible warning.
  await setup();
  await openSettings("robots.txt");
  fireEvent.change(screen.getByLabelText("robots.txt behaviour"), { target: { value: "ignore" } });
  await openSettings("Identity");
  fireEvent.change(screen.getByLabelText("User agent"), { target: { value: "googlebot_smartphone" } });
  await openSettings("Speed");
  fireEvent.click(screen.getByRole("radio", { name: "Fast" }));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  const summary = await screen.findByLabelText("Less respectful settings");
  await waitFor(() => {
    expect(summary.textContent).toContain("robots.txt will be crawled");
    expect(summary.textContent).toContain("no longer identify itself as SpiderLens");
    expect(summary.textContent).toContain("intended for sites you control");
  });
  expect(screen.getByRole("button", { name: "3 less respectful settings" })).toBeTruthy();
});

test("editing_a_speed_value_shows_custom", async () => {
  await setup();
  await openSettings("Speed");
  await waitFor(() => expect(screen.getByRole("radio", { name: "Polite" }).getAttribute("aria-checked")).toBe("true"));
  fireEvent.change(screen.getByLabelText("Max requests per site"), { target: { value: "3" } });
  await waitFor(() => expect(screen.getByRole("radio", { name: "Custom" }).getAttribute("aria-checked")).toBe("true"));
});

test("loaded_profile_shows_modified_and_reverts", async () => {
  const transport = fakeTransport();
  const config = defaultConfig();
  config.limits.max_urls = 100;
  transport.saved.set("Client A", { name: "Client A", config, start_url: null, ignored: [] });
  await setup(transport);
  await openSettings("Limits");
  const profile = screen.getByLabelText("Profile") as HTMLSelectElement;
  await screen.findByRole("option", { name: "Client A" });
  fireEvent.change(profile, { target: { value: "Client A" } });
  await waitFor(() => expect((screen.getByLabelText("Max URLs") as HTMLInputElement).value).toBe("100"));
  expect(screen.queryByText("Modified")).toBeNull();

  fireEvent.change(screen.getByLabelText("Max URLs"), { target: { value: "50" } });
  expect(screen.getByText("Modified")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Revert" }));
  await waitFor(() => expect((screen.getByLabelText("Max URLs") as HTMLInputElement).value).toBe("100"));
  expect(screen.queryByText("Modified")).toBeNull();
});

async function saveAs(name: string) {
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Save as new profile…" }));
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: name } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
}

test("saving_an_existing_name_asks_to_overwrite", async () => {
  const transport = await setup();
  await openSettings();
  await saveAs("Weekly");
  await waitFor(() => expect(transport.saved.has("Weekly")).toBe(true));
  await waitFor(() => expect(screen.queryByLabelText("New profile name")).toBeNull());
  await saveAs("Weekly");
  expect((await screen.findByText(/already exists/)).textContent).toContain("Weekly");
  fireEvent.click(screen.getByRole("button", { name: "Overwrite" }));
  await waitFor(() => expect(screen.getByText('Saved "Weekly".')).toBeTruthy());
});

test("list_mode_shows_counts_and_invalid_lines", async () => {
  await setup();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  fireEvent.change(await screen.findByRole("textbox", { name: "URL list" }), {
    target: { value: "https://a.test/1\nhttps://a.test/1\nhttps://a.test/2\nht!tp//bad" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Load pasted URLs" }));
  const summary = await screen.findByTestId("list-summary");
  expect(summary.textContent).toContain("2 URLs to crawl");
  expect(summary.textContent).toContain("1 duplicates removed");
  expect(summary.textContent).toContain("1 invalid");
  expect(summary.textContent).toContain("ht!tp//bad");
  fireEvent.click(screen.getByRole("tab", { name: /^Limits/ }));
  expect((screen.getByLabelText("Max depth") as HTMLInputElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  expect(await screen.findByRole("button", { name: "2 URLs loaded" })).toBeTruthy();
});

test("sheet_opens_on_speed_first_and_remembers_category", async () => {
  await setup();
  await openSettings();
  expect(screen.getByRole("tab", { name: /^Speed/ }).getAttribute("aria-selected")).toBe("true");
  fireEvent.click(screen.getByRole("tab", { name: /^Scope/ }));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(screen.queryByRole("navigation", { name: "Settings categories" })).toBeNull());
  await openSettings();
  expect(screen.getByRole("tab", { name: /^Scope/ }).getAttribute("aria-selected")).toBe("true");
});

test("category_shows_count_of_its_issues", async () => {
  // US2-2: counts equal the engine's validation output for that category.
  await setup();
  await openSettings("robots.txt");
  fireEvent.change(screen.getByLabelText("robots.txt behaviour"), { target: { value: "ignore" } });
  await waitFor(() => expect(screen.getByRole("tab", { name: /^robots\.txt/ }).textContent).toContain("1 warning"));
  expect(screen.getByRole("tab", { name: /^Limits/ }).textContent).toBe("Limits");
});

test("settings_shortcut_opens_and_escape_closes", async () => {
  await setup();
  act(() => {
    fireEvent.keyDown(window, { key: ",", code: "Comma", ctrlKey: true });
  });
  expect(await screen.findByRole("navigation", { name: "Settings categories" })).toBeTruthy();
  fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("navigation", { name: "Settings categories" })).toBeNull());
});
