import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { expect, test } from "vitest";
import type { SitemapGenerateOptions } from "../generated/SitemapGenerateOptions";
import { defaultGenerateOptions, fakeTransport } from "../testing/fakeTransport";
import { GenerateSitemapSheet } from "./GenerateSitemapSheet";

// fakeUrls(30): 27 internal pages answer 200, 3 answer 404.
function Harness({ t, running = false }: { t: ReturnType<typeof fakeTransport>; running?: boolean }) {
  const [o, setO] = useState<SitemapGenerateOptions>(defaultGenerateOptions());
  return <GenerateSitemapSheet open onOpenChange={() => {}} transport={t} path="/crawls/example.com.spiderlens" running={running} options={o} onOptions={setO} />;
}

const generate = () => screen.getByRole("button", { name: /generate/i });

test("the_count_follows_the_options", async () => {
  const t = fakeTransport();
  render(<Harness t={t} />);
  expect(await screen.findByText(/27 pages in 1 file/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("URLs per file"), { target: { value: "10" } });
  expect(await screen.findByText(/27 pages in 4 files/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Leave out addresses matching"), { target: { value: "page-1" } });
  expect(await screen.findByText(/pages left out by patterns/)).toBeTruthy();
});

test("a_pattern_that_matches_nothing_says_so", async () => {
  const t = fakeTransport();
  render(<Harness t={t} />);
  await screen.findByText(/27 pages/);
  fireEvent.change(screen.getByLabelText("Only addresses matching"), { target: { value: "^nothing$" } });
  expect((await screen.findByRole("alert")).textContent).toContain("No page matches");
});

test("generating_needs_a_folder_and_writes_the_chosen_options", async () => {
  const t = fakeTransport();
  t.picks.folder = "/site/public";
  render(<Harness t={t} />);
  await screen.findByText(/27 pages/);
  expect((generate() as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("radio", { name: "Separate file" }));
  fireEvent.click(screen.getByRole("switch", { name: "Compress (.xml.gz)" }));
  fireEvent.click(screen.getByRole("button", { name: /choose folder/i }));
  await waitFor(() => expect((screen.getByLabelText("Folder") as HTMLInputElement).value).toBe("/site/public"));
  await waitFor(() => expect((generate() as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(generate());
  expect(await screen.findByText(/Wrote 1 file with 27 pages to \/site\/public/)).toBeTruthy();
  expect(t.sitemaps.generated).toHaveLength(1);
  expect(t.sitemaps.generated[0]).toMatchObject({ folder: "/site/public", replace: false, options: { images: "separate", gzip: true } });
  expect((generate() as HTMLButtonElement).disabled).toBe(true);
});

test("existing_files_are_named_before_they_are_replaced", async () => {
  const t = fakeTransport();
  t.picks.folder = "/site/public";
  t.sitemaps.existing.set("/site/public", ["sitemap.xml"]);
  render(<Harness t={t} />);
  fireEvent.click(screen.getByRole("button", { name: /choose folder/i }));
  expect(await screen.findByText(/Replaces sitemap.xml in this folder/)).toBeTruthy();
  const btn = await screen.findByRole("button", { name: "Replace and generate" });
  await waitFor(() => expect((btn as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(btn);
  await screen.findByText(/Wrote 1 file/);
  expect(t.sitemaps.generated[0].replace).toBe(true);
});

test("a_running_crawl_holds_generation", async () => {
  const t = fakeTransport();
  t.picks.folder = "/site/public";
  render(<Harness t={t} running />);
  fireEvent.click(screen.getByRole("button", { name: /choose folder/i }));
  await screen.findByText(/27 pages/);
  expect(screen.getByText(/once the crawl has finished/)).toBeTruthy();
  expect((generate() as HTMLButtonElement).disabled).toBe(true);
});

test("fixed_date_and_priority_show_their_inputs", async () => {
  const t = fakeTransport();
  render(<Harness t={t} />);
  fireEvent.change(screen.getByLabelText("Last modified"), { target: { value: "fixed" } });
  expect(screen.getByLabelText("Last modified date")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Priority"), { target: { value: "fixed" } });
  expect((screen.getByLabelText("Priority value") as HTMLInputElement).value).toBe("0.5");
  expect(screen.queryByLabelText("Image hosts")).toBeNull();
  fireEvent.click(screen.getByRole("radio", { name: "In the page sitemap" }));
  expect(screen.getByLabelText("Image hosts")).toBeTruthy();
});
