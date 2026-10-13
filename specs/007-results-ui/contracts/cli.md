# CLI: export (FR-031)

```
spiderlens export <crawl-file> --tab <tab> [--filter <filter>] [--scope all|internal|external]
                  [--search <term>] [--sort <column>[:desc]] [--columns a,b,c]
                  [--out <file.csv>]          # default: stdout
spiderlens export --list-tabs [--json]        # tabs, filters and column ids
```

- Rows, order and default columns are identical to the desktop view with
  the same tab, filter, search and sort (same `spiderlens-results` code).
- Output is the same CSV as the desktop export (BOM, quoting, formula
  neutralising). With `--out`, written to `<file>.part` and renamed.
- Exit codes: 0 done, 2 bad arguments (unknown tab, filter or column, with
  the valid ids listed), 3 file cannot be opened, 4 filter unavailable for
  this file (reason printed).
