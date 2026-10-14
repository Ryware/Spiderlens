# CLI contract: issues

```text
spiderlens rules [--json] [--category <name>]
    Print the rule catalogue: id, name, category, default priority, threshold, why, fix.

spiderlens issues <crawl-file> [--json] [--all] [--min-priority <p>]
                  [--issues <settings.json> | --profile <name>]
                  [--fail-on <high|medium|low|opportunity>]
    Print issues with counts, grouped by priority (High first), sorted by count.
    --all          include rules with zero, unavailable or turned off
    --min-priority hide issues below this priority
    Settings: --issues or --profile, else the file's stored issue_settings, else defaults.

spiderlens issues <crawl-file> --export <dir> [--issue <id>]...
    Write one CSV per issue (or per given --issue) with the same rows and
    columns as the desktop export. File name: <issue-id>.csv.

spiderlens export <crawl-file> --tab issues --filter <issue-id> [-o file.csv]
    Same as the 007 export, for one issue.
```

Exit codes: 0 success; 2 usage or invalid settings; `ISSUES_FOUND` (next free code at merge, expected 7) when `--fail-on` is given and an enabled issue at or above that priority has affected URLs.

JSON (`--json`) is the `IssuesSummary` and `RuleInfo` shapes from data-model.md.
