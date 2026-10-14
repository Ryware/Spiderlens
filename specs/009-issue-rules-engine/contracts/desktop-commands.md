# Desktop commands: issues

All return the ts-rs types in data-model.md; all run on the results reader thread.

| Command | Arguments | Returns |
|---|---|---|
| `rule_catalogue` | none | `RuleInfo[]` with the active profile's settings applied |
| `issues_summary` | none | `IssuesSummary` for the open crawl (provisional while running) |
| `url_issues` | `url_id` | `UrlIssue[]` for one URL |
| `set_issue_settings` | `IssueSettings` | `IssueSettingsResult`; saves to the active profile, re-evaluates, writes meta when the file is writable |
| `open_view` (007) | `ViewQuery { tab: "issues", filter: "<rule id>", ... }` | unchanged 007 behaviour |

Event: the existing results `changed` event carries the issues summary version so the sidebar refetches only when it changed.
