import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { defaultConfig, fakeTransport } from "../testing/fakeTransport";
import { SettingsScreen } from "./SettingsScreen";

async function setup() {
  const transport = fakeTransport();
  render(<SettingsScreen transport={transport} />);
  await screen.findByLabelText("Start URL");
  return transport;
}

const start = () => screen.getByRole("button", { name: "Start" }) as HTMLButtonElement;

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
});

test("less_respectful_settings_are_labelled", async () => {
  // SC-009: every less-respectful choice shows a visible warning.
  await setup();
  fireEvent.change(screen.getByLabelText("robots.txt behaviour"), { target: { value: "ignore" } });
  fireEvent.change(screen.getByLabelText("User agent"), { target: { value: "googlebot_smartphone" } });
  fireEvent.change(screen.getByLabelText("Speed preset"), { target: { value: "fast" } });
  const summary = await screen.findByLabelText("Less respectful settings");
  await waitFor(() => {
    expect(summary.textContent).toContain("robots.txt will be crawled");
    expect(summary.textContent).toContain("no longer identify itself as SpiderLens");
    expect(summary.textContent).toContain("intended for sites you control");
  });
});

test("editing_a_speed_value_shows_custom", async () => {
  await setup();
  const preset = screen.getByLabelText("Speed preset") as HTMLSelectElement;
  await waitFor(() => expect(preset.value).toBe("polite"));
  fireEvent.change(screen.getByLabelText("Max requests per site"), { target: { value: "3" } });
  await waitFor(() => expect(preset.value).toBe("custom"));
});

test("loaded_profile_shows_modified_and_reverts", async () => {
  const transport = fakeTransport();
  const config = defaultConfig();
  config.limits.max_urls = 100;
  transport.saved.set("Client A", { name: "Client A", config, start_url: null, ignored: [] });
  render(<SettingsScreen transport={transport} />);
  const profile = (await screen.findByLabelText("Profile")) as HTMLSelectElement;
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

test("saving_an_existing_name_asks_to_overwrite", async () => {
  const transport = await setup();
  fireEvent.change(screen.getByLabelText("New profile name"), { target: { value: "Weekly" } });
  fireEvent.click(screen.getByRole("button", { name: "Save as new" }));
  await waitFor(() => expect(transport.saved.has("Weekly")).toBe(true));
  fireEvent.click(screen.getByRole("button", { name: "Save as new" }));
  expect((await screen.findByText(/already exists/)).textContent).toContain("Weekly");
  expect(screen.getByRole("button", { name: "Overwrite" })).toBeTruthy();
});

test("list_mode_shows_counts_and_invalid_lines", async () => {
  await setup();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.change(screen.getByLabelText("URL list"), {
    target: { value: "https://a.test/1\nhttps://a.test/1\nhttps://a.test/2\nht!tp//bad" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Load pasted URLs" }));
  const summary = await screen.findByTestId("list-summary");
  expect(summary.textContent).toContain("2 URLs to crawl");
  expect(summary.textContent).toContain("1 duplicates removed");
  expect(summary.textContent).toContain("1 invalid");
  expect(summary.textContent).toContain("ht!tp//bad");
  expect((screen.getByLabelText("Max depth") as HTMLInputElement).disabled).toBe(true);
});
