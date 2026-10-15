# Desktop commands: report and exports (spec 010)

| Command | Arguments | Returns |
|---|---|---|
| `report` | `{ path, running, profile }` | `Report` |
| `export_report_html` | `{ path, dest, file: FileOptions, overwrite }` | `{ path, bytes }` |
| `print_report` | `{ path, file: FileOptions }` | `()`; opens the report in its own window and the OS print dialog for PDF |
| `bulk_export` | `{ path, options: BulkOptions }` | `BulkResult`; emits `bulk-export-progress` events |
| `bulk_export_cancel` | `()` | `()` |

`path` is the open crawl file, as in the spec 007 commands. Events: `bulk-export-progress` with `BulkProgress` (named apart from the spec 007 `export-progress` event). Errors are strings with the same messages as the CLI. Existing paths are refused with `exists` unless `overwrite` is set after the user confirms.

App setting (UI settings, not the crawl profile): `openReportOnFinish: boolean` (default false), `reportFile: { title, note, urlsPerIssue }`.
