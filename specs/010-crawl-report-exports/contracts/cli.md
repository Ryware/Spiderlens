# CLI contract: report and exports (spec 010)

## spiderlens report

Existing behaviour (URL lists, `--json` summary) is unchanged. New flags:

```
spiderlens report <FILE> --audit [--json]
spiderlens report <FILE> --html <OUT> [--title <TEXT>] [--note <TEXT>] [--urls-per-issue <N>] [--force]
    [--issues <FILE> | --profile <NAME>]
```

- `--audit`: prints the report: score, breakdown by priority, top issues, completeness, then each section's figures. With `--json`, one JSON object, the `Report` value of data-model.md.
- `--html`: writes the self-contained report file. Refuses an existing file unless `--force`.
- Issue settings: `--issues` or `--profile`, else the file's stored settings, else defaults (spec 009 plan D9).

## spiderlens export

Existing single-view export is unchanged. New flags:

```
spiderlens export <FILE> --bulk <TABLES|all> --format <csv|xlsx> --out <PATH> [--force]
    [--issues <FILE> | --profile <NAME>]
```

- `TABLES`: comma separated from `tabs, issues, links, images, redirect_chains, headers, summary`.
- `--format csv` writes a folder at `PATH`; `--format xlsx` writes one workbook at `PATH`.
- Progress lines go to standard error; the list of files written goes to standard output.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | done |
| 2 | usage error (unknown table, bad limit, existing target without `--force`) |
| 3 | crawl file cannot be opened or read |
| 6 | local problem writing the output (disk full, permission), spec 006 |

No partial output is left on any non-zero exit.
