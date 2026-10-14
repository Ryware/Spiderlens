# CLI contract: schedules

All commands accept `--json`. Schedules are found by id or by name.

| Command | Effect |
|---|---|
| `spiderlens schedule list` | name, id, enabled, frequency, next run, last status, registration |
| `spiderlens schedule show <s>` | everything in the schedule plus next 5 runs |
| `spiderlens schedule add --name N (URL \| --list FILE) --out DIR (--hourly N \| --daily HH:MM \| --weekly mon,thu@HH:MM \| --monthly D@HH:MM \| --cron "EXPR") [--no-catch-up] [--notify always\|problems\|never] [--export TAB[:FILTER]]... [crawl options \| --profile FILE]` | creates and registers; exit 2 with the issues if invalid or a secret is needed |
| `spiderlens schedule edit <s> [same options]` | changes only the given options, re-registers |
| `spiderlens schedule duplicate <s> --name N` | copy, disabled |
| `spiderlens schedule enable <s>` / `disable <s>` | registers / removes the registration |
| `spiderlens schedule remove <s>` | removes schedule and registration; crawl files stay |
| `spiderlens schedule run-now <s>` | runs in the foreground with progress, like `crawl` |
| `spiderlens schedule history <s> [--limit N]` | newest first |
| `spiderlens schedule remove-all [--yes]` | removes every schedule and every SpiderLens registration, orphans included |
| `spiderlens schedule tick <id>` | what the OS calls (plan D1); quiet |

## Exit codes (`tick` and `run-now`)

| Code | Meaning |
|---|---|
| 0 | finished, stopped at limit, or nothing due |
| 2 | usage, schedule not found |
| 4 | run failed (reason in history and on stderr) |
| 5 | stopped by user (`run-now`, Ctrl-C) |
| 6 | paused by a disk or write problem (spec 006) |
| 7 | skipped: previous run still running |
