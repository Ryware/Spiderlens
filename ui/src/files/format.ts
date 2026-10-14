// How crawl files read on screen: sizes, dates and names.

/** 512 B, 3.4 KB, 120 MB, 1.2 GB. Binary units, as file managers show them. */
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "–";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  // 9.96 MB rounds to "10.0", so pick the precision after rounding.
  const text = v < 10 && Number(v.toFixed(1)) < 10 ? v.toFixed(1) : Math.round(v).toString();
  return `${text} ${units[u]}`;
}

/** "Just now", "5 min ago", "3 h ago", "Yesterday", "4 days ago", then a short date. */
export function formatWhen(iso: string, now: number = Date.now()): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const min = Math.floor((now - t) / 60_000);
  if (min < 1) return "Just now";
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.floor(h / 24);
  if (d === 1) return "Yesterday";
  if (d < 7) return `${d} days ago`;
  const date = new Date(t);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) });
}

/** Full date and time for details such as the crawl information. */
export function formatDateTime(iso: string | null): string {
  if (!iso) return "–";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** The file name without its folder or the .spiderlens extension. */
export function fileStem(path: string): string {
  const base = path.split(/[\\/]/).pop() ?? path;
  return base.replace(/\.spiderlens$/i, "") || base;
}

export const isCrawlFile = (path: string) => /\.spiderlens$/i.test(path);
