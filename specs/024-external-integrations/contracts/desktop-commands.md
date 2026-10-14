# Desktop commands: External Data Integrations

Tauri commands in `app/src-tauri/src/integrations.rs`, thin wrappers over `spiderlens-integrations`. Types are generated with ts-rs (data-model.md).

| Command | Input | Output |
|---|---|---|
| `integrations_status` | | `ProviderStatus[]`, credential store kind |
| `integrations_settings_save` | provider, enabled, limits overrides | `ProviderStatus` |
| `integrations_google_client` | client id, secret? (empty id: built-in client) | |
| `integrations_connect_google` | scopes, own client? | `Connection` (opens the browser; cancellable) |
| `integrations_connect_cancel` | | |
| `integrations_save_key` | provider, label, key | `Connection` |
| `integrations_forget` | provider, label | revoked: bool |
| `integrations_properties` | provider, label | `PropertyChoice[]` (suggested one first for the open crawl) |
| `integrations_estimate` | `RunRequest` | `RunEstimate` |
| `integrations_run_start` | `RunRequest` | run id (refused while a crawl is running) |
| `integrations_run_pause` / `_resume` / `_cancel` | run id | `RunProgress` |
| `integrations_runs` | | `RunProgress[]` for the open crawl |
| `integrations_queries` | url id | top queries (fetched on demand, cached) |
| `integrations_orphans` | source?, page | `OrphanCandidate[]` page |
| `integrations_remove` | provider or all | |
| `integrations_export_sheets` | tab, view query, split/truncate | `SheetExport` |
| `integrations_open_looker` | `SheetExport` | opens the browser |
| `integrations_cache_info` / `_clear` | provider? | size, entries |

Event `integrations://progress` carries `RunProgress` at most every 200 ms.
