# Desktop commands: schedules

All return `Result<T, String>`; errors are plain-language messages.

| Command | Arguments | Returns |
|---|---|---|
| `schedules_list` | | `ScheduleRow[]`: schedule fields + `next_runs[0]`, `last` (Run?), `registration`, `running` (counts when a run is active) |
| `schedule_get` | `id` | `ScheduleDetail`: schedule + `next_runs` (5) + `profile_changed` |
| `schedule_preview` | `frequency` | `{ next_runs: string[], error?: string }` (FR-005, live in the editor) |
| `schedule_validate` | `draft` | `Issue[]` (002 shape) |
| `schedule_save` | `draft` (with or without `id`) | `ScheduleDetail`; registers when enabled |
| `schedule_set_enabled` | `id`, `enabled` | `ScheduleRow` |
| `schedule_duplicate` | `id`, `name` | `ScheduleRow` |
| `schedule_delete` | `id` | nothing |
| `schedule_run_now` | `id` | nothing; starts the tick process with Run now in the background |
| `schedule_stop` | `id` | nothing; asks the running process to stop (stop file, D5) |
| `schedule_history` | `id`, `limit` | `Run[]` |
| `schedule_update_from_profile` | `id` | `ScheduleDetail` |
| `schedules_repair` | | `{ repaired: string[], failed: [{ name, reason }] }` |
| `schedules_remove_all` | | nothing |

Starting the editor from the current settings is UI-only: it builds a draft
from the current config and start URL.
