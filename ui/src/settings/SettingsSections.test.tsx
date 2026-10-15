// Every settings category, the headers editor, pattern tester, profile menu and
// list upload, each checked through to what the crawl or transport receives
// (spec 002 FR-010..FR-013, spec 017 US2).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { vi } from "vitest";
import { App } from "../App";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { ListSummary } from "../transport";
import { defaultConfig, fakeTransport } from "../testing/fakeTransport";

type T = ReturnType<typeof fakeTransport>;

async function setup(transport: T = fakeTransport()) {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  return transport;
}

const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const sw = (name: string) => screen.getByRole("switch", { name });
const statusBar = () => document.querySelector('footer[aria-label="Crawl status"]')!;
const nav = () => screen.queryByRole("navigation", { name: "Settings categories" });

async function openSettings(category?: string) {
  let n = nav();
  if (!n) {
    fireEvent.click(screen.getByRole("button", { name: "Configure" }));
    n = await screen.findByRole("navigation", { name: "Settings categories" });
  }
  if (category) fireEvent.click(within(n).getByRole("tab", { name: new RegExp(`^${category.replace(/[.]/g, "\\.")}`) }));
  return n;
}

async function closeSettings() {
  if (!nav()) return;
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
}

/** Closes the sheet, starts a spider crawl of example.com and returns its config. */
async function startCrawl(t: T): Promise<CrawlConfig> {
  await closeSettings();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  return t.crawls.started[0].config;
}

async function profileAction(name: string) {
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  const item = await screen.findByRole("menuitem", { name });
  fireEvent.click(item);
}

/** A transport with "Client A" saved (Max URLs 100) and loaded in the sheet. */
async function withLoadedProfile(t: T = fakeTransport()) {
  const config = defaultConfig();
  config.limits.max_urls = 100;
  t.saved.set("Client A", { name: "Client A", config, start_url: null, ignored: [] });
  await setup(t);
  await openSettings("Limits");
  await screen.findByRole("option", { name: "Client A" });
  fireEvent.change(screen.getByLabelText("Profile"), { target: { value: "Client A" } });
  await waitFor(() => expect(input("Max URLs").value).toBe("100"));
  return t;
}

const summary = (urls: number): ListSummary => ({ urls, duplicates: 0, invalid_count: 0, invalid_sample: [], notices: [], first_urls: [] });

// ---------------------------------------------------------------- categories

test("every_category_renders_its_fields", async () => {
  // FR-010: each category shows its settings as rows.
  await setup();
  const expected: Record<string, string[]> = {
    Limits: ["Max URLs", "Max depth", "List depth"],
    Scope: ["Include patterns", "Exclude patterns", "Subdomains", "Query parameters", "Max URL length", "Sample URLs"],
    Identity: ["User agent"],
    Speed: ["Max concurrent requests", "Max requests per site", "Max URLs per second"],
    "robots.txt": ["robots.txt behaviour"],
    "Network and retries": [
      "Connect timeout (seconds)",
      "Read timeout (seconds)",
      "Max redirect hops",
      "Max page size (MB)",
      "Attempts per URL",
      "Max wait between attempts (seconds)",
    ],
  };
  const switches: Record<string, string[]> = {
    Scope: ["Check external links", "Follow internal nofollow links", "Follow external nofollow links", "Stay in the start folder", "Read sitemaps", "Crawl URLs found only in sitemaps", "Sort parameters"],
    Speed: ["Slow down automatically when a site struggles"],
    "robots.txt": ["Honour crawl-delay"],
    "Network and retries": ["Retry server errors (5xx)", "Retry rate limiting (429)"],
  };
  for (const [category, labels] of Object.entries(expected)) {
    await openSettings(category);
    expect(screen.getByRole("tabpanel", { name: category })).toBeTruthy();
    for (const l of labels) expect(screen.getByLabelText(l)).toBeTruthy();
    for (const s of switches[category] ?? []) expect(sw(s)).toBeTruthy();
  }
  await openSettings("Identity");
  expect(screen.getByRole("button", { name: /Add header/ })).toBeTruthy();
  await openSettings("Speed");
  expect(screen.getByRole("radiogroup", { name: "Speed preset" })).toBeTruthy();
});

test("url_list_category_appears_only_in_list_mode", async () => {
  await setup();
  const n = await openSettings();
  expect(within(n).queryByRole("tab", { name: /^URL list/ })).toBeNull();
  expect(within(n).getAllByRole("tab").map((x) => x.textContent)).toEqual(["Limits", "Scope", "Identity", "Speed", "robots.txt", "Network and retries", "Sign-in and cookies", "Page data", "Segments", "Link analysis", "Issues", "Custom rules", "Storage", "Spelling", "JavaScript rendering", "Appearance", "AI assistants"]);
  await closeSettings();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  const n2 = await screen.findByRole("navigation", { name: "Settings categories" });
  expect(within(n2).getByRole("tab", { name: /^URL list/ }).getAttribute("aria-selected")).toBe("true");
});

test("done_button_closes_the_sheet", async () => {
  await setup();
  await openSettings("Limits");
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
});

test("category_tabs_are_focusable_buttons_and_focus_lands_on_the_selected_one", async () => {
  // Keyboard users reach the categories: initial focus is the current tab, and
  // tabs are native buttons so Enter activates them (jsdom does not synthesise
  // the click a browser sends on Enter, so it is sent here).
  await setup();
  const n = await openSettings();
  const speed = within(n).getByRole("tab", { name: /^Speed/ });
  await waitFor(() => expect(document.activeElement).toBe(speed));
  const limits = within(n).getByRole("tab", { name: /^Limits/ });
  expect(limits.tagName).toBe("BUTTON");
  expect(limits.getAttribute("type")).toBe("button");
  limits.focus();
  fireEvent.keyDown(limits, { key: "Enter" });
  fireEvent.click(limits);
  expect(limits.getAttribute("aria-selected")).toBe("true");
  expect(screen.getByRole("tabpanel", { name: "Limits" })).toBeTruthy();
});

// ---------------------------------------------------------------- limits

test("limits_reach_the_crawl_and_blank_max_urls_means_no_limit", async () => {
  const t = await setup();
  await openSettings("Limits");
  fireEvent.change(input("Max URLs"), { target: { value: "500" } });
  fireEvent.change(input("Max URLs"), { target: { value: "" } });
  fireEvent.change(input("Max depth"), { target: { value: "3" } });
  expect(input("List depth").disabled).toBe(true);
  const config = await startCrawl(t);
  expect(config.limits).toEqual({ max_urls: null, max_depth: 3, list_depth: 0 });
});

test("list_depth_is_enabled_in_list_mode_and_reaches_the_crawl", async () => {
  const t = await setup();
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  fireEvent.change(await screen.findByRole("textbox", { name: "URL list" }), { target: { value: "https://a.test/1\nhttps://a.test/2" } });
  fireEvent.click(screen.getByRole("button", { name: "Load pasted URLs" }));
  await screen.findByTestId("list-summary");
  await openSettings("Limits");
  expect(input("Max depth").disabled).toBe(true);
  expect(input("List depth").disabled).toBe(false);
  fireEvent.change(input("List depth"), { target: { value: "2" } });
  await closeSettings();
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].startUrl).toBeNull();
  expect(t.crawls.started[0].config.mode).toBe("list");
  expect(t.crawls.started[0].config.limits.list_depth).toBe(2);
});

// ---------------------------------------------------------------- identity

test("preset_user_agent_string_is_shown_as_help", async () => {
  await setup();
  await openSettings("Identity");
  expect(screen.getByText("SpiderLens/0.1.0").tagName).toBe("CODE");
  fireEvent.change(screen.getByLabelText("User agent"), { target: { value: "googlebot_smartphone" } });
  expect(screen.getByText("Googlebot").tagName).toBe("CODE");
  expect(screen.queryByText("SpiderLens/0.1.0")).toBeNull();
});

test("custom_user_agent_reveals_its_fields_and_reaches_the_crawl", async () => {
  const t = await setup();
  await openSettings("Identity");
  expect(screen.queryByLabelText("Custom user agent")).toBeNull();
  expect(screen.queryByLabelText("robots.txt token")).toBeNull();
  fireEvent.change(screen.getByLabelText("User agent"), { target: { value: "custom" } });
  fireEvent.change(await screen.findByLabelText("Custom user agent"), { target: { value: "AuditBot/2.0" } });
  fireEvent.change(screen.getByLabelText("robots.txt token"), { target: { value: "AuditBot" } });
  const config = await startCrawl(t);
  expect(config.identity.user_agent).toEqual({ preset: "custom", custom_string: "AuditBot/2.0", robots_token: "AuditBot" });
});

test("headers_editor_adds_secret_all_hosts_and_removes_headers", async () => {
  const t = await setup();
  await openSettings("Identity");
  const add = () => fireEvent.click(screen.getByRole("button", { name: /Add header/ }));
  add();
  fireEvent.change(input("Header 1 name"), { target: { value: "Authorization" } });
  fireEvent.change(input("Header 1 value"), { target: { value: "Bearer abc" } });
  expect(input("Header 1 value").type).toBe("text");
  fireEvent.click(sw("Header 1 secret"));
  await waitFor(() => expect(input("Header 1 value").type).toBe("password"));
  expect(input("Header 1 value").value).toBe("Bearer abc");
  expect(sw("Header 1 all hosts").getAttribute("aria-checked")).toBe("false");
  fireEvent.click(sw("Header 1 all hosts"));
  await waitFor(() => expect(sw("Header 1 all hosts").getAttribute("aria-checked")).toBe("true"));

  add();
  fireEvent.change(input("Header 2 name"), { target: { value: "X-Drop" } });
  fireEvent.click(screen.getByRole("button", { name: "Remove header 2" }));
  expect(screen.queryByLabelText("Header 2 name")).toBeNull();

  add();
  fireEvent.change(input("Header 2 name"), { target: { value: "X-Env" } });
  fireEvent.change(input("Header 2 value"), { target: { value: "staging" } });

  const config = await startCrawl(t);
  expect(config.identity.headers).toEqual([
    { name: "Authorization", value: { secret: true, value: "Bearer abc" }, send_to: "all" },
    { name: "X-Env", value: { plain: "staging" }, send_to: "internal" },
  ]);
});

test("removing_the_only_header_leaves_none", async () => {
  const t = await setup();
  await openSettings("Identity");
  fireEvent.click(screen.getByRole("button", { name: /Add header/ }));
  fireEvent.click(screen.getByRole("button", { name: "Remove header 1" }));
  expect(screen.queryByLabelText("Header 1 name")).toBeNull();
  const config = await startCrawl(t);
  expect(config.identity.headers).toEqual([]);
});

// ---------------------------------------------------------------- speed

test("fast_and_polite_presets_set_all_four_values", async () => {
  await setup();
  await openSettings("Speed");
  fireEvent.click(screen.getByRole("radio", { name: "Fast" }));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Fast" }).getAttribute("aria-checked")).toBe("true"));
  expect(input("Max concurrent requests").value).toBe("64");
  expect(input("Max requests per site").value).toBe("32");
  expect(input("Max URLs per second").value).toBe("");
  expect(sw("Slow down automatically when a site struggles").getAttribute("aria-checked")).toBe("true");
  expect(screen.getByText("Fast is for sites you control.")).toBeTruthy();

  fireEvent.click(screen.getByRole("radio", { name: "Polite" }));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Polite" }).getAttribute("aria-checked")).toBe("true"));
  expect(input("Max concurrent requests").value).toBe("8");
  expect(input("Max requests per site").value).toBe("4");
  expect(input("Max URLs per second").value).toBe("5");
  expect(screen.queryByText("Fast is for sites you control.")).toBeNull();
});

test("fast_preset_reaches_the_crawl", async () => {
  const t = await setup();
  await openSettings("Speed");
  fireEvent.click(screen.getByRole("radio", { name: "Fast" }));
  const config = await startCrawl(t);
  expect(config.speed).toMatchObject({ max_concurrent: 64, max_per_host: 32, max_urls_per_sec: null, adaptive: true });
});

test("blank_urls_per_second_means_no_cap_and_adaptive_switch_reaches_the_crawl", async () => {
  const t = await setup();
  await openSettings("Speed");
  fireEvent.change(input("Max URLs per second"), { target: { value: "" } });
  expect(input("Max URLs per second").placeholder).toBe("No cap");
  fireEvent.click(sw("Slow down automatically when a site struggles"));
  await waitFor(() => expect(sw("Slow down automatically when a site struggles").getAttribute("aria-checked")).toBe("false"));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Custom" }).getAttribute("aria-checked")).toBe("true"));
  fireEvent.change(input("Max concurrent requests"), { target: { value: "6" } });
  const config = await startCrawl(t);
  expect(config.speed).toMatchObject({ max_concurrent: 6, max_per_host: 4, max_urls_per_sec: null, adaptive: false });
});

// ---------------------------------------------------------------- robots.txt

test("robots_mode_and_crawl_delay_reach_the_crawl_and_status_bar", async () => {
  const t = await setup();
  await waitFor(() => expect(statusBar().textContent).toContain("robots.txt respected"));
  await openSettings("robots.txt");
  const select = screen.getByLabelText("robots.txt behaviour") as HTMLSelectElement;
  expect([...select.options].map((o) => [o.value, o.textContent])).toEqual([
    ["respect", "Respect"],
    ["ignore_report", "Ignore but report"],
    ["ignore", "Ignore"],
  ]);
  fireEvent.change(select, { target: { value: "ignore" } });
  await waitFor(() => expect(statusBar().textContent).toContain("robots.txt ignored"));
  expect(statusBar().textContent).not.toContain("reported");
  fireEvent.change(select, { target: { value: "ignore_report" } });
  await waitFor(() => expect(statusBar().textContent).toContain("robots.txt ignored, reported"));
  expect(sw("Honour crawl-delay").getAttribute("aria-checked")).toBe("true");
  fireEvent.click(sw("Honour crawl-delay"));
  await waitFor(() => expect(sw("Honour crawl-delay").getAttribute("aria-checked")).toBe("false"));
  const config = await startCrawl(t);
  expect(config.robots).toEqual({ mode: "ignore_report", honour_crawl_delay: false });
});

// ---------------------------------------------------------------- network

test("network_and_retry_settings_reach_the_crawl", async () => {
  const t = await setup();
  await openSettings("Network and retries");
  const values: [string, string][] = [
    ["Connect timeout (seconds)", "5"],
    ["Read timeout (seconds)", "45"],
    ["Max redirect hops", "3"],
    ["Max page size (MB)", "20"],
    ["Attempts per URL", "1"],
    ["Max wait between attempts (seconds)", "15"],
  ];
  for (const [l, v] of values) fireEvent.change(input(l), { target: { value: v } });
  fireEvent.click(sw("Retry server errors (5xx)"));
  fireEvent.click(sw("Retry rate limiting (429)"));
  await waitFor(() => expect(sw("Retry rate limiting (429)").getAttribute("aria-checked")).toBe("false"));
  const config = await startCrawl(t);
  expect(config.network).toEqual({ connect_timeout_secs: 5, read_timeout_secs: 45, max_redirects: 3, max_body_mb: 20 });
  expect(config.retry).toEqual({
    attempts: 1,
    max_wait_secs: 15,
    retry_5xx: false,
    retry_429: false,
    // Spec 004 settings, left at their defaults here.
    base_wait_secs: 1,
    host_pause_after: 5,
    host_pause_secs: 60,
    host_pause_max_secs: 600,
    host_give_up_after: 5,
  });
});

// ---------------------------------------------------------------- pattern tester

test("pattern_tester_shows_a_row_per_sample", async () => {
  const t = await setup();
  const spy = vi.spyOn(t, "testPatterns");
  await openSettings("Scope");
  fireEvent.change(screen.getByLabelText("Include patterns"), { target: { value: "/blog/" } });
  fireEvent.change(screen.getByLabelText("Sample URLs"), { target: { value: "https://a.test/blog/1\nhttps://a.test/about" } });
  fireEvent.click(screen.getByRole("button", { name: "Test patterns" }));
  const badges = await screen.findAllByText("Crawled");
  expect(badges).toHaveLength(2);
  expect(spy).toHaveBeenCalledWith(["/blog/"], [], ["https://a.test/blog/1", "https://a.test/about"]);
  const table = badges[0].closest("table")!;
  const rows = within(table).getAllByRole("row").slice(1);
  expect(rows.map((r) => within(r).getAllByRole("cell")[0].textContent)).toEqual(["https://a.test/blog/1", "https://a.test/about"]);
});

// ---------------------------------------------------------------- profiles

test("status_bar_shows_the_loaded_profile_name", async () => {
  await withLoadedProfile();
  await waitFor(() => expect(statusBar().textContent).toContain("Client A · "));
});

test("save_with_include_start_url_stores_the_start_url", async () => {
  const t = await setup();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await openSettings();
  await profileAction("Save as new profile…");
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: "With URL" } });
  expect(sw("Include start URL").getAttribute("aria-checked")).toBe("false");
  fireEvent.click(sw("Include start URL"));
  await waitFor(() => expect(sw("Include start URL").getAttribute("aria-checked")).toBe("true"));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(t.saved.get("With URL")?.start_url).toBe("example.com"));
  expect(await screen.findByText('Saved "With URL".')).toBeTruthy();
  expect((screen.getByLabelText("Profile") as HTMLSelectElement).value).toBe("With URL");
});

test("save_without_include_start_url_stores_none", async () => {
  const t = await setup();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await openSettings();
  await profileAction("Save as new profile…");
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: "No URL" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(t.saved.has("No URL")).toBe(true));
  expect(t.saved.get("No URL")!.start_url).toBeNull();
});

test("save_button_is_disabled_for_a_blank_name", async () => {
  const t = await setup();
  const save = vi.spyOn(t, "saveProfile");
  await openSettings();
  await profileAction("Save as new profile…");
  const name = await screen.findByLabelText("New profile name");
  const button = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
  expect(button().disabled).toBe(true);
  fireEvent.change(name, { target: { value: "   " } });
  expect(button().disabled).toBe(true);
  fireEvent.submit(name.closest("form")!);
  fireEvent.change(name, { target: { value: "Ok" } });
  expect(button().disabled).toBe(false);
  expect(save).not.toHaveBeenCalled();
});

test("cancel_closes_the_name_dialog", async () => {
  const t = await setup();
  const save = vi.spyOn(t, "saveProfile");
  await openSettings();
  await profileAction("Save as new profile…");
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: "Nope" } });
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByLabelText("New profile name")).toBeNull());
  expect(save).not.toHaveBeenCalled();
  expect(nav()).toBeTruthy();
});

test("transport_error_during_save_shows_an_alert", async () => {
  const t = await setup();
  t.saveProfile = async () => {
    throw new Error("disk full");
  };
  await openSettings();
  await profileAction("Save as new profile…");
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: "Weekly" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  // The notice sits in the picker, under the still-open name dialog.
  await waitFor(() => expect(screen.getAllByRole("alert", { hidden: true }).some((a) => a.textContent?.includes("disk full"))).toBe(true));
  expect(screen.getByLabelText("New profile name")).toBeTruthy();
});

test("profile_actions_need_a_loaded_profile", async () => {
  await setup();
  await openSettings();
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  for (const name of ["Rename…", "Duplicate…", "Make default", "Export…", "Delete profile"]) {
    expect((await screen.findByRole("menuitem", { name })).getAttribute("aria-disabled")).toBe("true");
  }
  for (const name of ["Save as new profile…", "Import…", "Reset to defaults"]) {
    expect(screen.getByRole("menuitem", { name }).getAttribute("aria-disabled")).not.toBe("true");
  }
});

test("profile_actions_are_enabled_with_a_loaded_profile", async () => {
  await withLoadedProfile();
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  for (const name of ["Rename…", "Duplicate…", "Make default", "Export…", "Delete profile"]) {
    expect((await screen.findByRole("menuitem", { name })).getAttribute("aria-disabled")).not.toBe("true");
  }
});

test("rename_calls_transport_and_shows_the_new_name", async () => {
  const t = fakeTransport();
  const rename = vi.fn(async (from: string, to: string) => {
    const p = t.saved.get(from)!;
    t.saved.delete(from);
    t.saved.set(to, { ...p, name: to });
  });
  t.renameProfile = rename;
  await withLoadedProfile(t);
  await profileAction("Rename…");
  const name = (await screen.findByLabelText("New profile name")) as HTMLInputElement;
  expect(name.value).toBe("Client A");
  expect(screen.queryByRole("switch", { name: "Include start URL" })).toBeNull();
  fireEvent.change(name, { target: { value: "Client B" } });
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  await waitFor(() => expect(rename).toHaveBeenCalledWith("Client A", "Client B"));
  await waitFor(() => expect(screen.queryByLabelText("New profile name")).toBeNull());
  await waitFor(() => expect((screen.getByLabelText("Profile") as HTMLSelectElement).value).toBe("Client B"));
  expect(screen.queryByRole("option", { name: "Client A" })).toBeNull();
  await waitFor(() => expect(statusBar().textContent).toContain("Client B · "));
});

test("duplicate_calls_transport_with_both_names", async () => {
  const t = fakeTransport();
  const dup = vi.fn(async (from: string, to: string) => {
    t.saved.set(to, { ...t.saved.get(from)!, name: to });
  });
  t.duplicateProfile = dup;
  await withLoadedProfile(t);
  await profileAction("Duplicate…");
  const name = (await screen.findByLabelText("New profile name")) as HTMLInputElement;
  expect(name.value).toBe("Client A");
  fireEvent.change(name, { target: { value: "Client A copy" } });
  fireEvent.click(screen.getByRole("button", { name: "Duplicate" }));
  await waitFor(() => expect(dup).toHaveBeenCalledWith("Client A", "Client A copy"));
  await waitFor(() => expect(screen.queryByLabelText("New profile name")).toBeNull());
  expect(await screen.findByRole("option", { name: "Client A copy" })).toBeTruthy();
  // The original stays loaded.
  expect((screen.getByLabelText("Profile") as HTMLSelectElement).value).toBe("Client A");
});

test("make_default_calls_transport", async () => {
  const t = fakeTransport();
  const def = vi.fn(async () => {});
  t.setDefaultProfile = def;
  await withLoadedProfile(t);
  await profileAction("Make default");
  await waitFor(() => expect(def).toHaveBeenCalledWith("Client A"));
});

test("default_profile_is_marked_in_the_list", async () => {
  const t = fakeTransport();
  t.saved.set("Client A", { name: "Client A", config: defaultConfig(), start_url: null, ignored: [] });
  t.profiles = async () => [
    { name: "Client A", is_default: true, error: null },
    { name: "Broken", is_default: false, error: "unreadable file" },
  ];
  await setup(t);
  await openSettings();
  expect(await screen.findByRole("option", { name: "Client A (default)" })).toBeTruthy();
  expect((screen.getByRole("option", { name: "Broken (unreadable)" }) as HTMLOptionElement).disabled).toBe(true);
});

test("export_picks_a_path_then_exports", async () => {
  const t = fakeTransport();
  const pick = vi.fn(async (name: string) => `/exports/${name}.json`);
  const exp = vi.fn(async () => {});
  t.pickExportPath = pick;
  t.exportProfile = exp;
  await withLoadedProfile(t);
  await profileAction("Export…");
  await waitFor(() => expect(exp).toHaveBeenCalledWith("Client A", "/exports/Client A.json"));
  expect(pick).toHaveBeenCalledWith("Client A");
});

test("export_does_nothing_when_the_picker_is_cancelled", async () => {
  const t = fakeTransport();
  const pick = vi.fn(async () => null);
  const exp = vi.fn(async () => {});
  t.pickExportPath = pick;
  t.exportProfile = exp;
  await withLoadedProfile(t);
  await profileAction("Export…");
  await waitFor(() => expect(pick).toHaveBeenCalled());
  await act(async () => {});
  expect(exp).not.toHaveBeenCalled();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("import_applies_settings_and_reports_ignored_ones", async () => {
  const t = await setup();
  const config = defaultConfig();
  config.limits.max_urls = 77;
  config.robots.mode = "ignore";
  t.pickProfileFile = async () => "/profiles/old.json";
  const imp = vi.fn(async () => ({ name: "Old export", config, start_url: "imported.test", ignored: ["render.js", "extract.css"] }));
  t.importProfile = imp;
  await openSettings("Limits");
  await profileAction("Import…");
  const msg = await screen.findByText(/Settings this version does not know were ignored/);
  expect(msg.textContent).toBe('Imported "Old export". Settings this version does not know were ignored: render.js, extract.css.');
  expect(imp).toHaveBeenCalledWith("/profiles/old.json");
  expect(input("Max URLs").value).toBe("77");
  expect(input("Start URL").value).toBe("imported.test");
  await waitFor(() => expect(statusBar().textContent).toContain("Old export · "));
  expect(statusBar().textContent).toContain("robots.txt ignored");
});

test("import_without_ignored_settings_says_imported", async () => {
  const t = await setup();
  t.pickProfileFile = async () => "/profiles/new.json";
  t.importProfile = async () => ({ name: "Fresh", config: defaultConfig(), start_url: null, ignored: [] });
  await openSettings();
  await profileAction("Import…");
  expect(await screen.findByText('Imported "Fresh".')).toBeTruthy();
});

test("import_cancelled_does_nothing", async () => {
  const t = await setup();
  const imp = vi.fn(t.importProfile);
  t.importProfile = imp;
  const pick = vi.fn(async () => null);
  t.pickProfileFile = pick;
  await openSettings();
  await profileAction("Import…");
  await waitFor(() => expect(pick).toHaveBeenCalled());
  await act(async () => {});
  expect(imp).not.toHaveBeenCalled();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("import_error_shows_an_alert", async () => {
  const t = await setup();
  t.pickProfileFile = async () => "/profiles/bad.json";
  t.importProfile = async () => {
    throw new Error("not a SpiderLens profile");
  };
  await openSettings();
  await profileAction("Import…");
  expect((await screen.findByRole("alert")).textContent).toContain("not a SpiderLens profile");
});

test("delete_profile_removes_it_and_resets_to_defaults", async () => {
  const t = await withLoadedProfile();
  await profileAction("Delete profile");
  await waitFor(() => expect(t.saved.has("Client A")).toBe(false));
  await waitFor(() => expect(input("Max URLs").value).toBe(""));
  expect((screen.getByLabelText("Profile") as HTMLSelectElement).value).toBe("");
  await waitFor(() => expect(screen.queryByRole("option", { name: "Client A" })).toBeNull());
  await waitFor(() => expect(statusBar().textContent).not.toContain("Client A"));
});

test("reset_to_defaults_restores_defaults_and_unloads_the_profile", async () => {
  const t = fakeTransport();
  const defaults = vi.fn(t.defaults);
  t.defaults = defaults;
  await withLoadedProfile(t);
  fireEvent.change(input("Max depth"), { target: { value: "4" } });
  await profileAction("Reset to defaults");
  await waitFor(() => expect(input("Max URLs").value).toBe(""));
  expect(input("Max depth").value).toBe("");
  expect(defaults).toHaveBeenLastCalledWith(true);
  expect((screen.getByLabelText("Profile") as HTMLSelectElement).value).toBe("");
  expect(screen.queryByText("Modified")).toBeNull();
  // The profile itself is kept.
  expect(t.saved.has("Client A")).toBe(true);
});

test("loaded_profile_settings_reach_the_crawl", async () => {
  const t = await withLoadedProfile();
  const config = await startCrawl(t);
  expect(config.limits.max_urls).toBe(100);
});

// ---------------------------------------------------------------- list upload

async function openList() {
  fireEvent.click(screen.getByLabelText("List"));
  fireEvent.click(await screen.findByRole("button", { name: "Add a list of URLs…" }));
  await screen.findByRole("textbox", { name: "URL list" });
}

test("uploading_a_list_file_parses_it", async () => {
  const t = fakeTransport();
  t.pickListFile = async () => "/lists/urls.csv";
  const parse = vi.fn(async () => summary(1200));
  t.parseListFile = parse;
  await setup(t);
  await openList();
  fireEvent.click(screen.getByRole("button", { name: /Upload text or CSV file/ }));
  const s = await screen.findByTestId("list-summary");
  expect(parse).toHaveBeenCalledWith("/lists/urls.csv");
  expect(s.textContent).toContain("1,200 URLs to crawl");
  await closeSettings();
  expect(await screen.findByRole("button", { name: "1,200 URLs loaded" })).toBeTruthy();
});

test("cancelled_list_file_picker_does_nothing", async () => {
  const t = fakeTransport();
  const parse = vi.fn(async () => summary(1));
  t.parseListFile = parse;
  const pick = vi.fn(async () => null);
  t.pickListFile = pick;
  await setup(t);
  await openList();
  fireEvent.click(screen.getByRole("button", { name: /Upload text or CSV file/ }));
  await waitFor(() => expect(pick).toHaveBeenCalled());
  await act(async () => {});
  expect(parse).not.toHaveBeenCalled();
  expect(screen.queryByTestId("list-summary")).toBeNull();
});

test("list_file_error_shows_an_alert", async () => {
  const t = fakeTransport();
  t.pickListFile = async () => "/lists/huge.bin";
  t.parseListFile = async () => {
    throw new Error("The file is not text.");
  };
  await setup(t);
  await openList();
  fireEvent.click(screen.getByRole("button", { name: /Upload text or CSV file/ }));
  await waitFor(() => expect(screen.getAllByRole("alert").some((a) => a.textContent?.includes("The file is not text."))).toBe(true));
  expect(screen.queryByText(/Reading…/)).toBeNull();
});

test("list_file_shows_reading_progress", async () => {
  const t = fakeTransport();
  let report: ((n: number) => void) | undefined;
  t.onListProgress = async (cb) => {
    report = cb;
    return () => {};
  };
  let finish: ((s: ListSummary) => void) | undefined;
  t.pickListFile = async () => "/lists/big.txt";
  t.parseListFile = () => new Promise((resolve) => (finish = resolve));
  await setup(t);
  await openList();
  fireEvent.click(screen.getByRole("button", { name: /Upload text or CSV file/ }));
  expect((await screen.findByText(/Reading…/)).textContent).toBe("Reading… 0 lines");
  await waitFor(() => expect(report).toBeDefined());
  act(() => report!(25000));
  expect(screen.getByText(/Reading…/).textContent).toBe("Reading… 25,000 lines");
  await act(async () => finish!(summary(25000)));
  await waitFor(() => expect(screen.queryByText(/Reading…/)).toBeNull());
  expect((await screen.findByTestId("list-summary")).textContent).toContain("25,000 URLs to crawl");
});
