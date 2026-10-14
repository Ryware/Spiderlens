# Implementation Plan: Scheduled Crawls

**Branch**: `021-scheduled-crawls` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

A schedule is a small JSON record owned by SpiderLens. The operating system
only knows one thing about it: "start this SpiderLens program at minute MM of
every hour, and once at login". Everything else (which run is due, catch-up,
missed runs, overlap, history, naming, after-run steps) is decided by our own
code, which is pure, clock-injected and tested on every OS in CI.

1. **`crates/schedule`** (new library): the schedule model and store, the
   cron evaluator, the due-run decision, the runner (crawl, save, after-run
   steps, history), and one `Scheduler` back end per OS behind a trait.
2. **CLI**: `spiderlens schedule list|show|add|edit|duplicate|enable|disable|remove|run-now|history|remove-all|tick`.
3. **App** (`app/src-tauri/src/schedules.rs`): commands for the UI; the desktop
   executable also accepts `--schedule-tick` and runs headless (no window), so
   the shipped bundle needs no extra binary.
4. **UI** (`ui/src/schedules/`): a Schedules area in the sidebar with the list,
   an editor sheet and run history; "Schedule this crawl" in the toolbar menu.

**No crawl file format change.** The schedule name, id and scheduled time are
written to the crawl's `meta` table under the key `schedule` (a key-value
row, no new table or column), so 021 takes **no migration step**.

## Technical Context

- Rust 2024; existing workspace deps `jiff` (time zones, DST), `serde_json`,
  `thiserror`, `clap`, `tempfile`. `std::fs::File::try_lock` for the per-schedule
  run lock (as 006 does for crawl files).
- No notification crate: notifications go through the OS's own commands
  (`osascript` on macOS, a PowerShell toast on Windows, `notify-send` on
  Linux) behind a `Notifier` trait; tests use a recorder. `dirs` for the config
  directory in the CLI (the app gets it from Tauri; both resolve the same
  folder, D6).
- No crate for cron or for the OS schedulers: the cron subset is ~200 lines and
  the back ends write a text file and call `launchctl`, `schtasks` or
  `systemctl`/`crontab` (principle VIII).
- Builds on 002 (config, validation, secrets), 007 (CSV export in
  `crates/results`). 006 (#27) adds the recent list and the disk reserve;
  FR-014's reserve check and FR-016 plug in when #27 is on main (T030, T031).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec 021 approved 2026-10-13; this plan is a separate docs PR. |
| II. Local-first | OS schedulers only; no service, no account. |
| III. Engine independent of shell | All logic in `crates/schedule`; the CLI and the app are thin callers. Everything the app does is a CLI command (FR-028). |
| IV. Respectful crawling | Runs use the same engine and stored defaults; a schedule cannot run more than hourly (FR-005). |
| V. Correct and complete data | Every run gets a history record before the crawl starts and a final one after; a crash leaves a `running` record that the next tick turns into Interrupted (D4). |
| VI. Scales by design | The tick reads one small JSON file and exits in milliseconds when nothing is due. The crawl itself is the normal engine path. |
| VII. Test-first | Due logic, cron, DST, naming and history are pure functions with a fake clock; back ends have golden-file tests on all OSes and a real registration test where the CI runner allows (Windows always; Linux and macOS when a user session exists). |
| VIII. Simplicity | One registration shape for every frequency (D1); no resident process; one new crate dependency. |

No violations.

## Design decisions

### D1. The OS sends an hourly tick; SpiderLens decides what is due

Schedules may run at most hourly and the minute field is one value (FR-005),
so every schedule has a fixed minute MM. Each schedule registers:

| OS | Registration | Missed while off or asleep |
|---|---|---|
| macOS | `~/Library/LaunchAgents/com.ryware.spiderlens.schedule.<id>.plist` with `StartCalendarInterval {Minute: MM}` and `RunAtLoad`, loaded with `launchctl bootstrap gui/<uid>` | launchd fires once on wake; `RunAtLoad` fires at login |
| Windows | Task `\SpiderLens\<id>` from XML via `schtasks /Create /XML`: daily trigger at `00:MM` repeating every hour for one day, a logon trigger, `StartWhenAvailable`, `MultipleInstancesPolicy=IgnoreNew`, `InteractiveToken`, no time limit, run on battery | `StartWhenAvailable` |
| Linux | `~/.config/systemd/user/spiderlens-schedule-<id>.{service,timer}`, `OnCalendar=*-*-* *:MM:00`, `Persistent=true`, `OnStartupSec=1min`; `systemctl --user enable --now`. Fallback when `systemctl --user` fails: tagged lines in the user crontab (`MM * * * *` and `@reboot`) | systemd catches up elapsed timers on resume and with `Persistent`; cron relies on `@reboot` |

The command is `<program> schedule tick <id>` (CLI) or
`<program> --schedule-tick <id>` (desktop executable). The tick:

1. Reads the schedule; exits 0 if it is disabled or gone.
2. Computes `due(schedule, last_record, now)` (D3). Nothing due: exit 0.
3. Takes the run lock; if held, records Skipped and exits 7.
4. Runs the crawl (D5).

Why not translate each frequency to native triggers: launchd and Task
Scheduler cannot express "last day of month" or arbitrary cron lists without
large expansions, and every translation is a separate source of bugs per OS.
An hourly no-op process start costs a few milliseconds.

### D2. Frequencies are stored as one cron expression

`every N hours`, `daily`, `weekly`, `monthly` and `advanced` are editor
presets that produce a five-field expression (`minute hour dom month dow`)
plus the preset kind for display. Supported syntax: `*`, numbers, lists,
ranges, `*/n` and `a-b/n`, `L` in day of month, `0` or `7` for Sunday,
standard OR rule when both day fields are restricted. The minute field must be
one number (FR-005). `next_after(expr, tz, instant)` walks days forward (at
most 5 years, else "never runs") and builds civil times with `jiff`:

- a gap (spring forward): the run goes to the first valid minute after the gap;
- a fold (fall back): the earlier instant is used, and since the search is
  strictly after the last run, the second occurrence is skipped.

### D3. The due decision is one pure function

```text
due(schedule, last, now) ->
    Nothing                         // no slot in (last_slot, now]
  | Run { slot, trigger: Scheduled } // newest slot within 5 min of now
  | Run { slot, trigger: CaughtUp }  // newest slot older, catch-up on
  | Missed { slot }                  // newest slot older, catch-up off
```

`last_slot` is the scheduled time of the newest history record (any status),
or the schedule's `created_at` if there is none. Only the newest missed slot
counts, so several missed times give one run or one Missed record (FR-012).
Run now bypasses `due` with trigger `RunNow` and does not move `last_slot`
(the regular schedule is unchanged, US1-5).

### D4. History is append-only JSON lines

`<config>/schedules/history/<id>.jsonl`. A run appends a `running` record
before the first request and its final record at the end, both keyed by run
number; readers keep the last record per run. A `running` record whose
schedule lock is free when read is reported as Interrupted (and rewritten so
on the next tick). Files are small (hundreds of bytes per run); the reader
returns the newest N. History survives deleting the crawl files (FR-018).

### D5. The runner

1. Check the output folder exists and is writable (create a probe file).
2. Read the List file if List mode (re-read every run, FR-001), validate the
   configuration (002 `validate`).
3. File name `<name> YYYY-MM-DD HHmm.spiderlens` in local time of the slot;
   characters invalid in file names are replaced by `-`; ` (2)`, ` (3)` if it
   exists (FR-015).
4. `Crawler::create`, then `Crawler::set_meta("schedule", {...})` (new
   two-line engine method), then `run`. No window; events are counted only.
5. Final status from the summary: finished, stopped at limit, paused (disk,
   after 006), failed with the engine error.
6. After-run steps in order (FR-023): CSV exports via
   `spiderlens_results::export`, then report (010) and comparison (012) when
   those crates exist. Each step's result goes into the run record.
7. Notification per level (FR-019).

Stop (FR-017): the app writes `<config>/schedules/<id>.stop`; the runner
checks for it every second and sends `Control::Stop`, then deletes it. The
app shows live counts by reading the run record's `counts`, which the runner
rewrites at most every 5 seconds while running, and opens the crawl file
read-only to watch it.

Tests set `SPIDERLENS_NOW` to fix the clock for `tick` (ignored in release
builds unless `SPIDERLENS_TEST=1` is also set).

### D6. Where schedules live and who writes them

`<config>/schedules/schedules.json` where `<config>` is Tauri's
`app_config_dir()` (`<OS config dir>/com.ryware.spiderlens`); the CLI uses
`dirs::config_dir().join("com.ryware.spiderlens")`, overridable with
`SPIDERLENS_CONFIG_DIR` (used by tests). Writes go through
`schedules.json.lock` + write to temp + rename, so the app and the CLI can both
edit. Each schedule holds its own `CrawlConfig` copy, the source profile name
and a hash of the profile at copy time, so the editor can say "profile
changed" (FR-003).

Secrets: `CrawlConfig` headers marked secret have no stored value; a schedule
whose config has any secret header is refused (FR-006). Integration
credentials (FR-006a) come with 024; the runner gets a `credentials` hook
that reads the OS keychain then env vars, unused until then.

### D7. Registration upkeep

Each registration embeds the program path. On app start the app compares
each schedule's registration (`Scheduler::inspect`) with its own executable
path and re-registers if it differs or is missing (FR-025); results are shown
once as a notice. `remove-all` lists our registrations by prefix
(`com.ryware.spiderlens.schedule.*`, `\SpiderLens\`, `spiderlens-schedule-*`,
tagged cron lines) and removes them, including orphans (FR-026).

### D8. UI

Sidebar item **Schedules** (clock icon) under Overview. The view is a
three-column list (name, next run, last run with status chip), a detail pane
on the right with the run history, and an editor in a sheet (same pattern
as Settings). "Schedule this crawl" in the toolbar's more-menu and the command
palette prefill the editor from the current settings (FR-002). Frequency
presets show the next five run times live (FR-005). All copy and colours use
the existing components and tokens; no new colours.

## Performance impact

None on the crawl path. The tick: one process start, one JSON read, exit
(measured target under 50 ms on the reference machine, T022).

## Project Structure

```text
crates/schedule/
  src/lib.rs          model, errors
  src/cron.rs         parse, validate, next_after (D2)
  src/due.rs          due decision (D3)
  src/store.rs        schedules.json, locking (D6)
  src/history.rs      JSONL history (D4)
  src/runner.rs       tick and run (D5)
  src/naming.rs       file names
  src/os/mod.rs       Scheduler trait, Registration, Fake
  src/os/launchd.rs   plist text + launchctl
  src/os/taskschd.rs  task XML + schtasks
  src/os/systemd.rs   unit text + systemctl, cron fallback
  tests/              golden files, end-to-end tick with the fixture site
crates/cli/src/schedule_cmd.rs
crates/engine/src/crawl/runner.rs   + Crawler::set_meta
app/src-tauri/src/schedules.rs      commands, --schedule-tick, startup repair
ui/src/schedules/                   SchedulesView, ScheduleEditor, RunHistory, useSchedules, tests
```
