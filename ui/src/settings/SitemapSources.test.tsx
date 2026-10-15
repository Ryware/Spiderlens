import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { expect, test } from "vitest";
import type { Issue } from "../generated/Issue";
import type { SitemapSettings } from "../generated/SitemapSettings";
import { fakeTransport } from "../testing/fakeTransport";
import { SitemapSources } from "./SitemapSources";

const defaults: SitemapSettings = { read: true, crawl_sitemap_only: true, urls: [], files: [] };

function Harness({ initial = defaults, issues = [], t = fakeTransport(), seen }: { initial?: SitemapSettings; issues?: Issue[]; t?: ReturnType<typeof fakeTransport>; seen: SitemapSettings[] }) {
  const [v, setV] = useState(initial);
  return (
    <SitemapSources
      value={v}
      issues={issues}
      transport={t}
      onChange={(n) => {
        seen.push(n);
        setV(n);
      }}
    />
  );
}

const sw = (name: string) => screen.getByRole("switch", { name });

test("both_switches_start_on_and_crawling_depends_on_reading", () => {
  const seen: SitemapSettings[] = [];
  render(<Harness seen={seen} />);
  expect(sw("Read sitemaps").getAttribute("aria-checked")).toBe("true");
  expect(sw("Crawl URLs found only in sitemaps").getAttribute("aria-checked")).toBe("true");
  fireEvent.click(sw("Crawl URLs found only in sitemaps"));
  expect(seen.at(-1)).toMatchObject({ read: true, crawl_sitemap_only: false });
  fireEvent.click(sw("Read sitemaps"));
  expect(seen.at(-1)?.read).toBe(false);
  expect(sw("Crawl URLs found only in sitemaps").getAttribute("aria-disabled") ?? sw("Crawl URLs found only in sitemaps").getAttribute("data-disabled")).not.toBeNull();
  expect((screen.getByLabelText("More sitemaps") as HTMLTextAreaElement).disabled).toBe(true);
  expect((screen.getByRole("button", { name: /Add sitemap file/ }) as HTMLButtonElement).disabled).toBe(true);
});

test("extra_sitemap_urls_are_one_per_line", () => {
  const seen: SitemapSettings[] = [];
  render(<Harness seen={seen} />);
  fireEvent.change(screen.getByLabelText("More sitemaps"), { target: { value: "https://a.test/a.xml\nhttps://a.test/b.xml" } });
  expect(seen.at(-1)?.urls).toEqual(["https://a.test/a.xml", "https://a.test/b.xml"]);
  fireEvent.change(screen.getByLabelText("More sitemaps"), { target: { value: "" } });
  expect(seen.at(-1)?.urls).toEqual([]);
});

test("a_local_file_is_picked_given_an_address_and_removed", async () => {
  const seen: SitemapSettings[] = [];
  const t = fakeTransport();
  render(<Harness seen={seen} t={t} />);
  fireEvent.click(screen.getByRole("button", { name: /Add sitemap file/ }));
  await new Promise((r) => setTimeout(r, 0));
  expect(seen).toHaveLength(0);
  t.picks.sitemapFile = "/drafts/sitemap.xml";
  fireEvent.click(screen.getByRole("button", { name: /Add sitemap file/ }));
  await waitFor(() => expect(screen.getByLabelText("Sitemap file 1")).toBeTruthy());
  expect((screen.getByLabelText("Sitemap file 1") as HTMLInputElement).value).toBe("/drafts/sitemap.xml");
  fireEvent.change(screen.getByLabelText("Sitemap file 1 address"), { target: { value: "https://a.test/sitemap.xml" } });
  expect(seen.at(-1)?.files).toEqual([{ path: "/drafts/sitemap.xml", base_url: "https://a.test/sitemap.xml" }]);
  fireEvent.click(screen.getByRole("button", { name: "Remove sitemap file 1" }));
  expect(seen.at(-1)?.files).toEqual([]);
});

test("issues_show_under_the_setting_they_name", () => {
  const issues: Issue[] = [
    { path: "scope.sitemaps.urls[0]", severity: "error", code: "sitemap_url_invalid", message: "Enter the full sitemap address." },
    { path: "scope.sitemaps.files[0].base_url", severity: "error", code: "sitemap_base_invalid", message: "Enter the address this file will be published at." },
  ];
  render(<Harness seen={[]} issues={issues} initial={{ ...defaults, urls: ["x"], files: [{ path: "/a.xml", base_url: "" }] }} />);
  const alerts = screen.getAllByRole("alert").map((a) => a.textContent);
  expect(alerts).toEqual(["Error: Enter the full sitemap address.", "Error: Enter the address this file will be published at."]);
});
