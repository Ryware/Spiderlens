# Quickstart: Scheduled Crawls

Run against the local fixture site (`cargo run -p spiderlens-test-fixtures --bin fixture-site`).

1. Create a schedule for the next minute (local time):
   `spiderlens schedule add --name demo http://127.0.0.1:PORT/ --out /tmp/demo --cron "$(date -d '+1 min' +'%M %H') * * *"`
   Expect: `registered` in `spiderlens schedule list`.
2. Quit everything and wait. Expect `/tmp/demo/demo YYYY-MM-DD HHmm.spiderlens`
   and `spiderlens schedule history demo` showing `finished` (US1-2).
3. `spiderlens schedule run-now demo` twice within a minute. Expect a second
   file with ` (2)` (edge case) and the regular schedule unchanged (US1-5).
4. Start `run-now` and, while it runs, call `spiderlens schedule tick <id>` at
   a due time. Expect exit 7 and a `skipped` record (US2-4).
5. Point the start URL at a closed port. Expect `failed` with "could not
   connect" and the next run still registered (US2-2).
6. Set the clock-free check: `SPIDERLENS_NOW=2026-10-20T09:00 spiderlens schedule tick <id>`
   (test-only override) after a slot at 07:00 with catch-up on gives one
   `caught_up` run; with `--no-catch-up` one `missed` record (US2-3).
7. App: Schedules area lists `demo` with next run; open it to see the history;
   edit to weekly and see the five next runs update (FR-005).
8. Move the app (or edit the registration's program path by hand), launch
   the app: notice "Repaired 1 schedule" (US5-1).
9. `spiderlens schedule remove-all --yes`: no `spiderlens` entries remain in
   `launchctl list`, `schtasks /Query /TN \SpiderLens\`, or
   `systemctl --user list-timers` (US5-3).
