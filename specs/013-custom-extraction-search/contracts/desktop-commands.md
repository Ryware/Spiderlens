# Desktop Commands: Custom Rules

Tauri commands in `app/src-tauri/src/custom.rs`, mirrored in
`ui/src/transport.ts` and `ui/src/testing/fakeTransport.ts`.

| Command | Input | Output |
|---|---|---|
| `custom_validate` | `CustomRules` | `RuleError[]` (`{ kind, index, field, message, position? }`) |
| `custom_test_page` | `{ rules, crawl_path, url_id }` | `RuleTestResult[]` |
| `custom_test_url` | `{ rules, url, config }` | `RuleTestResult[]` or a fetch error |
| `custom_run` | `{ crawl_path, rules?, only? }` (rules stored with the crawl when `rules` is absent; `only` = rule names) | run id; progress events `custom-run-progress { run, done, total, per_sec }`, then `custom-run-done { run, path, state, pages, error }` |
| `custom_cancel` | run id | () |
| `custom_rules_of_crawl` | crawl path | `{ name, kind, last_run_at, run_pages, run_state }[]` |
| `custom_import` / `custom_export` | file path (dialog); export also takes `rules` | `CustomRules` / () |
| `url_custom` | `{ path, id }` | `CustomResult[]`: every rule's count, flags, values and error on one page |

`custom_test_page` and `custom_test_url` return
`{ url, status, results: RuleTestResult[], ms }` with
`RuleTestResult = { name, kind, count, flags: string[], values: string[], error }`.

The custom tabs come through the existing `results_tabs` command, which now
takes an optional crawl `path` and then returns the static tabs plus that
crawl's custom tabs; no new view command. The detail pane reads rule results
with `url_custom`. `export_start` takes an optional `all_values` separator
(FR-025).
