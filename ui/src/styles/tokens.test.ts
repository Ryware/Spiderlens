// SC-003 (contrast) and SC-004 (no values outside the token set), checked
// against the real token file and the real sources.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const here = join(process.cwd(), "src");
const tokensCss = readFileSync(join(here, "styles/tokens.css"), "utf8");

type Rgba = [number, number, number, number];

function block(selector: string): Record<string, string> {
  const start = tokensCss.indexOf(selector + " {");
  const body = tokensCss.slice(start, tokensCss.indexOf("\n}", start));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

function oklchToRgba(v: string): Rgba {
  const m = v.match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+))?\s*\)$/);
  if (!m) throw new Error(`not oklch: ${v}`);
  const [L, C, H] = [Number(m[1]), Number(m[2]), (Number(m[3]) * Math.PI) / 180];
  const a = C * Math.cos(H);
  const b = C * Math.sin(H);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const mm = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const lin = [
    4.0767416621 * l - 3.3077115913 * mm + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * mm - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * mm + 1.707614701 * s,
  ].map((x) => Math.min(1, Math.max(0, x)));
  return [lin[0], lin[1], lin[2], m[4] ? Number(m[4]) : 1];
}

// Browsers blend in gamma-encoded sRGB, so composite there.
const enc = (x: number) => (x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055);
const dec = (x: number) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
const over = (top: Rgba, base: Rgba): Rgba =>
  [0, 1, 2].map((i) => dec(enc(top[i]) * top[3] + enc(base[i]) * (1 - top[3]))).concat(1) as Rgba;
const luminance = (c: Rgba) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
function contrast(fg: Rgba, bg: Rgba) {
  const [a, b] = [luminance(fg), luminance(bg)].sort((x, y) => y - x);
  return (a + 0.05) / (b + 0.05);
}

const light = block(":root");
const themes = { light, dark: { ...light, ...block(':root[data-theme="dark"]') } };

describe.each(Object.entries(themes))("%s theme contrast (FR-018)", (_, t) => {
  const c = (name: string) => oklchToRgba(t[name]);
  const surfaces = ["bg", "surface", "surface-2", "sidebar", "chrome"];

  test.each(["fg", "fg-2", "fg-3"].flatMap((f) => surfaces.map((s) => [f, s])))("text %s on %s >= 4.5", (f, s) => {
    expect(contrast(c(f), c(s))).toBeGreaterThanOrEqual(4.5);
  });

  test.each(["ok", "info", "warn", "bad"].flatMap((m) => ["bg", "surface"].map((s) => [m, s])))(
    "status %s text on its soft fill over %s >= 4.5",
    (m, s) => {
      expect(contrast(c(m), over(c(`${m}-soft`), c(s)))).toBeGreaterThanOrEqual(4.5);
    },
  );

  test("primary button label >= 4.5", () => expect(contrast(c("accent-fg"), c("accent"))).toBeGreaterThanOrEqual(4.5));
  test.each(["bg", "surface"])("accent text on %s >= 4.5", (s) => expect(contrast(c("accent-text"), c(s))).toBeGreaterThanOrEqual(4.5));
  test.each(surfaces)("focus ring on %s >= 3", (s) => expect(contrast(c("accent"), c(s))).toBeGreaterThanOrEqual(3));
  test.each(["surface", "surface-2", "bg"])("control edges on %s >= 3", (s) =>
    expect(contrast(c("control-border"), c(s))).toBeGreaterThanOrEqual(3),
  );
});

test("dark theme only redefines tokens that light defines", () => {
  expect(Object.keys(block(':root[data-theme="dark"]')).filter((k) => !(k in light))).toEqual([]);
});

// SC-004: no raw colors or one-off sizes outside the token file.
function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === "generated" ? [] : sources(p);
    return /\.(tsx?|css)$/.test(f) && !f.endsWith(".test.ts") && !f.endsWith(".test.tsx") && f !== "tokens.css" ? [p] : [];
  });
}

test.each(sources(here).map((p) => [p.slice(here.length + 1), p]))("%s uses tokens only (SC-004)", (_, p) => {
  const text = readFileSync(p, "utf8");
  const raw = text.match(/#[0-9a-fA-F]{3,8}\b(?![\w-])|\b(?:rgba?|hsla?|oklch|oklab|lab|lch)\(/g) ?? [];
  const arbitrary = text.match(/\b[\w:-]+-\[[^\]]*\d(?:px|rem|em|%)?[^\]]*\]/g) ?? [];
  expect({ raw, arbitrary }).toEqual({ raw: [], arbitrary: [] });
});
