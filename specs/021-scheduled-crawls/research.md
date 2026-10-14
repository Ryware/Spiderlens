# Research: Scheduled Crawls (spec 021)

**Date**: 2026-10-13. Input for `plan.md`; the spec stays free of these choices.

## Question

How does a crawl run at a set time when the SpiderLens window is closed, on macOS, Windows and Linux, with no server (Principle II)?

## Options

| Option | Runs with app closed | Survives reboot | Missed runs | Cost |
|---|---|---|---|---|
| A. Timer inside the open app | No | No | Lost | Lowest, but does not meet the ask |
| B. App keeps a background process (tray or menu-bar agent) with its own timer | Yes, while the agent runs | Needs a login item | Own logic | A resident process all day; must be kept alive, updated and started at login on three OSes |
| C. **OS scheduler starts the SpiderLens command line** | Yes | Yes | Built into each OS | One registration per schedule; no resident process |

## Recommendation: C, the operating system's own scheduler

| OS | Mechanism | Missed runs | Notes |
|---|---|---|---|
| macOS | Per-user LaunchAgent plist in `~/Library/LaunchAgents`, `StartCalendarInterval`, loaded with `launchctl bootstrap gui/<uid>` | launchd runs a calendar job once on wake if the time passed while asleep; not after power-off, so the app adds its own check (below) | No admin rights. Program path is the CLI inside the `.app` bundle; moving the app breaks it, so FR-025 repairs on launch |
| Windows | Task Scheduler task in a `\SpiderLens\` folder, created through the Task Scheduler COM API or `schtasks /Create /XML`, trigger per frequency, `StartWhenAvailable=true`, `MultipleInstancesPolicy=IgnoreNew`, "run only when user is logged on" | `StartWhenAvailable` covers both sleep and power-off | No admin rights and no stored password because of "only when logged on" |
| Linux | systemd user timer and service in `~/.config/systemd/user`, `OnCalendar=`, `Persistent=true`; fallback to the user crontab when systemd user sessions are not available | `Persistent=true` covers sleep and power-off; cron fallback relies on the app's check | `loginctl enable-linger` is NOT used (that would run while logged out) |

Why C over B:

- Nothing runs between runs, so no memory, no battery cost and no process to keep alive (speed and footprint are the owner's priority).
- The OS already solves reboot, sleep, missed runs and single instance (`IgnoreNew`, launchd never overlaps a job, systemd never starts an active unit twice).
- The run is just the command line crawling (Principle III), so a scheduled crawl and `spiderlens crawl` share every line of crawl code.

Weaknesses and how the plan covers them:

- Three registration back ends to write and test. They sit behind one small trait (`Scheduler` with `register`, `suspend`, `remove`, `inspect`) in a new crate, with a fake for unit tests and one real registration test per OS in CI.
- macOS after power-off and the Linux cron fallback do not catch up. The CLI keeps a `last_run` per schedule; on every login a tiny "check missed" registration (LaunchAgent `RunAtLoad`, cron `@reboot`) runs `spiderlens schedule catch-up`, which starts at most one run per schedule (FR-012).
- Cron-format expressions do not map one-to-one onto launchd and Task Scheduler triggers. The schedule is expanded to the OS's trigger list where possible; otherwise it is registered as a 1-hour or 15-minute tick that asks the CLI "is anything due?" and exits in milliseconds when nothing is.
- The app moving or updating changes the program path: verified and repaired on every app start (FR-025).

## Where schedules live

One file per user, `schedules.json` in the app config directory next to the profiles store (spec 002), written atomically; run history in a small SQLite file in the same directory, since it grows and is queried by schedule. The OS registration only carries `spiderlens schedule run <id>`; the schedule itself is never duplicated in the OS entry, so app and CLI see one source of truth (FR-029).

## Credentials

The run is a process of the logged-in user, so it can read the same OS keychain entries the app wrote for integrations (spec 024): macOS Keychain, Windows Credential Manager, Linux Secret Service, through one keyring crate shared with 024. Environment variables override for CLI and CI use. On Linux without a Secret Service (headless), the keychain is unavailable and only environment variables work; the run records the integration step as failed with that reason.

## Notifications

The run is a command-line process. On macOS and Linux it posts through the desktop notification service; on Windows through a toast with the app's id. Clicking opens `spiderlens://schedule/<id>/run/<n>` in the app. Exact crates go in the plan.

## Open points for the plan

- The exit code number for "skipped" (next free after 6).
- Whether the run history belongs in `crates/config` or a new `crates/schedule` crate (leaning new crate: the OS back ends are platform code the config crate should not carry).
