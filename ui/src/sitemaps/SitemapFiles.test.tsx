import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { expect, test } from "vitest";
import { fakeSitemapFile, fakeTransport } from "../testing/fakeTransport";
import { fileStatus, SitemapFilesView } from "./SitemapFiles";

const P = "/crawls/example.com.spiderlens";

function tree() {
  const t = fakeTransport();
  t.sitemaps.files = {
    available: true,
    files: [
      fakeSitemapFile(1, "https://example.com/index.xml", { kind: "index", entries: 2, children: 2 }),
      fakeSitemapFile(2, "https://example.com/a.xml", { parent_id: 1, level: 1, invalid_entries: 3 }),
      fakeSitemapFile(3, "https://example.com/b.xml", { parent_id: 1, level: 1, kind: null, http_status: 200, error_class: "invalid_xml", error: "unexpected end of file", error_line: 7, entries: 4 }),
      fakeSitemapFile(4, "file:/drafts/new.xml", { source: "local", base_url: "https://example.com/new.xml", final_url: null, http_status: null }),
    ],
  };
  return t;
}

const items = () => screen.getAllByRole("treeitem");
const names = () => items().map((i) => i.querySelector("[title]")!.getAttribute("title"));

test("files_are_a_tree_with_levels_sources_and_states", async () => {
  render(<SitemapFilesView transport={tree()} path={P} live={false} />);
  await waitFor(() => expect(items()).toHaveLength(4));
  const [index, a, b, local] = items();
  expect([index, a, b, local].map((i) => i.getAttribute("aria-level"))).toEqual(["1", "2", "2", "1"]);
  expect(index.getAttribute("aria-expanded")).toBe("true");
  expect(a.getAttribute("aria-expanded")).toBeNull();
  expect(within(index).getByText("2 files")).toBeTruthy();
  expect(within(a).getByText("Invalid entries")).toBeTruthy();
  expect(within(b).getByText("Invalid XML")).toBeTruthy();
  expect(within(local).getByText("On this computer")).toBeTruthy();
});

test("keyboard_moves_collapses_and_expands", async () => {
  render(<SitemapFilesView transport={tree()} path={P} live={false} />);
  await waitFor(() => expect(items()).toHaveLength(4));
  const t = screen.getByRole("tree");
  items()[0].focus();
  fireEvent.keyDown(t, { key: "ArrowLeft" });
  expect(names()).toEqual(["https://example.com/index.xml", "file:/drafts/new.xml"]);
  expect(items()[0].getAttribute("aria-expanded")).toBe("false");
  fireEvent.keyDown(t, { key: "ArrowRight" });
  expect(items()).toHaveLength(4);
  fireEvent.keyDown(t, { key: "ArrowRight" });
  expect(items()[1].getAttribute("aria-selected")).toBe("true");
  await waitFor(() => expect(document.activeElement).toBe(items()[1]));
  fireEvent.keyDown(t, { key: "ArrowDown" });
  expect(items()[2].getAttribute("aria-selected")).toBe("true");
  fireEvent.keyDown(t, { key: "ArrowLeft" });
  expect(items()[0].getAttribute("aria-selected")).toBe("true");
  fireEvent.keyDown(t, { key: "End" });
  expect(items()[3].getAttribute("aria-selected")).toBe("true");
});

test("the_selected_file_shows_what_reading_found", async () => {
  render(<SitemapFilesView transport={tree()} path={P} live={false} />);
  await waitFor(() => expect(items()).toHaveLength(4));
  fireEvent.click(items()[2]);
  const facts = screen.getByRole("region", { name: "Selected sitemap file" });
  expect(facts.textContent).toContain("unexpected end of file (line 7)");
  fireEvent.click(items()[3]);
  expect(screen.getByRole("region", { name: "Selected sitemap file" }).textContent).toContain("https://example.com/new.xml");
});

test("older_files_say_only_the_summary_is_shown", async () => {
  const t = tree();
  t.sitemaps.files.available = false;
  render(<SitemapFilesView transport={t} path={P} live={false} />);
  expect(await screen.findByText(/only the summary of each file is shown/)).toBeTruthy();
});

test("no_files_explains_how_to_get_some", async () => {
  render(<SitemapFilesView transport={fakeTransport()} path={P} live={false} />);
  expect(await screen.findByText("No sitemap files")).toBeTruthy();
  expect(screen.getByText(/Turn on Read sitemaps/)).toBeTruthy();
});

test("status_labels_pair_tone_with_text", () => {
  const f = fakeSitemapFile(1, "x");
  expect(fileStatus(f)).toEqual({ label: "OK 200", tone: "ok" });
  expect(fileStatus({ ...f, kind: null, http_status: 404, error_class: "http_4xx", error: "Not found" })).toEqual({ label: "Not read 404", tone: "bad" });
  expect(fileStatus({ ...f, error_class: "too_deep", error: "deep" }).tone).toBe("warn");
  expect(fileStatus({ ...f, over_limit: true }).label).toBe("Over the limit");
});
