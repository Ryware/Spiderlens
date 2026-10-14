# Desktop commands: sitemaps

All commands take the open crawl's handle like the spec 007 commands and read through the results `Reader`.

| Command | Input | Output | Notes |
|---|---|---|---|
| `sitemap_files` | — | `{ available: bool, files: SitemapFileRow[] }` | tree order; `available = false` for files without `sitemap_entries` (base table rows shown) |
| `sitemap_entries_for_url` | `url_id` | `UrlSitemapEntry[]` | inspector section |
| `orphan_summary` | — | `{ orphans, nofollow_only, complete: bool, not_crawled_internal: u64, mode_ok: bool, provisional: bool }` | header of the orphan filters |
| `sitemap_preview` | `GenerateOptions` | `{ pages, images, files, existing: string[] }` | counts only; `existing` lists files in the chosen folder that would be replaced |
| `sitemap_generate` | `GenerateOptions`, `folder`, `replace: bool` | `GenerateReport` | refused while a crawl runs (`CrawlRunning`) |

The 007 `view` command accepts `sitemap_file: number | null` on the `sitemaps` tab.

Settings: `SitemapSettings` is part of `CrawlConfig` and travels through the existing profile commands; `GenerateOptions` through `get_profile` / `save_profile` under `sitemap_generate`.

Events: none new; the analysis progress event of spec 009 shows the two passes as "Analysing: Sitemaps" and "Analysing: Orphan pages".
