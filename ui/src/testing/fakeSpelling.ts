// In-memory dictionaries and spelling checks for UI tests. The real check is
// tested in Rust (crates/spell/tests).

import type { Dictionaries } from "../generated/Dictionaries";
import type { Installed } from "../generated/Installed";
import type { SpellingSettings } from "../generated/SpellingSettings";
import type { SpellSummary } from "../generated/SpellSummary";
import type { SpellProgress, Transport } from "../transport";

export interface FakeSpelling {
  dictionaries: Dictionaries;
  downloads: string[];
  started: { path: string; settings: SpellingSettings }[];
  cancelled: number;
  /** The summary spellSummary returns. */
  summary: SpellSummary | null;
  /** Sends a progress event, as the running check would. */
  progress(p: Partial<SpellProgress>): void;
  /** Ends the running check with this summary. */
  finish(s?: Partial<SpellSummary>): void;
}

export const fakeSummary = (s: Partial<SpellSummary> = {}): SpellSummary => ({
  pages: 30,
  partly_checked: 0,
  no_dictionary: 0,
  errors: 12,
  distinct_words: 5,
  site_wide_blocks: 1,
  missing: {},
  dictionaries: ["en"],
  cancelled: false,
  ms: 900,
  ...s,
});

const en: Installed = { lang: "en", name: "English (United States)", licence: "MIT AND BSD", size: 554_848, origin: "bundled" };

type SpellApi = Pick<
  Transport,
  | "spellDictionaries"
  | "spellDownload"
  | "pickDictionaryFiles"
  | "spellAdd"
  | "spellRemove"
  | "spellStart"
  | "spellCancel"
  | "spellSummary"
  | "onSpellProgress"
>;

export function fakeSpellingTransport(): SpellApi & { spelling: FakeSpelling } {
  const listeners = new Set<(p: SpellProgress) => void>();
  const emit = (p: SpellProgress) => listeners.forEach((cb) => cb(p));
  const spelling: FakeSpelling = {
    dictionaries: {
      installed: [en],
      available: [
        { lang: "en-GB", name: "English (United Kingdom)", licence: "MIT AND BSD", size: 555_192 },
        { lang: "he", name: "Hebrew", licence: "AGPL-3.0", size: 5_767_094 },
      ],
    },
    downloads: [],
    started: [],
    cancelled: 0,
    summary: null,
    progress: (p) => emit({ checked: 0, total: 30, state: "running", summary: null, error: null, ...p }),
    finish: (s) => {
      spelling.summary = fakeSummary(s);
      emit({ checked: 30, total: 30, state: "done", summary: spelling.summary, error: null });
    },
  };
  const install = (i: Installed) => {
    const d = spelling.dictionaries;
    spelling.dictionaries = {
      installed: [...d.installed.filter((x) => x.lang !== i.lang), i],
      available: d.available.filter((x) => x.lang !== i.lang),
    };
    return i;
  };
  return {
    spelling,
    spellDictionaries: async () => structuredClone(spelling.dictionaries),
    spellDownload: async (lang) => {
      spelling.downloads.push(lang);
      const a = spelling.dictionaries.available.find((x) => x.lang === lang)!;
      return install({ lang, name: a.name, licence: a.licence, size: a.size, origin: "downloaded" });
    },
    pickDictionaryFiles: async () => ({ aff: "/dicts/my.aff", dic: "/dicts/my.dic" }),
    spellAdd: async (lang) => install({ lang, name: lang, licence: "As supplied", size: 1000, origin: "file" }),
    spellRemove: async (lang) => {
      spelling.dictionaries = { ...spelling.dictionaries, installed: spelling.dictionaries.installed.filter((x) => x.lang !== lang) };
    },
    spellStart: async (path, settings) => {
      spelling.started.push({ path, settings });
    },
    spellCancel: async () => {
      spelling.cancelled++;
    },
    spellSummary: async () => spelling.summary,
    onSpellProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
  };
}
