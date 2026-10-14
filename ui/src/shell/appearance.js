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
];
export const ACCENTS = [
    { id: "teal", name: "Teal" },
    { id: "cobalt", name: "Cobalt" },
    { id: "lavender", name: "Lavender" },
    { id: "orchid", name: "Orchid" },
    { id: "celery", name: "Celery" },
    { id: "cloud", name: "Cloud" },
];
export const isLevel = (v) => LEVELS.some((l) => l.id === v);
export const isAccent = (v) => ACCENTS.some((a) => a.id === v);
export const toneOf = (level) => LEVELS.find((l) => l.id === level).tone;
export const levelName = (level) => LEVELS.find((l) => l.id === level).name;
export function resolveLevel(a, systemDark) {
    if (!a.followSystem)
        return a.level;
    return systemDark ? a.systemDark : a.systemLight;
}
/**
 * Choosing a step from the slider, Settings or a command (US3-2). While
 * following the system, a step of the tone the system shows now becomes that
 * side's step and following stays on; a step of the other tone would not show
 * at all, so it turns following off instead.
 */
export function chooseLevel(a, level, systemDark) {
    if (!a.followSystem)
        return { level };
    const tone = toneOf(level);
    if (tone !== (systemDark ? "dark" : "light"))
        return { followSystem: false, level };
    return tone === "light" ? { systemLight: level } : { systemDark: level };
}
export function applyAppearance(root, level, accent, material = false) {
    root.dataset.level = level;
    root.dataset.tone = toneOf(level);
    root.dataset.accent = accent;
    if (material)
        root.dataset.material = "on";
    else
        delete root.dataset.material;
}
