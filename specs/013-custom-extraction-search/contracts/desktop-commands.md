# Desktop Commands: Custom Rules

Tauri commands in `app/src-tauri/src/custom.rs`, mirrored in
`ui/src/transport.ts` and `ui/src/testing/fakeTransport.ts`.

| Command | Input | Output |
|---|---|---|
| `custom_validate` | `CustomRules` | `RuleError[]` (`{ kind, index, field, message, position? }`) |
| `custom_test_page` | `{ rules, crawl_path, url_id }` | `RuleTestResult[]` |
| `custom_test_url` | `{ rules, url, config }` | `RuleTestResult[]` or a fetch error |
| `custom_run` | `{ crawl_path, rule_ids? }` | run id; progress events `custom-run-progress { done, total, per_sec }`, then `custom-run-done { state }` |
| `custom_cancel` | run id | () |
| `custom_rules_of_crawl` | crawl path | `StoredRule[]` with `last_run_at`, `run_pages`, `run_state`, match count |
| `custom_import` / `custom_export` | file path (dialog) | `CustomRules` / () |

`RuleTestResult = { name, kind, count, flags: string[], values: string[], ms }`.

The custom tabs come through the existing `results_tabs` command, which now
returns the static tabs plus the crawl's custom tabs; no new view command.
The detail command gains `custom: CustomDetail[]`.
