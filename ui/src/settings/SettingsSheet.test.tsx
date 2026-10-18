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
  const t = await setup();
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect((screen.getByLabelText("Start URL") as HTMLInputElement).value).toBe("https://example.com/"));
  await waitFor(() => expect(t.crawls.started.map((s) => s.startUrl)).toEqual(["https://example.com/"]));
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

test("save_error_is_announced_inside_the_name_dialog", async () => {
  const transport = fakeTransport();
  transport.saveProfile = async () => {
    throw new Error("disk is read-only");
  };
  await setup(transport);
  await openSettings();
  await saveAs("Weekly");
  const dialog = await screen.findByRole("dialog", { name: "Save as new profile" });
  const alert = await within(dialog).findByRole("alert");
  expect(alert.textContent).toContain("disk is read-only");
  // The dialog stays open so the name can be fixed or the save retried.
  expect(within(dialog).getByLabelText("New profile name")).toBeTruthy();
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByLabelText("New profile name")).toBeNull());
  // Reopening the dialog starts clean.
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Save as new profile…" }));
  const reopened = await screen.findByRole("dialog", { name: "Save as new profile" });
  expect(within(reopened).queryByRole("alert")).toBeNull();
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

test("spec_003_scope_and_limit_settings_reach_the_crawl", async () => {
  const t = await setup();
  await openSettings("Limits");
  expect((screen.getByLabelText("List depth") as HTMLInputElement).disabled).toBe(true);
  await openSettings("Scope");
  fireEvent.click(screen.getByRole("switch", { name: "Stay in the start folder" }));
  fireEvent.click(screen.getByRole("switch", { name: "Crawl URLs found only in sitemaps" }));
  expect(screen.queryByLabelText("Parameters to remove")).toBeNull();
  fireEvent.change(screen.getByLabelText("Query parameters"), { target: { value: "remove" } });
  fireEvent.change(screen.getByLabelText("Parameters to remove"), { target: { value: "utm_source\nutm_medium" } });
  fireEvent.click(screen.getByRole("switch", { name: "Sort parameters" }));
  fireEvent.change(screen.getByLabelText("Max URL length"), { target: { value: "1024" } });
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));

  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.scope).toMatchObject({
    stay_in_folder: true,
    sitemaps: { read: true, crawl_sitemap_only: false, urls: [], files: [] },
    max_url_length: 1024,
    query_params: { mode: "remove", names: ["utm_source", "utm_medium"], sort: true },
  });
});

test("scope_tester_explains_each_url_with_the_current_settings", async () => {
  // Spec 003 FR-016 in the window (spec 007 T041).
  await setup();
  await openSettings("Scope");
  expect(screen.getByText(/Enter a start URL first/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  await openSettings("Scope");
  fireEvent.change(screen.getByLabelText("Exclude patterns"), { target: { value: "/skip/" } });
  fireEvent.change(screen.getByLabelText("URLs to test"), { target: { value: "https://example.com/a\nhttps://example.com/skip/me" } });
  fireEvent.click(screen.getByRole("button", { name: "Test URLs" }));
  const rows = await screen.findAllByRole("row");
  expect(rows.filter((r) => r.getAttribute("data-verdict") === "crawl")).toHaveLength(1);
  const excluded = rows.find((r) => r.getAttribute("data-verdict") === "excluded")!;
  expect(within(excluded).getByText("Excluded by rule")).toBeTruthy();
  expect(within(excluded).getByText("/skip/")).toBeTruthy();
});

test("store_html_setting_reaches_the_crawl", async () => {
  const t = await setup();
  await openSettings("Page data");
  const toggle = screen.getByRole("switch", { name: "Store page HTML" });
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(toggle);
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.extraction.store_html).toBe(false);
  expect(t.crawls.started[0].config.extraction.check_amp).toBe(false);
});

// Spec 027 part C: the PDF settings.
test("pdf_settings_reach_the_crawl", async () => {
  const t = await setup();
  await openSettings("Page data");
  const crawl = screen.getByRole("switch", { name: "Crawl PDFs" });
  expect(crawl.getAttribute("aria-checked")).toBe("false");
  // The rest waits for PDFs to be crawled.
  expect((screen.getByLabelText("PDF size limit (MB)") as HTMLInputElement).disabled).toBe(true);
  fireEvent.click(crawl);
  fireEvent.click(screen.getByRole("switch", { name: "Follow links in PDFs" }));
  fireEvent.change(screen.getByLabelText("PDF size limit (MB)"), { target: { value: "50" } });
  fireEvent.change(screen.getByLabelText("Time to read one PDF (seconds)"), { target: { value: "10" } });
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.extraction.pdf).toEqual({ crawl: true, follow_links: false, max_mb: 50, time_limit_secs: 10 });
});

test("rendering_category_switches_the_mode", async () => {
  // Spec 014 T027: the mode switch writes rendering.mode and reaches the crawl.
  const t = await setup();
  await openSettings("JavaScript rendering");
  const raw = screen.getByRole("radio", { name: "Raw HTML" });
  expect(raw.getAttribute("aria-checked")).toBe("true");
  expect(screen.getByText("Pages are read from their HTML as served. No browser is started.")).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "JavaScript rendering" }));
  await waitFor(() => expect(screen.getByRole("radio", { name: "JavaScript rendering" }).getAttribute("aria-checked")).toBe("true"));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.rendering.mode).toBe("javascript");
});

test("rendering_issues_count_in_the_rendering_category", async () => {
  const transport = fakeTransport();
  const config = defaultConfig();
  config.rendering.max_render_secs = 0;
  transport.saved.set("Broken render", { name: "Broken render", config, start_url: null, ignored: [] });
  await setup(transport);
  await openSettings("Limits");
  await screen.findByRole("option", { name: "Broken render" });
  fireEvent.change(screen.getByLabelText("Profile"), { target: { value: "Broken render" } });
  await waitFor(() => expect(screen.getByRole("tab", { name: /^JavaScript rendering/ }).textContent).toContain("1 error"));
  expect(screen.getByRole("tab", { name: /^Limits/ }).textContent).toBe("Limits");
});

async function openRenderingOn(transport = fakeTransport()) {
  const t = await setup(transport);
  await openSettings("JavaScript rendering");
  fireEvent.click(screen.getByRole("radio", { name: "JavaScript rendering" }));
  await screen.findByLabelText("Maximum render time");
  return t;
}

const typeInto = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });
const toggle = (name: string) => fireEvent.click(screen.getByRole("switch", { name }));

test("rendering_category_each_control_writes_its_field", async () => {
  // Spec 014 T054: every setting of the category reaches the crawl's configuration.
  const t = await openRenderingOn();
  typeInto("Maximum render time", "20");
  typeInto("Minimum wait", "2");
  fireEvent.change(screen.getByLabelText("Window"), { target: { value: "custom" } });
  typeInto("Window width", "800");
  typeInto("Window height", "600");
  toggle("Block stylesheets");
  toggle("Block third-party resources");
  toggle("Render error pages");
  typeInto("Only render pages matching", "/products/\n/shop/");
  typeInto("Never render pages matching", "\\?print=");
  typeInto("Pages rendered at once", "3");
  typeInto("Browser memory budget", "2048");
  toggle("Screenshots");
  typeInto("JavaScript content: new text", "30");
  typeInto("JavaScript content: new words", "80");
  typeInto("Hidden text threshold", "60");
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.rendering).toEqual({
    mode: "javascript",
    max_render_secs: 20,
    min_wait_secs: 2,
    window: { preset: "custom", width: 800, height: 600 },
    block: { images: true, media: true, fonts: true, stylesheets: true, third_party: true },
    include: ["/products/", "/shop/"],
    exclude: ["\\?print="],
    render_error_pages: true,
    concurrency: 3,
    memory_budget_mb: 2048,
    screenshots: true,
    js_content_min_percent: 30,
    js_content_min_words: 80,
    hidden_content_percent: 60,
  });
});

test("rendering_category_load_all_resources_clears_the_blocks", async () => {
  const t = await openRenderingOn();
  const all = () => screen.getByRole("switch", { name: "Load all resources" });
  expect(all().getAttribute("aria-checked")).toBe("false");
  fireEvent.click(all());
  await waitFor(() => expect(all().getAttribute("aria-checked")).toBe("true"));
  for (const name of ["Block images", "Block video and audio", "Block fonts", "Block stylesheets", "Block third-party resources"]) {
    expect(screen.getByRole("switch", { name }).getAttribute("aria-checked")).toBe("false");
  }
  // One block back on: not "all" any more.
  toggle("Block images");
  await waitFor(() => expect(all().getAttribute("aria-checked")).toBe("false"));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.rendering.block).toEqual({ images: true, media: false, fonts: false, stylesheets: false, third_party: false });
});

test("rendering_category_shows_automatic_values_as_auto", async () => {
  await openRenderingOn();
  expect((screen.getByLabelText("Pages rendered at once") as HTMLInputElement).placeholder).toBe("Auto");
  expect((screen.getByLabelText("Browser memory budget") as HTMLInputElement).placeholder).toBe("Auto");
  expect((screen.getByLabelText("Window") as HTMLSelectElement).selectedOptions[0].textContent).toBe("Auto");
  // Blank goes back to automatic.
  typeInto("Pages rendered at once", "4");
  typeInto("Pages rendered at once", "");
  expect((screen.getByLabelText("Pages rendered at once") as HTMLInputElement).value).toBe("");
});

test("rendering_category_shows_each_issue_on_its_own_control", async () => {
  await openRenderingOn();
  typeInto("Maximum render time", "90");
  await screen.findByText("The maximum render time must be between 1 and 60 seconds.");
  typeInto("Pages rendered at once", "40");
  await screen.findByText("Pages rendered at once must be between 1 and 16. Leave it blank for automatic.");
  typeInto("Hidden text threshold", "0");
  await screen.findByText("The hidden content threshold must be between 1 and 100 percent.");
  fireEvent.change(screen.getByLabelText("Window"), { target: { value: "custom" } });
  typeInto("Window width", "100");
  await screen.findByText("A custom window width must be between 320 and 3,840 pixels.");
  expect(screen.getByRole("tab", { name: /^JavaScript rendering/ }).textContent).toContain("error");
});

test("rendering_category_shows_the_browser_and_changes_it", async () => {
  const t = await openRenderingOn();
  expect((await screen.findByRole("status")).textContent).toContain("none was found");
  fireEvent.click(screen.getByRole("button", { name: "Change browser" }));
  fireEvent.change(screen.getByLabelText("Browser path"), { target: { value: "/opt/chrome/chrome" } });
  fireEvent.click(screen.getByRole("button", { name: "Use this browser" }));
  await waitFor(() => expect(t.render.paths).toEqual(["/opt/chrome/chrome"]));
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Google Chrome 155.0.8059.39 (chosen by you)"));
  expect(screen.queryByLabelText("Browser path")).toBeNull();
});
