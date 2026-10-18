// Spec 016 US1 (FR-020 to FR-022, FR-028): Settings › AI assistants.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeActivity, fakeExecutable } from "../testing/fakeAgents";
import { fakeTransport } from "../testing/fakeTransport";

async function openAssistants(transport = fakeTransport()) {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: "AI assistants" }));
  const panel = await screen.findByRole("tabpanel", { name: "AI assistants" });
  await within(panel).findByRole("switch", { name: "Allow AI assistants to read my crawls" });
  return { transport, nav, panel };
}

test("ai_assistants_is_an_app_setting_and_off_by_default", async () => {
  const { nav, panel } = await openAssistants();
  const tab = within(nav).getByRole("tab", { name: "AI assistants" });
  expect(tab.closest("ul")?.textContent).toMatch(/^App/);
  expect(within(panel).getByText("Saved on this computer")).toBeTruthy();
  const sw = within(panel).getByRole("switch", { name: "Allow AI assistants to read my crawls" });
  expect(sw.getAttribute("aria-checked")).toBe("false");
  expect(within(panel).getByText(/text written to mislead an AI/)).toBeTruthy();
});

test("turning_read_access_on_saves_it", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(within(panel).getByRole("switch", { name: "Allow AI assistants to read my crawls" }));
  await waitFor(() => expect(transport.assistants.saves.at(-1)?.allow_read).toBe(true));
  expect(within(panel).getByRole("switch", { name: "Allow AI assistants to read my crawls" }).getAttribute("aria-checked")).toBe("true");
});

test("folders_are_added_and_removed", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(within(panel).getByRole("button", { name: /Add folder/ }));
  const list = await within(panel).findByRole("list", { name: "Allowed folders" });
  expect(list.textContent).toContain("/Users/me/Crawls");
  expect(transport.assistants.settings.allowed_folders).toEqual(["/Users/me/Crawls"]);
  fireEvent.click(within(list).getByRole("button", { name: "Remove /Users/me/Crawls" }));
  await waitFor(() => expect(within(panel).queryByRole("list", { name: "Allowed folders" })).toBeNull());
  expect(transport.assistants.settings.allowed_folders).toEqual([]);
});

test("a_folder_that_does_not_exist_is_refused_and_not_kept", async () => {
  const transport = fakeTransport();
  transport.assistants.folders = ["/gone"];
  transport.assistants.missing.add("/gone");
  const { panel } = await openAssistants(transport);
  fireEvent.click(within(panel).getByRole("button", { name: /Add folder/ }));
  expect((await within(panel).findByRole("alert")).textContent).toContain("/gone is not a folder on this computer.");
  expect(within(panel).queryByRole("list", { name: "Allowed folders" })).toBeNull();
});

test("snippets_use_this_app_and_switch_per_assistant", async () => {
  const { panel } = await openAssistants();
  const desktop = within(panel).getByLabelText("Claude Desktop configuration");
  const config = JSON.parse(desktop.textContent!);
  expect(config.mcpServers.spiderlens).toEqual({ command: fakeExecutable, args: ["mcp"] });
  expect(within(panel).getByText(/Settings › Developer › Edit Config/)).toBeTruthy();
  fireEvent.click(within(panel).getByRole("radio", { name: "Claude Code" }));
  expect(within(panel).getByLabelText("Claude Code configuration").textContent).toBe(`claude mcp add spiderlens -- ${fakeExecutable} mcp`);
});

test("copy_puts_the_snippet_on_the_clipboard", async () => {
  const writes: string[] = [];
  Object.defineProperty(navigator, "clipboard", { value: { writeText: async (t: string) => void writes.push(t) }, configurable: true });
  const { panel } = await openAssistants();
  fireEvent.click(within(panel).getByRole("radio", { name: "Claude Code" }));
  fireEvent.click(within(panel).getByRole("button", { name: "Copy" }));
  await within(panel).findByRole("button", { name: "Copied" });
  expect(writes).toEqual([`claude mcp add spiderlens -- ${fakeExecutable} mcp`]);
});

test("test_reports_the_server_or_why_it_failed", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(within(panel).getByRole("button", { name: "Test" }));
  expect((await within(panel).findByRole("status")).textContent).toMatch(/The server works: 8 tools, protocol 2026-07-28\. Read access is off/);
  transport.assistants.testError = "The MCP server did not answer within 10 seconds.";
  fireEvent.click(within(panel).getByRole("button", { name: "Test" }));
  await waitFor(() => expect(within(panel).getByRole("status").textContent).toBe("The MCP server did not answer within 10 seconds."));
});

test("activity_lists_recent_calls_and_clears", async () => {
  const transport = fakeTransport();
  transport.assistants.activity = fakeActivity(30);
  const { panel } = await openAssistants(transport);
  const list = await within(panel).findByRole("list", { name: "Recent assistant activity" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows).toHaveLength(20);
  expect(rows[0].textContent).toContain("claude-code 2.4");
  expect(rows[0].textContent).toContain("query_pages");
  expect(within(rows[1]).getByText("Refused")).toBeTruthy();
  fireEvent.click(within(panel).getByRole("button", { name: "Clear activity" }));
  expect(await within(panel).findByText("No assistant has used SpiderLens yet.")).toBeTruthy();
});

// Spec 016 US6 (FR-026): assistants starting crawls.
const crawlSwitch = (panel: HTMLElement) => within(panel).getByRole("switch", { name: "Allow AI assistants to start crawls" });

test("starting_crawls_is_off_by_default_with_a_plain_risk_note_and_profiles_locked", async () => {
  const { panel, transport } = await openAssistants();
  expect(crawlSwitch(panel).getAttribute("aria-checked")).toBe("false");
  expect(within(panel).getByText(/send requests to websites from this computer/)).toBeTruthy();
  expect(within(panel).getByText(/marked “Started by an AI assistant”/)).toBeTruthy();
  const list = within(panel).getByRole("list", { name: "Allowed profiles" });
  for (const sw of within(list).getAllByRole("switch")) expect((sw as HTMLButtonElement).disabled || sw.getAttribute("data-disabled") !== null).toBe(true);
  expect(transport.assistants.saves).toEqual([]);
});

test("turning_crawling_on_and_allowing_a_profile_saves_both", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(crawlSwitch(panel));
  await waitFor(() => expect(transport.assistants.settings.allow_crawl).toBe(true));
  fireEvent.click(within(panel).getByRole("switch", { name: "Allow Polite" }));
  await waitFor(() => expect(transport.assistants.settings.allowed_profiles).toEqual(["Polite"]));
  expect(within(panel).getByRole("switch", { name: "Allow Polite" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.click(within(panel).getByRole("switch", { name: "Allow Polite" }));
  await waitFor(() => expect(transport.assistants.settings.allowed_profiles).toEqual([]));
});

test("a_profile_that_ignores_robots_txt_is_disabled_with_the_reason", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(crawlSwitch(panel));
  await waitFor(() => expect(transport.assistants.settings.allow_crawl).toBe(true));
  const rude = within(panel).getByRole("switch", { name: "Allow Stress test" });
  expect(rude.hasAttribute("disabled") || rude.getAttribute("data-disabled") !== null).toBe(true);
  expect(rude.closest("li")?.textContent).toContain("Ignores robots.txt");
  fireEvent.click(rude);
  expect(transport.assistants.settings.allowed_profiles).toEqual([]);
});

test("a_refusal_from_the_app_is_shown_and_the_switch_goes_back", async () => {
  const transport = fakeTransport();
  // The app refuses a profile that was changed to ignore robots.txt after it was allowed.
  transport.assistants.settings = { allow_read: false, allowed_folders: [], allow_crawl: false, allowed_profiles: ["Stress test"] };
  const { panel } = await openAssistants(transport);
  fireEvent.click(crawlSwitch(panel));
  expect((await within(panel).findByRole("alert")).textContent).toContain('The profile "Stress test" ignores robots.txt');
  await waitFor(() => expect(crawlSwitch(panel).getAttribute("aria-checked")).toBe("false"));
  expect(transport.assistants.settings.allow_crawl).toBe(false);
});

test("without_saved_profiles_the_picker_says_so", async () => {
  const transport = fakeTransport();
  transport.assistants.profiles = [];
  const { panel } = await openAssistants(transport);
  expect(within(panel).getByText(/No saved profiles yet/)).toBeTruthy();
  expect(within(panel).queryByRole("list", { name: "Allowed profiles" })).toBeNull();
});

test("the_test_reports_the_crawl_tools_when_crawling_is_on", async () => {
  const { panel, transport } = await openAssistants();
  fireEvent.click(crawlSwitch(panel));
  await waitFor(() => expect(transport.assistants.settings.allow_crawl).toBe(true));
  fireEvent.click(within(panel).getByRole("button", { name: "Test" }));
  expect((await within(panel).findByRole("status")).textContent).toMatch(/13 tools.*Starting crawls is on/);
});
