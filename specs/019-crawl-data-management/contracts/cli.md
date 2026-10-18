# CLI: Crawl Data Management

## `spiderlens files [DIR] [--json]`

Lists `.spiderlens` files directly in DIR (default `.`), newest last-used first.

```
NAME                      SIZE     HTML     URLS    STATUS    LAST USED
ryware-dev.spiderlens     412 MB   301 MB   446     finished  2026-10-13 14:02
```

`--json`: an array of `ManagedCrawl` (data-model.md). Exit 0; 2 if DIR is not a folder.

## `spiderlens purge-html FILE [--no-compact] [--yes] [--json]`

Removes all stored HTML from FILE, then compacts it unless `--no-compact`. Without `--yes`, prints the space to be freed and asks for `y`.

Exit codes: 0 done; 2 usage; 6 local error (006), including the file being locked by another process and not enough free space to compact (message says how much is needed and suggests `--no-compact`).

## `spiderlens schedule add|edit ... --keep-last N|all`

Spec 021 commands gain one option. `N` is a whole number from 1 to 999: the schedule keeps its last N runs. `all` keeps every run. `add` without the option uses 10. `edit` without it leaves the value as is. A value outside 1 to 999 exits 2 with the allowed range.

`schedule show` prints `Keep last: 10 runs` or `Keep last: all`; `--json` has `keep_last` (number or null). A schedule saved before this feature shows `all`.

The CLI only stores and shows the rule. It never removes run files; the desktop app's cleanup does (FR-033, plan D11).
