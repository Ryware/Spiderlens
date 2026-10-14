# Desktop commands: report and exports (spec 010)

| Command | Arguments | Returns |
|---|---|---|
| `report` | `{ running: bool }` | `Report` |
| `export_report_html` | `{ path, title, note, urls_per_issue, overwrite }` | `{ path, bytes }` |
| `print_report` | `{ title, note, urls_per_issue }` | `()`; opens the OS print dialog for PDF |
| `bulk_export` | `BulkOptions` | `BulkResult`; emits `export_progress` events |
| `cancel_export` | `()` | `()` |

Events: `export_progress` with `BulkProgress`. Errors are strings with the same messages as the CLI. Existing paths are refused with `exists` unless `overwrite` is set after the user confirms.

App setting (UI settings, not the crawl profile): `openReportOnFinish: boolean` (default false), `reportFile: { title, note, urlsPerIssue }`.
