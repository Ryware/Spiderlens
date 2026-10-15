import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { measure, pxWidth, truncate } from "./measure";

// Same fixture the Rust side checks: Chromium reference widths and the
// widths crates/results/src/serp records (plan D1, one table two readers).
const csv = readFileSync(resolve(__dirname, "../../../crates/test-fixtures/data/serp/reference.csv"), "utf8");

function rows() {
  return csv
    .trim()
    .split(/\r?\n/)
    .slice(1)
    .map((line) => {
      const m = line.match(/^([^,]+),([^,]+),("(?:[^"]|"")*"|[^,]*),(\d+),(\d+)$/)!;
      const text = m[3].startsWith('"') ? m[3].slice(1, -1).replace(/""/g, '"') : m[3];
      return { id: m[1], style: m[2] === "title" ? ("title" as const) : ("description" as const), text, reference: +m[4], ours: +m[5] };
    });
}

describe("serp measure", () => {
  it("matches the Rust widths on every fixture string", () => {
    const all = rows();
    expect(all).toHaveLength(400);
    for (const r of all) expect([r.id, pxWidth(r.text, r.style)]).toEqual([r.id, r.ours]);
  });

  it("is within 2% of the browser for 95% of strings (SC-A1)", () => {
    const all = rows();
    const close = all.filter((r) => Math.abs(pxWidth(r.text, r.style) - r.reference) / r.reference <= 0.02);
    expect(close.length / all.length).toBeGreaterThanOrEqual(0.95);
  });

  it("cuts long titles at a word with an ellipsis", () => {
    const title = "The Complete Beginner Guide To Choosing The Right Running Shoes For Every Kind Of Runner";
    const t = truncate(title, "title", 600);
    expect(t.cut).toBe(true);
    expect(t.text.endsWith("…")).toBe(true);
    const kept = t.text.slice(0, -1);
    expect(title.startsWith(kept)).toBe(true);
    expect(title[kept.length]).toBe(" ");
    expect(pxWidth(t.text, "title")).toBeLessThanOrEqual(600);
  });

  it("reports desktop and mobile separately", () => {
    const d =
      "Shop lightweight running shoes for road and trail with free delivery, easy returns within thirty days and a price match on every pair we sell";
    const m = measure(d, "description");
    expect(m.cut_desktop).toBe(false);
    expect(m.cut_mobile).toBe(true);
    expect(m.display_desktop).toBe(d);
  });

  it("collapses whitespace and ignores niqqud width", () => {
    expect(pxWidth("  a   b ", "title")).toBe(pxWidth("a b", "title"));
    expect(pxWidth("בָּיִת", "title")).toBe(pxWidth("בית", "title"));
  });
});
