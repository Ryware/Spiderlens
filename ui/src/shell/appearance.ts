// Appearance (spec 018): a brightness step and an accent palette, applied as
// data attributes on <html>; the token file does the rest. Both lists live
// here only, so a new step or palette is one entry plus its token block.

export const LEVELS = [
  { id: "light", name: "Light", tone: "light" },
  { id: "soft", name: "Soft", tone: "light" },
  { id: "island", name: "Island", tone: "light" },
  { id: "dark", name: "Dark", tone: "dark" },
  { id: "deep", name: "Deep", tone: "dark" },
  { id: "midnight", name: "Midnight", tone: "dark" },
] as const;

export const ACCENTS = [
  { id: "teal", name: "Teal" },
  { id: "cobalt", name: "Cobalt" },
  { id: "lavender", name: "Lavender" },
  { id: "orchid", name: "Orchid" },
  { id: "celery", name: "Celery" },
  { id: "cloud", name: "Cloud" },
] as const;

export type Level = (typeof LEVELS)[number]["id"];
export type Accent = (typeof ACCENTS)[number]["id"];
export type Tone = "light" | "dark";

export const isLevel = (v: unknown): v is Level => LEVELS.some((l) => l.id === v);
export const isAccent = (v: unknown): v is Accent => ACCENTS.some((a) => a.id === v);
export const toneOf = (level: Level): Tone => LEVELS.find((l) => l.id === level)!.tone;
export const levelName = (level: Level) => LEVELS.find((l) => l.id === level)!.name;

export interface Appearance {
  /** Follow the system light or dark setting, using the two steps below. */
  followSystem: boolean;
  /** The step used when not following the system. */
  level: Level;
  systemLight: Level;
  systemDark: Level;
  accent: Accent;
  /** macOS: let the system material show through the sidebar and toolbar (spec 017 FR-027). */
  translucent?: boolean;
}

export function resolveLevel(a: Appearance, systemDark: boolean): Level {
  if (!a.followSystem) return a.level;
  return systemDark ? a.systemDark : a.systemLight;
}

/**
 * Choosing a step from the slider, Settings or a command (US3-2). While
 * following the system, a step of the tone the system shows now becomes that
 * side's step and following stays on; a step of the other tone would not show
 * at all, so it turns following off instead.
 */
export function chooseLevel(a: Appearance, level: Level, systemDark: boolean): Partial<Appearance> {
  if (!a.followSystem) return { level };
  const tone = toneOf(level);
  if (tone !== (systemDark ? "dark" : "light")) return { followSystem: false, level };
  return tone === "light" ? { systemLight: level } : { systemDark: level };
}

export function applyAppearance(root: HTMLElement, level: Level, accent: Accent, material = false): void {
  root.dataset.level = level;
  root.dataset.tone = toneOf(level);
  root.dataset.accent = accent;
  if (material) root.dataset.material = "on";
  else delete root.dataset.material;
}
