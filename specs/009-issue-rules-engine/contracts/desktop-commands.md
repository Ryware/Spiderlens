# Desktop commands: issues

All return the ts-rs types in data-model.md; the reading ones run on the
results reader thread of the file named by `path`.

| Command | Arguments | Returns |
|---|---|---|
| `rule_catalogue` | `settings: IssueSettings` | `RuleCatalogue { rules: RuleInfo[], notices }` with those settings applied (defaults when they have errors) |
| `issues_summary` | `path`, `running` | `IssuesSummary` for that crawl (site-wide counts provisional while running) |
| `url_issues` | `path`, `id` | `UrlIssue[]` for one URL, with evidence |
| `issues_apply` | `path` (or null), `settings: IssueSettings` | `SettingsNotice[]`; rejected without change when any notice is an error. Otherwise applies to every open reader and export, updates the built hits in place, and records `rules_version` and `issue_settings` in a writable file in the background. The settings sheet saves the overrides to the active profile like every other category. |
| `analysis_run` | `path` | `AnalysisReport`; runs the Crawl Analysis passes missing or stale on that file (plan D12) |
| `open_view` (007) | `ViewQuery { tab: "issue:<rule id>", filter: "all", ... }` | unchanged 007 behaviour; `results_tabs` lists one `issue:` tab per rule |

Events:

- `analysis-progress` (`AnalysisProgress { pass, label, done, total }`):
  the Crawl Analysis phase at crawl end (before `crawl-finished`) and on
  opening a writable file. The status bar shows "Analysing: <label> N%".
- `issues-partial` (`{ path, summary: IssuesSummary }`, every count
  provisional): counts so far, about once a second, while `issues_summary`
  evaluates a file for the first time (FR-008).
- The sidebar refetches the summary on new crawl data (the 007 refresh)
  and after each applied settings change.
