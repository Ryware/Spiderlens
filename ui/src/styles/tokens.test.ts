// SC-003 (contrast) and SC-004 (no values outside the token set), checked
// against the real token file and the real sources.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { ACCENTS, LEVELS } from "../shell/appearance";

const here = join(process.cwd(), "src");
const tokensCss = readFileSync(join(here, "styles/tokens.css"), "utf8");

type Rgba = [number, number, number, number];

/** Tokens declared in the rule whose selector list starts with `selector`. */
function block(selector: string): Record<string, string> {
  const start = tokensCss.indexOf(selector);
  if (start < 0) throw new Error(`no block ${selector}`);
  const open = tokensCss.indexOf("{", start);
  const body = tokensCss.slice(open, tokensCss.indexOf("\n}", open));
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

const light = block(":root {");
const darkBlock = block(':root[data-tone="dark"],');
const dark = { ...light, ...darkBlock };
const levelBlock = (id: string) => block(`:root[data-level="${id}"] {`);

// Every step as the token set a region actually sees (spec 018 R2). Island is
// Light content plus Dark chrome, so it is covered by checking both.
const levels: Record<string, Record<string, string>> = {
  light,
  soft: { ...light, ...levelBlock("soft") },
  "island content": { ...light, ...levelBlock("island") },
  "island chrome": dark,
  dark,
  deep: { ...dark, ...levelBlock("deep") },
  midnight: { ...dark, ...levelBlock("midnight") },
};
const lightTone = new Set(["light", "soft", "island content"]);

const ACCENT_TOKENS = ["accent", "accent-hover", "accent-fg", "accent-text", "accent-soft", "selection"];
function accentBlock(id: string, tone: "light" | "dark"): Record<string, string> {
  if (id === "teal") return Object.fromEntries(ACCENT_TOKENS.map((k) => [k, (tone === "light" ? light : dark)[k]]));
  return block(tone === "light" ? `:root[data-accent="${id}"] {` : `:root[data-accent="${id}"][data-tone="dark"],`);
}

const combos = Object.entries(levels).flatMap(([lv, t]) =>
  ACCENTS.map((a) => [`${lv} / ${a.name}`, { ...t, ...accentBlock(a.id, lightTone.has(lv) ? "light" : "dark") }] as const),
);

describe.each(combos)("%s contrast (spec 017 FR-018, spec 018 FR-017)", (_, t) => {
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

test("step blocks only redefine tokens that light defines (spec 018 FR-018)", () => {
  const blocks = [darkBlock, ...["soft", "island", "deep", "midnight"].map(levelBlock)];
  expect(blocks.flatMap((b) => Object.keys(b)).filter((k) => !(k in light))).toEqual([]);
});

test("Light and Dark keep the spec 017 values", () => {
  expect(light.bg).toBe("oklch(0.985 0.002 195)");
  expect(light.accent).toBe("oklch(0.50 0.095 188)");
  expect(dark.bg).toBe("oklch(0.175 0.006 195)");
  expect(dark.accent).toBe("oklch(0.80 0.10 178)");
});

describe.each(ACCENTS.filter((a) => a.id !== "teal"))("palette $name", (a) => {
  test("defines exactly the six accent tokens in both tones (FR-009)", () => {
    for (const tone of ["light", "dark"] as const) expect(Object.keys(accentBlock(a.id, tone)).sort()).toEqual([...ACCENT_TOKENS].sort());
  });
  test("Island chrome uses the dark-tone values", () => {
    expect(block(`:root[data-level="island"][data-accent="${a.id}"] [data-region="chrome"] {`)).toEqual(accentBlock(a.id, "dark"));
  });
});

test.each(ACCENTS)("$name swatch matches its palette in both tones", (a) => {
  expect(block(`[data-swatch="${a.id}"] {`).swatch).toBe(accentBlock(a.id, "light").accent);
  expect(block(`:root[data-tone="dark"] [data-swatch="${a.id}"],`).swatch).toBe(accentBlock(a.id, "dark").accent);
});

test.each(LEVELS.map((l) => [l.id]))("%s preview in Settings shows that step's sidebar and surface", (id) => {
  const pv = block(`[data-preview="${id}"] {`);
  const side = id === "island" ? levels["island chrome"] : levels[id];
  const main = id === "island" ? levels["island content"] : levels[id];
  expect(pv).toEqual({ "preview-side": side.sidebar, "preview-main": main.surface });
});

// FR-010: an accent is never mistaken for a status colour.
function hue(v: string): { c: number; h: number } {
  const m = v.match(/^oklch\(\s*[\d.]+\s+([\d.]+)\s+([\d.]+)/)!;
  return { c: Number(m[1]), h: Number(m[2]) };
}
test.each(ACCENTS.flatMap((a) => (["light", "dark"] as const).map((tone) => [a.name, tone, a.id] as const)))(
  "%s (%s tone) sits 20 degrees of hue away from every status colour",
  (_, tone, id) => {
    const acc = hue(accentBlock(id, tone).accent);
    if (acc.c < 0.03) return; // no visible hue (Cloud)
    const statuses = Object.values(levels).flatMap((t) => ["ok", "info", "warn", "bad"].map((m) => hue(t[m]).h));
    for (const h of statuses) {
      const d = Math.abs(acc.h - h) % 360;
      expect(Math.min(d, 360 - d)).toBeGreaterThanOrEqual(20);
    }
  },
);

test("every palette in the token file is offered in the app, and the reverse (FR-011)", () => {
  const inCss = [...tokensCss.matchAll(/:root\[data-accent="([\w-]+)"\] \{/g)].map((m) => m[1]);
  expect(["teal", ...inCss].sort()).toEqual(ACCENTS.map((a) => a.id).sort());
  const levelsInCss = [...tokensCss.matchAll(/:root\[data-level="([\w-]+)"\] \{/g)].map((m) => m[1]);
  expect(["light", "dark", ...levelsInCss].sort()).toEqual(LEVELS.map((l) => l.id).sort());
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
