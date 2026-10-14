// Search result snippet measuring in the UI (spec 027 part A, plan D1).
//
// A port of crates/results/src/serp: the same generated width table, the
// same Arabic joining and the same truncation, so the preview editor can
// measure while the user types without asking the app. measure.test.ts
// checks it against the widths the Rust side records in the fixture.

import table from "../generated/serpWidths.json";
import type { SerpStyle } from "../generated/SerpStyle";
import type { SnippetMeasure } from "../generated/SnippetMeasure";

export const LIMITS: Record<SerpStyle, { desktop: number; mobile: number }> = {
  title: { desktop: 600, mobile: 580 },
  description: { desktop: 990, mobile: 680 },
};
const FONT_PX: Record<SerpStyle, number> = { title: 20, description: 14 };
const MAX_MEASURED_CHARS = 1000;
const ELLIPSIS = "…";
const TATWEEL = 0x0640;
const LAM = 0x0644;

const widths = new Map<number, number>(table.widths as [number, number][]);
type Forms = { iso: number; fin: number; init: number; med: number; dual: boolean };
const arabicForms = new Map<number, Forms>(
  (table.arabic as number[][]).map(([cp, iso, fin, init, med, dual]) => [cp, { iso, fin, init, med, dual: dual === 1 }]),
);
const lamAlef = new Map<number, [number, number]>((table.lamAlef as number[][]).map(([cp, iso, fin]) => [cp, [iso, fin]]));

const between = (cp: number, ranges: [number, number][]) => ranges.some(([a, b]) => cp >= a && cp <= b);

/** Width of one character in thousandths of an em. */
export function emWidth(cp: number): number {
  const w = widths.get(cp);
  if (w !== undefined) return w;
  if (
    between(cp, [
      [0x200b, 0x200f], [0x2060, 0x2064], [0xfe00, 0xfe0f], [0xe0100, 0xe01ef],
      [0x0300, 0x036f], [0x0591, 0x05c7], [0x064b, 0x065f], [0x1ab0, 0x1aff], [0x20d0, 0x20ff],
    ])
  )
    return 0;
  if (
    between(cp, [
      [0x1100, 0x11ff], [0x2e80, 0x9fff], [0xa960, 0xa97f], [0xac00, 0xd7ff], [0xf900, 0xfaff],
      [0xfe30, 0xfe4f], [0xff00, 0xff60], [0xffe0, 0xffe6], [0x20000, 0x3ffff],
    ])
  )
    return 1000;
  if (between(cp, [[0x2600, 0x27bf], [0x1f000, 0x1faff]])) return 1250;
  return 556;
}

/** Collapses whitespace and trims, as search results display text. */
const displayed = (text: string): number[] => {
  const words = text.split(/\s+/u).filter(Boolean);
  return [...words.join(" ")].map((c) => c.codePointAt(0)!);
};

const joinsNext = (cp: number) => cp === TATWEEL || arabicForms.get(cp)?.dual === true;
const isJoining = (cp: number) => cp === TATWEEL || arabicForms.has(cp);

function charWidths(cps: number[]): number[] {
  const out: number[] = [];
  for (let i = 0; i < cps.length; ) {
    const c = cps[i];
    const prevJoins = i > 0 && joinsNext(cps[i - 1]);
    const lig = c === LAM && i + 1 < cps.length ? lamAlef.get(cps[i + 1]) : undefined;
    if (lig) {
      out.push(prevJoins ? lig[1] : lig[0], 0);
      i += 2;
      continue;
    }
    const f = arabicForms.get(c);
    if (f) {
      const nextJoins = f.dual && i + 1 < cps.length && isJoining(cps[i + 1]);
      out.push(prevJoins ? (nextJoins ? f.med : f.fin) : nextJoins ? f.init : f.iso);
    } else {
      out.push(emWidth(c));
    }
    i += 1;
  }
  return out;
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

/** Width in pixels, rounded, as a search result draws the line. */
export function pxWidth(text: string, style: SerpStyle): number {
  const units = sum(charWidths(displayed(text).slice(0, MAX_MEASURED_CHARS)));
  return Math.round((units * FONT_PX[style]) / 1000);
}

const isSpace = (cp: number) => /\s/u.test(String.fromCodePoint(cp));

/** The line as shown within `limitPx`: cut at a word with an ellipsis when it does not fit. */
export function truncate(text: string, style: SerpStyle, limitPx: number): { text: string; cut: boolean } {
  const all = displayed(text);
  const cps = all.slice(0, MAX_MEASURED_CHARS);
  const w = charWidths(cps);
  const limitUnits = Math.floor((limitPx * 1000) / FONT_PX[style]);
  if (sum(w) <= limitUnits && all.length <= MAX_MEASURED_CHARS) return { text: String.fromCodePoint(...all), cut: false };
  const room = Math.max(0, limitUnits - emWidth(ELLIPSIS.codePointAt(0)!));
  let used = 0;
  let end = 0;
  for (let i = 0; i < w.length; i++) {
    if (used + w[i] > room) break;
    used += w[i];
    end = i + 1;
  }
  const midWord = end > 0 && end < cps.length && !isSpace(cps[end]) && !isSpace(cps[end - 1]);
  if (midWord) {
    for (let j = end - 1; j >= 0; j--) {
      if (isSpace(cps[j])) {
        end = j;
        break;
      }
    }
  }
  return { text: String.fromCodePoint(...cps.slice(0, end)).trimEnd() + ELLIPSIS, cut: true };
}

export function measure(text: string, style: SerpStyle): SnippetMeasure {
  const { desktop, mobile } = LIMITS[style];
  const d = truncate(text, style, desktop);
  const m = truncate(text, style, mobile);
  return {
    chars: displayed(text).length,
    px: pxWidth(text, style),
    cut_desktop: d.cut,
    cut_mobile: m.cut,
    display_desktop: d.text,
    display_mobile: m.text,
  };
}
