# Feature Specification: Scheduled Crawls

**Feature Branch**: `021-scheduled-crawls`

**Created**: 2026-10-13

**Status**: Draft (waiting for the owner's approval)

**Input**: Owner approval of the feature-gap report (row 10, "Scheduling: recurring crawls with automatic exports", 2026-10-13), with the brief: define recurring crawls (profile, start URL, interval or cron-like schedule), run them without the app window open where the operating system allows, save each run as its own crawl file named by date, optionally run exports or the end-of-crawl report and the comparison against the previous run afterwards, with a schedule list and run history in the app and the command line. Desktop only, no servers, secrets never stored in SpiderLens files.

**Research**: [research.md](research.md) compares the ways to run a crawl while the app is closed and recommends using each operating system's own scheduler.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Crawl a site every week without opening the app (Priority: P1)

A consultant audits a client site every Monday morning. They pick a saved profile and a start URL, choose "Weekly, Monday 07:00", and close the app. Every Monday at 07:00 the crawl runs in the background and leaves a dated crawl file in the chosen folder, ready to open when they sit down.

**Why this priority**: This is the feature. Everything else adds to it.

**Independent Test**: Create a schedule that runs in two minutes, quit the app, wait, and confirm a new crawl file with the expected name exists and opens with complete results.

**Acceptance Scenarios**:

1. **Given** a saved profile and a start URL, **When** the user creates a schedule "Weekly, Monday 07:00", **Then** the schedule list shows it with its next run time in the user's local time.
2. **Given** an enabled schedule and the app quit, **When** the scheduled time arrives while the user is logged in and the computer is awake, **Then** the crawl starts within one minute without opening any window, and on completion a crawl file named `<schedule name> <YYYY-MM-DD HHmm>` is in the schedule's output folder.
3. **Given** the app is open when a scheduled run starts, **When** the user looks at the schedule list, **Then** the run shows as running with live URL counts, and the user can open it to watch it read-only or stop it.
4. **Given** a completed scheduled run, **When** the user opens its crawl file, **Then** it holds the same data and configuration record as a crawl started by hand (specs 002 and 006).
5. **Given** a schedule, **When** the user chooses Run now, **Then** a run starts immediately with the same settings and naming, and the regular schedule is unchanged.

---

### User Story 2 - See what ran and what went wrong (Priority: P1)

The consultant comes back from holiday and wants to know which runs happened, which failed and why, and to open any run with one click.

**Why this priority**: Unattended work that fails silently is worse than no automation (Principle V).

**Independent Test**: Make one run succeed, one fail (unreachable start URL) and one be missed (computer off), then check the history shows all three with the right status and reason.

**Acceptance Scenarios**:

1. **Given** a schedule with past runs, **When** the user opens its run history, **Then** each run shows start and end time, status, URL counts by final state, the crawl file, the results of any after-run steps, and for failures a plain-language reason.
2. **Given** a run whose start URL cannot be reached, **When** it ends, **Then** its status is Failed with the reason, its crawl file is kept, and the next run still happens on time.
3. **Given** the computer was off at the scheduled time, **When** the user next logs in or wakes the computer, **Then** the missed run happens once within five minutes (if catch-up is on) or is recorded as Missed (if off); several missed times never cause several runs.
4. **Given** a run is still going when the next one is due, **When** the next time arrives, **Then** no second run starts and the history records Skipped: previous run still running.
5. **Given** notifications are set to "Problems only" (default), **When** a run fails or is missed, **Then** the operating system shows a notification naming the schedule and the reason, and clicking it opens the run in the app; a successful run shows no notification.

---

### User Story 3 - Get the exports, report and comparison automatically (Priority: P2)

After each weekly run, the consultant wants the CSV exports they send to the client, the end-of-crawl report, and a comparison against last week's run, without clicking anything.

**Why this priority**: Scheduling with automatic exports is how the gap was described; the run alone already saves time.

**Independent Test**: Configure a schedule with a CSV export of two tabs and a comparison, let two runs happen, and confirm the export files exist next to the second crawl and the comparison names the first run as its baseline.

**Acceptance Scenarios**:

1. **Given** a schedule with after-run exports chosen (any results tab that can be exported, spec 007), **When** a run finishes, **Then** the exports are written to a folder named after the run, next to its crawl file.
2. **Given** the end-of-crawl report (spec 010) is chosen, **When** a run finishes, **Then** the report is produced in the same folder in the formats the report supports.
3. **Given** "Compare with the previous run" (spec 012) is chosen and an earlier finished run of the same schedule exists, **When** a run finishes, **Then** a comparison against the most recent earlier finished run is produced and its headline numbers appear in the run history; when no earlier run exists, the step is recorded as Skipped: no earlier run.
4. **Given** an after-run step fails (for example the output folder is not writable), **When** the run ends, **Then** the crawl itself is still recorded as finished, the failed step and its reason are shown separately, and with "Problems only" notifications the user is told.
5. **Given** spec 010 or 012 is not yet available in the installed version, **When** the user edits a schedule, **Then** the corresponding step is not offered.

---

### User Story 4 - Manage schedules from the command line (Priority: P2)

A developer keeps schedules in a script for several sites and wants to list, add, change, pause and inspect them without the app (Principle III).

**Why this priority**: Everything the app can do must be possible from the command line.

**Independent Test**: Create a schedule with the command line, see it in the app, disable it in the app, and see it disabled in the command line listing.

**Acceptance Scenarios**:

1. **Given** the command line, **When** the user adds a schedule with a profile file or options, a start URL or a URL list file, a frequency and an output folder, **Then** the schedule is created and registered exactly as one created in the app, and appears in the app's list.
2. **Given** schedules exist, **When** the user lists them or shows a run history, **Then** the output gives the same information as the app, also as JSON on request.
3. **Given** a schedule, **When** the user enables, disables, edits, removes or runs it now from the command line, **Then** the effect is identical to doing it in the app.

---

### User Story 5 - Schedules keep working after updates and moves (Priority: P3)

The user updates SpiderLens, or moves the app to another folder, and expects existing schedules to keep running.

**Why this priority**: Rarely needed, but a broken schedule after an update is a silent failure.

**Independent Test**: Register a schedule, move the app, launch it once, and confirm the next run happens.

**Acceptance Scenarios**:

1. **Given** schedules registered by an earlier version or from another app location, **When** the app starts, **Then** it checks every registration and repairs any that point to a missing or outdated program, telling the user what it repaired.
2. **Given** a registration was removed outside the app (for example by a system cleaner), **When** the app starts, **Then** the schedule is marked Not registered with a Repair action.
3. **Given** the user wants to remove SpiderLens, **When** they choose Remove all schedules, **Then** every registration the app made is removed from the operating system.

### Edge Cases

- The schedule's configuration includes a value that is never stored (a marked-secret header, spec 002 FR-038, or a site password or login cookie, spec 025): the schedule cannot be created; the app explains that an unattended run could not supply it.
- The schedule uses an integration (spec 024) whose key or token is not in the operating system keychain, or was removed with Forget: the run still crawls, the integration step is recorded as Failed: credentials missing, and with "Problems only" the user is told.
- The computer is asleep at the scheduled time: the run happens on wake as a missed run (User Story 2, scenario 3). Waking the computer to run is out of scope.
- The user is logged out: no run happens; on next login the missed-run rule applies.
- Daylight saving time: a time that does not exist that day runs at the first valid minute after it; a time that occurs twice runs once.
- The output folder is missing or not writable at run time: the run fails before any request is made, with the reason.
- Free disk space is below the safety reserve (spec 006): the run does not start and is recorded as Failed: low disk space; a run that runs out of space during the crawl pauses as in spec 006 and is recorded as Paused: disk, resumable by hand.
- A crawl file with the run's name already exists (two runs in the same minute through Run now): a suffix ` (2)` is added; nothing is overwritten.
- The schedule's profile is changed or deleted after the schedule was made: the schedule keeps its own copy and keeps running; the editor shows that the source profile changed and offers Update from profile.
- The app is open with the same crawl file it would write: impossible by naming; every run writes a new file.
- Two schedules start at the same time: both run, each respecting its own speed settings; per-host politeness does not coordinate across separate runs (stated in the editor when two schedules target the same host at the same time).
- The program is quit or the computer shuts down mid-run: the crawl is recovered as in spec 006 on next open and the run is recorded as Interrupted; it is not restarted automatically.
- The operating system refuses the registration (policy, permission): the schedule is saved as Not registered with the system's reason.

## Requirements *(mandatory)*

### Functional Requirements

**Defining schedules**

- **FR-001**: Users MUST be able to create a schedule from the desktop app with: a name, a crawl configuration taken from a saved profile or from the current settings, the crawl input (Spider mode start URL, or List mode file path re-read at every run), a frequency, an output folder, and the after-run steps of FR-020 to FR-023.
- **FR-002**: Users MUST be able to start creating a schedule from the crawl settings ("Schedule this crawl") and from an open crawl ("Schedule again"), with its configuration and input filled in.
- **FR-003**: A schedule MUST keep its own copy of the configuration at the time it is created or updated. Later changes to the source profile MUST NOT change the schedule; the editor MUST show when the source profile differs and offer Update from profile.
- **FR-004**: Frequencies MUST include: every N hours (1 to 24), daily at a time, weekly on chosen days at a time, monthly on a day of the month (1 to 28, or last day) at a time, and an advanced expression in the standard five-field cron format. Times MUST be in the computer's local time zone.
- **FR-005**: The editor MUST show the next five run times for the chosen frequency, and MUST refuse a frequency that would never run or runs more often than once per hour.
- **FR-006**: The system MUST refuse to save a schedule whose configuration is invalid (spec 002 FR-007) or needs a value that is never stored (marked-secret headers, site passwords, login cookies), explaining which setting is the cause.
- **FR-006a**: Integration API keys and OAuth tokens (spec 024) MUST be read at run time from the operating system keychain of the logged-in user, never from the schedule, the crawl file, the run history or logs. On the command line, an environment variable named by spec 024 MUST override the keychain value for that run. A missing credential MUST fail only the steps that need it (FR-023).
- **FR-007**: Users MUST be able to edit, duplicate, enable, disable, delete and Run now any schedule. Deleting a schedule MUST NOT delete its crawl files; it MUST remove its operating system registration.
- **FR-008**: Users MUST be able to choose per schedule whether a missed run is caught up (default on) and the notification level: Always, Problems only (default), Never.

**Running without the app**

- **FR-009**: An enabled schedule MUST run at its times while the user is logged in and the computer is awake, whether or not the app is open, using only facilities of the operating system and SpiderLens itself (Principle II).
- **FR-010**: A scheduled run MUST open no window and MUST NOT take focus. It MUST use the same crawl engine, configuration handling, storage, respectful-crawling defaults and notices as a crawl started by hand.
- **FR-011**: A run MUST start within one minute of its scheduled time when the computer is awake.
- **FR-012**: When a scheduled time passed while the computer was off, asleep or the user was logged out, the system MUST, with catch-up on, start one run within five minutes of the next login or wake, and record the run as caught up; with catch-up off it MUST record one Missed entry. Several missed times MUST produce at most one run.
- **FR-013**: At most one run of a schedule MUST be active at a time; a run due while another is active MUST be recorded as Skipped with the reason.
- **FR-014**: Before making any request, a run MUST check the output folder is writable and free space is above the safety reserve (spec 006 FR-027); otherwise it MUST end as Failed with the reason and no crawl file.
- **FR-015**: Each run MUST save its crawl as a saved crawl file (not an unsaved crawl) in the output folder, named `<schedule name> <YYYY-MM-DD HHmm>` in local time with the SpiderLens extension, adding ` (2)`, ` (3)` and so on rather than overwriting. The crawl's stored information MUST record the schedule name and identifier and the scheduled time.
- **FR-016**: Scheduled runs MUST appear in the recent crawls list (spec 006 FR-024) marked as scheduled.
- **FR-017**: When the app is open during a run, it MUST show the run as running with live counts, let the user watch it read-only, and let the user stop it. Stopping MUST end the run as Stopped by user.

**History and notifications**

- **FR-018**: The system MUST keep a run history per schedule with, for each run: scheduled time, actual start and end, status (Finished, Stopped at limit, Stopped by user, Failed, Paused: disk, Interrupted, Skipped, Missed), URL counts by final state, crawl file path, each after-run step with its result, and a plain-language reason for every status other than Finished. History MUST survive app restarts and updates and MUST NOT be lost when crawl files are moved or deleted (such entries show the file as unavailable).
- **FR-019**: Depending on the schedule's notification level, the system MUST show an operating system notification when a run ends, naming the schedule, the status and for problems the reason; activating the notification MUST open the app on that run.

**After-run steps**

- **FR-020**: Users MUST be able to choose CSV exports of any exportable results tabs (spec 007) to run after each finished run, written to a folder `<run name> exports` next to the crawl file.
- **FR-021**: When spec 010 is available, users MUST be able to choose the end-of-crawl report and any of its export formats as after-run steps.
- **FR-022**: When spec 012 is available, users MUST be able to choose "Compare with the previous run"; the baseline MUST be the most recent earlier run of the same schedule with status Finished or Stopped at limit whose file is still available.
- **FR-023**: After-run steps MUST run only for runs that produced a crawl file, in the order exports, report, comparison; a failing step MUST NOT change the crawl's status or stop the following steps, and MUST be shown with its reason.

**Registration and maintenance**

- **FR-024**: Enabling, editing or Run now MUST update the operating system registration; disabling MUST suspend it; deleting MUST remove it. If the system refuses, the schedule MUST be marked Not registered with the system's reason.
- **FR-025**: On every app start the system MUST verify each schedule's registration against the installed program location and version, repair what it can, and report what it repaired or could not repair.
- **FR-026**: Users MUST be able to Remove all schedules in one action, removing every registration the app made.
- **FR-027**: Schedules belong to the current user account on the current computer; they MUST NOT require administrator rights and MUST NOT run for other users.

**Command line**

- **FR-028**: The command line MUST be able to list, show, add, edit, duplicate, enable, disable, remove and run now schedules, show run history, and remove all schedules, with the same validation and effect as the app, and JSON output on request.
- **FR-029**: Schedules created in the app and on the command line MUST be the same schedules, visible and editable in both.
- **FR-030**: The program that performs a scheduled run MUST exit with a status code that distinguishes finished, failed, skipped and disk-paused runs, consistent with the crawl exit codes of specs 004 and 006.

### Key Entities

- **Schedule**: identifier, name, enabled flag, configuration copy (with the source profile name and when it was copied), crawl input (start URL or list file path), frequency, output folder, catch-up flag, notification level, after-run steps, registration state (Registered, Suspended, Not registered with reason), next run time.
- **Run**: schedule identifier, scheduled time, actual start and end, trigger (scheduled, caught up, run now), status and reason, URL counts, crawl file path, after-run step results.
- **After-run step**: kind (export, report, comparison), its options, and per run its result (done with output paths, skipped with reason, failed with reason).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On each supported operating system, a schedule created with the app closed afterwards runs at its time and produces a complete, openable crawl file in 100% of automated end-to-end runs.
- **SC-002**: Runs start within 60 seconds of the scheduled time while the computer is awake.
- **SC-003**: After a missed time, exactly one catch-up run starts within 5 minutes of login or wake, for any number of missed times.
- **SC-004**: No schedule ever has two runs active at once, and no run ever overwrites an existing crawl file.
- **SC-005**: A crawl from a scheduled run has the same results as the same crawl started by hand from the same configuration against the same fixture site.
- **SC-006**: A user can schedule an existing crawl configuration in 5 interactions or fewer from the crawl settings.
- **SC-007**: Every run that does not finish has a status and plain-language reason in the history; 0 silent failures in the test suite.
- **SC-008**: After moving the app or installing a newer version, 100% of registrations are verified on first launch and repaired where the operating system allows.

## Assumptions

- Runs happen only while the user is logged in. Running while logged out would need the user's password stored in the operating system and is out of scope.
- The computer is not woken to run; missed runs are caught up instead.
- Integration keys and OAuth tokens live in the operating system keychain (owner's decision 2026-10-13, spec 024); scheduled runs read them from there. Site passwords, login cookies (025) and marked-secret headers stay unstored (spec 002 FR-038, spec 006 FR-034), so schedules needing them are refused.
- Retention of scheduled run files (for example keep the last N runs) belongs to spec 019. Until it lands, scheduled runs are kept like any saved crawl. Spec 019 is asked to add a per-schedule "keep the last N runs" rule.
- Comparison (012) and the report (010) are defined by their own specs; this spec only says when they run and where their output goes.
- Desktop only. No server, no remote triggering, no sharing schedules between computers.

## Out of Scope

- Waking the computer, running when logged out, and skipping runs on battery power.
- Email, Slack or other delivery of results (no servers; may come with spec 024 integrations).
- Exporting and importing schedules as files.
- Chaining schedules (run B after A).
- Coordinating politeness between separate runs that hit the same host.
