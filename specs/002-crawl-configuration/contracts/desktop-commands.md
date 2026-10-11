# Desktop commands (Tauri), via `ui/src/transport.ts`

| Command | Input | Output |
| --- | --- | --- |
| `config_defaults` | none | `CrawlConfig` |
| `config_validate` | `CrawlConfig`, `ModeInput` | `Issue[]` |
| `config_presets` | none | user agent and speed presets |
| `patterns_test` | include[], exclude[], sample URLs | per URL: decision, pattern index |
| `list_parse_text` | pasted text | `ListSummary` (counts, invalid sample) |
| `list_parse_file` | path | same; emits `list-progress` events |
| `profiles_list` | none | names, default flag |
| `profile_load` / `profile_save` / `profile_rename` / `profile_duplicate` / `profile_delete` | name, config | result or `NameTaken` |
| `profile_set_default` | name or null | |
| `profile_export` / `profile_import` | path | import returns ignored settings |
