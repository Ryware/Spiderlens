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
