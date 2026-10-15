import { render, renderHook, screen, waitFor } from "@testing-library/react";
import { expect, test } from "vitest";
import { fakeTransport } from "../testing/fakeTransport";
import { SitemapEntryList, useSitemapEntries } from "./InspectorSitemaps";

test("sitemap_entries_list_each_file_and_its_lastmod", () => {
  render(
    <SitemapEntryList
      entries={[
        { file_id: 1, location: "https://example.com/a.xml", lastmod_raw: "2026-01-02", lastmod: 1767312000 },
        { file_id: 2, location: "https://example.com/b.xml", lastmod_raw: "yesterday", lastmod: null },
        { file_id: 3, location: "https://example.com/c.xml", lastmod_raw: null, lastmod: null },
      ]}
    />,
  );
  const list = screen.getByRole("list", { name: "Sitemaps listing this URL" });
  expect(list.textContent).toContain("lastmod 2026-01-02");
  expect(list.textContent).toContain("yesterday (not a valid date)");
  expect(list.textContent).toContain("no lastmod");
});

test("sitemap_entries_say_when_no_sitemap_lists_the_url", () => {
  render(<SitemapEntryList entries={[]} />);
  expect(screen.getByText("Not listed in any sitemap the crawl read.")).toBeTruthy();
});

test("sitemap_entries_load_for_the_selected_url", async () => {
  const t = fakeTransport();
  t.sitemaps.entries.set(4, [{ file_id: 1, location: "https://example.com/a.xml", lastmod_raw: null, lastmod: null }]);
  const { result } = renderHook(() => useSitemapEntries(t, "/c.spiderlens", 4));
  await waitFor(() => expect(result.current).toHaveLength(1));
});
