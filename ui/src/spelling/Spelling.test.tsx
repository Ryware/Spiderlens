// Spec 027 part D in the window: spelling settings and dictionaries, the
// Check spelling control on the Spelling tab, and misspelt words in the
// details pane. The checking itself is tested in Rust (crates/spell/tests).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import type { UrlDetail } from "../generated/UrlDetail";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

type T = ReturnType<typeof fakeTransport>;
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;

async function setup(t: T = fakeTransport()) {
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  return t;
}

async function openSpellingSettings() {
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: /^Spelling/ }));
}

async function crawl(t: T) {
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  await screen.findByRole("button", { name: "Spelling" });
}

async function openSpellingTab(t: T) {
  await crawl(t);
  fireEvent.click(screen.getByRole("button", { name: "Spelling" }));
  return screen.findByRole("region", { name: "Spelling" });
}

test("spelling_settings_reach_the_crawl", async () => {
  const t = await setup();
  await openSpellingSettings();
  fireEvent.click(screen.getByRole("switch", { name: "Check spelling after the crawl" }));
  fireEvent.change(screen.getByLabelText("Language of pages without a lang attribute"), { target: { value: "he" } });
  fireEvent.click(screen.getByRole("switch", { name: "Skip words in capitals" }));
  fireEvent.change(screen.getByLabelText("Words to ignore"), { target: { value: "SpiderLens\nRyware" } });
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.spelling).toEqual({ check: true, default_lang: "he", skip_all_caps: false, ignore: ["SpiderLens", "Ryware"] });
});

test("a_dictionary_shows_its_size_and_licence_before_download", async () => {
  const t = await setup();
  await openSpellingSettings();
  const installed = await screen.findByRole("list", { name: "Installed dictionaries" });
  expect(within(installed).getByText("Built in")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Dictionary to download"), { target: { value: "he" } });
  fireEvent.click(screen.getByRole("button", { name: "Download…" }));
  expect(screen.getByText(/Hebrew is 5\.5 MB and licensed under AGPL-3\.0/)).toBeTruthy();
  expect(t.spelling.downloads).toEqual([]);
  fireEvent.click(screen.getByRole("button", { name: "Download" }));
  await screen.findByText("Installed Hebrew.");
  expect(t.spelling.downloads).toEqual(["he"]);
  expect(within(screen.getByRole("list", { name: "Installed dictionaries" })).getByText("Hebrew")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Remove Hebrew" }));
  await screen.findByText("Removed Hebrew.");
  expect(within(screen.getByRole("list", { name: "Installed dictionaries" })).queryByText("Hebrew")).toBeNull();
});

test("a_dictionary_is_added_from_files", async () => {
  await setup();
  await openSpellingSettings();
  const add = await screen.findByRole("button", { name: "Add from .aff and .dic files…" });
  expect((add as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("Language code for your dictionary"), { target: { value: "de-CH" } });
  fireEvent.click(add);
  await screen.findByText("Installed de-CH.");
  expect(within(screen.getByRole("list", { name: "Installed dictionaries" })).getAllByText("de-CH").length).toBeGreaterThan(0);
});

test("check_spelling_runs_on_the_open_file_and_shows_what_it_found", async () => {
  const t = await setup();
  const tab = await openSpellingTab(t);
  // Not checked yet: the spelling filter says why.
  fireEvent.click(within(tab).getByRole("button", { name: "Check spelling" }));
  await waitFor(() => expect(t.spelling.started).toHaveLength(1));
  expect(t.spelling.started[0].path).toBe("/crawls/example.com.spiderlens");
  expect(t.spelling.started[0].settings.default_lang).toBe("en");

  act(() => t.spelling.progress({ checked: 10, total: 30 }));
  expect(within(tab).getByText("Checking spelling: 10 of 30 pages")).toBeTruthy();
  fireEvent.click(within(tab).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(t.spelling.cancelled).toBe(1));

  t.results.features.push("spelling");
  t.results.urls[1].spellErrors = 3;
  act(() => t.spelling.finish({ missing: { fr: 2 }, no_dictionary: 2 }));
  expect((await within(tab).findByTestId("spell-summary")).textContent).toContain("12 errors (5 different words) in 28 pages");
  expect(within(tab).getByText(/No dictionary for fr \(2 pages\)/)).toBeTruthy();
  expect(within(tab).getByRole("button", { name: "Check again" })).toBeTruthy();
});

test("spelling_is_checked_when_a_crawl_ends_if_asked", async () => {
  const t = fakeTransport();
  await setup(t);
  await openSpellingSettings();
  fireEvent.click(screen.getByRole("switch", { name: "Check spelling after the crawl" }));
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await crawl(t);
  await waitFor(() => expect(t.spelling.started).toHaveLength(1));
});

test("spelling_is_not_checked_after_a_crawl_by_default", async () => {
  const t = await setup();
  await crawl(t);
  await new Promise((r) => setTimeout(r, 50));
  expect(t.spelling.started).toHaveLength(0);
});

test("details_list_misspelt_words_and_ignore_adds_to_the_settings", async () => {
  const t = fakeTransport();
  const base = t.urlDetail;
  t.urlDetail = async (p, id): Promise<UrlDetail> => ({
    ...(await base(p, id)),
    spelling: {
      lang: "en",
      status: "checked",
      words: 120,
      errors: 2,
      lang_mismatch: false,
      findings: [
        { word: "recieve", source: "body", context: "We recieve guests", suggestions: ["receive", "relieve"], site_wide: false },
        { word: "notise", source: "body", context: "Copyright notise", suggestions: ["notice"], site_wide: true },
      ],
    },
  });
  await setup(t);
  const tab = await openSpellingTab(t);
  fireEvent.click(within(tab).getAllByRole("row")[1]);
  const details = screen.getByRole("complementary", { name: "Details" });
  const section = (await within(details).findByRole("heading", { name: "Spelling (2)" })).closest("section") as HTMLElement;
  expect(within(section).getByText("recieve")).toBeTruthy();
  expect(within(section).getByText("Did you mean receive, relieve?")).toBeTruthy();
  expect(within(section).getByText("Site-wide")).toBeTruthy();
  fireEvent.click(within(section).getAllByRole("button", { name: "Ignore" })[0]);
  expect(within(section).getByRole("button", { name: "Ignored" })).toBeTruthy();

  // The next check uses the ignore list.
  fireEvent.click(within(tab).getByRole("button", { name: "Check spelling" }));
  await waitFor(() => expect(t.spelling.started).toHaveLength(1));
  expect(t.spelling.started[0].settings.ignore).toEqual(["recieve"]);
});
