import { describe, expect, it } from "vitest";
import { all, clip, contains, count, extend, normalize, shift, single, toggle } from "./selection";

describe("selection", () => {
  it("merges ranges", () => {
    expect(normalize([[5, 7], [0, 2], [2, 3], [6, 9]])).toEqual([[0, 3], [5, 9]]);
  });

  it("extends from the anchor and replaces the previous span", () => {
    let s = single(10);
    s = extend(s, 14);
    expect(s.ranges).toEqual([[10, 15]]);
    s = extend(s, 8);
    expect(s.ranges).toEqual([[8, 11]]);
    expect(count(s.ranges)).toBe(3);
  });

  it("toggles single rows, splitting a range", () => {
    let s = extend(single(0), 4);
    s = toggle(s, 2);
    expect(s.ranges).toEqual([[0, 2], [3, 5]]);
    expect(contains(s.ranges, 2)).toBe(false);
    s = toggle(s, 2);
    expect(s.ranges).toEqual([[0, 5]]);
  });

  it("selects everything as one range", () => {
    expect(all(1_000_000).ranges).toEqual([[0, 1_000_000]]);
  });

  it("follows a refresh and clips to the view", () => {
    const s = shift(extend(single(3), 5), 2, 7);
    expect(s.ranges).toEqual([[5, 7]]);
    expect(s.focus).toBeNull();
    expect(clip(single(9), 5).ranges).toEqual([]);
  });
});
