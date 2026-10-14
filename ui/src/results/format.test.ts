// How cell values read (plan D8).
import { cellTone, formatBytes, formatCell, formatCompact, numeric } from "./format";

test("format_bytes_picks_a_unit", () => {
  expect(formatBytes(0)).toBe("0 B");
  expect(formatBytes(1023)).toBe("1023 B");
  expect(formatBytes(1024)).toBe("1.0 KB");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(20 * 1024)).toBe("20 KB");
  expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
  expect(formatBytes(2048 * 1024 ** 3)).toBe("2048 GB");
});

test("format_cell_by_kind", () => {
  expect(formatCell("text", null)).toBe("");
  expect(formatCell("int", 1234567)).toBe((1234567).toLocaleString());
  expect(formatCell("int", "n/a")).toBe("n/a");
  expect(formatCell("bytes", 2048)).toBe("2.0 KB");
  expect(formatCell("ms", 1500)).toBe(`${(1500).toLocaleString()} ms`);
  expect(formatCell("bool", true)).toBe("Yes");
  expect(formatCell("bool", false)).toBe("No");
  expect(formatCell("url", "https://example.com/")).toBe("https://example.com/");
  expect(formatCell("status", 404)).toBe("404");
});

test("cell_tone_follows_status_class_and_state", () => {
  expect(cellTone("status", 200)).toBe("ok");
  expect(cellTone("status", 301)).toBe("info");
  expect(cellTone("status", 404)).toBe("warn");
  expect(cellTone("status", 503)).toBe("bad");
  expect(cellTone("status", null)).toBeNull();
  expect(cellTone("state", "Failed: timeout")).toBe("bad");
  expect(cellTone("state", "Blocked by robots.txt")).toBe("warn");
  expect(cellTone("state", "Crawled")).toBeNull();
  expect(cellTone("int", 404)).toBeNull();
});

test("numeric_kinds_align_right", () => {
  expect(["int", "bytes", "ms", "status"].every((k) => numeric(k as never))).toBe(true);
  expect(numeric("text")).toBe(false);
  expect(numeric("url")).toBe(false);
});

test("issues_cells_read_as_highest_priority_and_count", () => {
  expect(formatCell("issues", 4_000_003)).toBe("High · 3");
  expect(formatCell("issues", 1_000_012)).toBe("Opportunity · 12");
  expect(cellTone("issues", 3_000_001)).toBe("warn");
  expect(formatCell("issues", null)).toBe("");
});

test("format_compact_keeps_counts_short", () => {
  expect(formatCompact(0)).toBe("0");
  expect(formatCompact(950)).toBe("950");
  expect(formatCompact(1000)).toBe("1k");
  expect(formatCompact(1234)).toBe("1.2k");
  expect(formatCompact(9_960)).toBe("10k");
  expect(formatCompact(34_499)).toBe("34k");
  expect(formatCompact(999_499)).toBe("999k");
  expect(formatCompact(999_950)).toBe("1M");
  expect(formatCompact(1_500_000)).toBe("1.5M");
  expect(formatCompact(12_345_678)).toBe("12M");
});
