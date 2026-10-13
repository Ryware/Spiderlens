import { describe, expect, it } from "vitest";
import { contentHeight, frame, MAX_SCROLL_PX, scrollFor, scrollToRow, virtualTop } from "./windowing";

const ROW = 28;
const VIEW = 560;

describe("windowing", () => {
  it("maps one to one while the table fits the scroll limit", () => {
    expect(contentHeight(1000, ROW)).toBe(28_000);
    expect(virtualTop(1400, 1000, ROW, VIEW)).toBe(1400);
    const f = frame(1400, 1000, ROW, VIEW, 0);
    expect([f.first, f.last, f.firstTopPx]).toEqual([50, 70, 1400]);
  });

  it("caps the scroll area and still reaches the last row of a large table", () => {
    const total = 5_000_000;
    expect(contentHeight(total, ROW)).toBe(MAX_SCROLL_PX);
    const end = frame(MAX_SCROLL_PX - VIEW, total, ROW, VIEW, 0);
    expect(end.last).toBe(total);
    expect(end.first).toBe(total - VIEW / ROW);
    // The last row ends exactly at the bottom of the viewport.
    expect(end.firstTopPx + (end.last - end.first) * ROW).toBeCloseTo(MAX_SCROLL_PX, 3);
    expect(frame(0, total, ROW, VIEW, 0).first).toBe(0);
  });

  it("round-trips scroll positions through the scale", () => {
    const total = 2_000_000;
    for (const top of [0, 12_345 * ROW, (total - 20) * ROW - VIEW]) {
      const s = scrollFor(top, total, ROW, VIEW);
      expect(virtualTop(s, total, ROW, VIEW)).toBeCloseTo(top, 3);
    }
  });

  it("scrolls the least needed to show a row", () => {
    expect(scrollToRow(10, 0, 1000, ROW, VIEW)).toBeNull();
    expect(scrollToRow(20, 0, 1000, ROW, VIEW)).toBe(21 * ROW - VIEW);
    expect(scrollToRow(3, 1400, 1000, ROW, VIEW)).toBe(3 * ROW);
  });

  it("handles an empty table", () => {
    const f = frame(0, 0, ROW, VIEW);
    expect([f.first, f.last, f.contentPx]).toEqual([0, 0, 0]);
  });
});
