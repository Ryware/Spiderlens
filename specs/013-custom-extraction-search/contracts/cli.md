# CLI Contract: Custom Rules

All commands work without the desktop app (constitution III). Rule files
use the format in [data-model.md](../data-model.md).

| Command | What it does | Exit codes |
|---|---|---|
| `spiderlens crawl URL --rules FILE` | Adds the rules in FILE to the profile's rules for this crawl; values are stored during the crawl | as `crawl`; invalid rules: configuration error code |
| `spiderlens rules validate FILE` | Prints each error with rule name and position; prints nothing on success | 0 valid, configuration error code otherwise |
| `spiderlens rules test FILE --url URL` | Fetches URL once (crawl identity, robots.txt honoured unless `--ignore-robots`) and prints each rule's count and values | 0, or the fetch error code |
| `spiderlens rules test FILE --crawl CRAWL --page URL` | Same, on the stored HTML of a page in CRAWL, no request | 0; "not possible" when the page has no stored HTML |
| `spiderlens rules run CRAWL [--rules FILE] [--only NAME]...` | Runs the file's stored rules (or FILE's rules, which are then stored in the crawl) on stored HTML; progress on stderr; Ctrl+C cancels and records a partial run | 0 complete, 130 cancelled, "not possible" when the file has no stored HTML or is read-only |
| `spiderlens export CRAWL --tab custom_extraction\|custom_search [--all-values SEP]` | Exports the custom tabs; with `--all-values`, each extractor cell holds every value joined by SEP | as `export` |

`--json` on `rules test` prints `[{ "name", "kind", "count", "flags": [...], "values": [...] }]`.
