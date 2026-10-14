# Desktop commands (Tauri) for spec 015

| Command | Arguments | Returns | Notes |
|---|---|---|---|
| `aeo_run` | `{ path, profile? }` | `{ run_id }` | Emits `aeo://progress { run_id, phase, done, total }` and `aeo://finished { run_id, summary \| error }` |
| `aeo_cancel` | `{ run_id }` | `()` | Leaves the previous results in place |
| `aeo_summary` | `{ path }` | `Summary \| null` | Read-only |
| `aeo_page` | `{ path, url_id }` | `PageResult \| null` | Detail section |
| `aeo_llms_txt_draft` | `{ path, max_links }` | `{ text }` | Saved by the UI through the save dialog |
| `aeo_probe` | `{ path, bots, pages_per_host }` | `{ run_id }` | UI shows a confirm dialog first; emits `aeo://probe-progress` |
| `ai_bots_list` | | `{ version, date, bots: AiBot[] }` | Built-in plus custom |
| `ai_bots_save_custom` | `{ bots: AiBot[] }` | `()` | Validates tokens |
| `aeo_profiles_list` / `aeo_profile_save` | | | Scoring profiles |

The AI search tab itself uses the existing results commands (`results_view`, `results_counts`, `results_export`) with tab id `ai-search`; types are generated with ts-rs into `ui/src/generated/`.
