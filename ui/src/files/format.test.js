import { fileStem, formatSize, formatWhen, isCrawlFile } from "./format";
test("sizes_read_with_one_unit", () => {
    expect(formatSize(0)).toBe("0 B");
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(120 * 1024 ** 2)).toBe("120 MB");
    expect(formatSize(1.2 * 1024 ** 3)).toBe("1.2 GB");
    expect(formatSize(3 * 1024 ** 4)).toBe("3.0 TB");
    // Rounding up to 10 drops the decimal.
    expect(formatSize(9.97 * 1024 ** 2)).toBe("10 MB");
    expect(formatSize(-1)).toBe("–");
});
test("dates_read_relative_then_short", () => {
    const now = Date.parse("2026-10-13T12:00:00Z");
    expect(formatWhen("2026-10-13T11:59:40Z", now)).toBe("Just now");
    expect(formatWhen("2026-10-13T11:55:00Z", now)).toBe("5 min ago");
    expect(formatWhen("2026-10-13T09:00:00Z", now)).toBe("3 h ago");
    expect(formatWhen("2026-10-12T10:00:00Z", now)).toBe("Yesterday");
    expect(formatWhen("2026-10-09T12:00:00Z", now)).toBe("4 days ago");
    expect(formatWhen("2026-09-01T12:00:00Z", now)).not.toMatch(/ago/);
    expect(formatWhen("not a date", now)).toBe("not a date");
});
test("file_names", () => {
    expect(fileStem("/crawls/example.com.spiderlens")).toBe("example.com");
    expect(fileStem("C:\\Crawls\\Shop.SPIDERLENS")).toBe("Shop");
    expect(isCrawlFile("/a/b.spiderlens")).toBe(true);
    expect(isCrawlFile("/a/b.csv")).toBe(false);
});
