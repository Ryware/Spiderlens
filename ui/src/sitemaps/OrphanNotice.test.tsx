import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { fakeOrphanSummary, fakeTransport } from "../testing/fakeTransport";
import { OrphanNotice, orphanText } from "./OrphanNotice";

const P = "/crawls/example.com.spiderlens";

test("orphan_notice_counts_both_classes", async () => {
  const t = fakeTransport();
  t.sitemaps.orphans = fakeOrphanSummary({ orphans: 12, nofollow_only: 1 });
  render(<OrphanNotice transport={t} path={P} live={false} />);
  expect(await screen.findByRole("status", { name: "Orphan pages" })).toHaveTextContent("12 orphan pages, 1 page linked only through nofollow.");
});

test("orphan_notice_warns_when_the_crawl_was_incomplete", () => {
  expect(orphanText(fakeOrphanSummary({ orphans: 1, incomplete: true, not_crawled: 40 }))).toContain("40 URLs not crawled");
  expect(orphanText(fakeOrphanSummary({ provisional: true, incomplete: true }))).toContain("still running");
});

test("orphan_notice_explains_a_list_crawl", () => {
  const reason = "Orphan pages need a Spider mode crawl: a list crawl does not follow links.";
  expect(orphanText(fakeOrphanSummary({ available: false, list_mode: true, reason }))).toBe(reason);
});
