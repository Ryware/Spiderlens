# Feature Specification: Crawl Data Management and Cleanup

**Feature Branch**: `019-crawl-data-management`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions

**Input**: User description: "Did we discuss management and cleanup of the stored crawl data?" Owner direction for the draft: a crawl files manager in the app (list with size and date, open, delete to the trash, purge stored HTML from a file, total disk usage), automatic cleanup only of unsaved crawls and leftover temporary files with a configurable age, and never silent deletion of saved crawls. Added 2026-10-18 (owner): each schedule keeps its last 10 runs by default and moves older run files to the trash.

**Numbering note**: 009 to 016 and 017, 018 are taken or reserved in the product backlog, so this spec takes 019.

## Overview

Crawl storage (spec 006) makes crawls safe: every crawl is one file, unsaved crawls live in an app folder until the user saves them, and nothing is ever lost on a crash. What it deliberately left out is the other half of the lifecycle. Today a user can delete an unsaved crawl one at a time (permanently, from the start screen) and sees the space unsaved crawls use. Nothing else exists:

- Saved crawls can only be removed from the recent list; the files stay on disk and the app gives no view of how much space crawls take overall.
- Page HTML stored by page extraction (spec 005, "Store page HTML") and, later, rendered HTML (spec 014) is usually the largest part of a crawl file, and there is no way to drop it while keeping the results.
- Unsaved crawls are kept forever (006 assumption: "no automatic deletion in M1"), so a user who runs many quick crawls fills the app folder without noticing.
- A crash or forced quit can leave helper files behind: interrupted Save As copies (`.partial`), lock files of crawls that were deleted, write-ahead log files, and migration backups (`(format N backup)`) that are never removed.
- The 1 GB free-space safety reserve is fixed; 006 promised it would be user-configurable.

This feature adds one place to see and manage all crawl data, ways to shrink a crawl without losing results, and a narrow, opt-out automatic cleanup for data the user never chose to keep. Saved crawls are the user's documents: the app never deletes them on its own. The one exception is a scheduled run (spec 021) beyond that schedule's "keep the last N runs" limit, which the user chose when setting up the schedule (User Story 6).

This feature belongs to milestone M2. It builds on crawl storage (006), page extraction (005) and the design system shell (017), and is extended by JavaScript rendering (014) when that stores rendered HTML.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See all crawl data and how much space it uses (Priority: P1)

A user opens "Manage crawls" from the File menu, the command palette or the start screen. One list shows every crawl the app knows about: unsaved crawls, saved crawls from the recent list, and crawl files the user added to the list. For each crawl it shows the name or start URL, where it is (unsaved or the folder), last used date, created date, URL count, status, file size, and how much of that size is stored page HTML. A total at the top shows the space used by all listed crawls, by unsaved crawls, and by leftover files that can be cleaned up, plus the free space on the drive that holds unsaved crawls.

**Why this priority**: Without a view of what exists and what it costs, every other cleanup action is guesswork. It is also useful on its own.

**Independent Test**: Create three unsaved crawls and two saved ones of known sizes (one with stored HTML), open Manage crawls, and check every row and the totals against the files on disk.

**Acceptance Scenarios**:

1. **Given** unsaved and saved crawls exist, **When** the user opens Manage crawls, **Then** all of them are listed with size, dates, URL count and status, and the totals match the sum of the files on disk within 1%.
2. **Given** the list is open, **When** the user sorts by size, date or name, or filters to unsaved, saved or unavailable, **Then** the list updates without reloading the files.
3. **Given** a saved crawl whose file was moved or deleted outside the app, **When** the list is shown, **Then** the entry is marked unavailable with "Locate" and "Remove from list", as in 006 FR-026.
4. **Given** a crawl is running, **When** the list is shown, **Then** that crawl is marked running and its size updates while it grows.
5. **Given** a crawl in the list, **When** the user chooses "Show in folder", "Open", or "Crawl information", **Then** the file manager, the crawl, or the 006 crawl information window opens for it.

---

### User Story 2 - Delete crawls safely (Priority: P1)

From the manager the user selects one or several crawls and chooses Delete. The app shows how many crawls, which ones are saved files outside the app folder, and how much space will be freed, and asks for confirmation. Deleted crawls go to the operating system's trash or recycle bin with their helper files, so a mistake can be undone there.

**Why this priority**: Deleting is the main way users get space back, and doing it file by file in the file manager misses helper files and leaves stale entries in the app.

**Independent Test**: Select two unsaved crawls and one saved crawl, delete them, then check that all three files and their helper files are in the trash, the entries are gone from the manager and the recent list, and restoring one from the trash and opening it works.

**Acceptance Scenarios**:

1. **Given** one or more selected crawls, **When** the user chooses Delete, **Then** a confirmation names the count, marks saved files separately, shows the space to be freed, and nothing is deleted until the user confirms.
2. **Given** the user confirms, **When** the delete runs, **Then** each crawl file and its helper files (lock, log and partial copies) are moved to the trash, and the entries leave the manager and the recent list.
3. **Given** the trash is not available (for example a network drive or a system without one), **When** the user deletes, **Then** the confirmation says the delete is permanent before the user confirms.
4. **Given** a selected crawl is running, open in another window, or locked by another process, **When** the user deletes, **Then** that crawl is skipped with the reason, and the others are still deleted.
5. **Given** the open crawl is selected and not running, **When** the user deletes it, **Then** the app closes it first and returns to the start screen.
6. **Given** "Remove from list" on a saved crawl, **When** the user chooses it, **Then** only the entry goes away and the file stays on disk (006 FR-026 unchanged).

---

### User Story 3 - Make a crawl smaller without losing results (Priority: P2)

A user wants to keep a crawl's results but not its stored HTML. From the manager or the crawl information window they choose "Remove stored HTML". The app shows how much space this frees, warns that the HTML viewer and any later re-extraction or rendering of old pages will no longer work for this crawl, and after confirmation removes the stored HTML and compacts the file. All result columns, issues and exports stay exactly the same.

**Why this priority**: Stored HTML is usually most of a crawl file's size, so this gives most of the space back while keeping the work.

**Independent Test**: Take a crawl with stored HTML, note its size and an export of every results tab, remove the stored HTML, then check the size dropped by at least the reported amount and every export is byte-identical to before.

**Acceptance Scenarios**:

1. **Given** a crawl with stored HTML, **When** the user chooses Remove stored HTML, **Then** the confirmation shows the space to be freed and what stops working, and nothing changes until the user confirms.
2. **Given** the user confirms, **When** the purge completes, **Then** the file is smaller by at least 90% of the reported HTML size, every results tab exports the same data as before, and the crawl information records the purge with its date.
3. **Given** a purge is running, **When** the app crashes or the disk fills, **Then** the crawl reopens complete, either with or without the stored HTML, never damaged (006 crash safety applies).
4. **Given** there is not enough free space to compact the file, **When** the user starts a purge, **Then** the app says how much space is needed before starting, and offers to remove the HTML without compacting (space is reused by the crawl but not returned to the drive).
5. **Given** a crawl that is running, **When** the user opens its actions, **Then** Remove stored HTML is unavailable with the reason; it is available when the crawl is paused, stopped or finished.
6. **Given** a resumed crawl after a purge, **When** it fetches new pages with Store page HTML on, **Then** those pages store HTML again as usual.

---

### User Story 4 - Unsaved crawls and leftovers clean themselves up (Priority: P2)

A user who runs many quick crawls never saves most of them. The app removes unsaved crawls not opened for a set number of days (default 30) and leftover helper files from crashes, moving them to the trash. Saved crawls are never touched by this age rule (scheduled runs follow their own rule, User Story 6). Before an unsaved crawl is removed for the first time, the app tells the user once that this setting exists and where to change it.

**Why this priority**: It prevents the unsaved folder from silently growing to many gigabytes, which is the most likely way the app fills a disk.

**Independent Test**: Create unsaved crawls with last-used dates of 10, 31 and 90 days ago, a saved crawl last used a year ago, a stale `.partial` file and an orphan lock file, launch the app, and check that only the 31- and 90-day unsaved crawls and the leftovers went to the trash.

**Acceptance Scenarios**:

1. **Given** automatic cleanup is on (the default) with 30 days, **When** the app starts, **Then** unsaved crawls not opened for more than 30 days are moved to the trash, and a short notice says how many and how much space was freed, with a link to the trash and to the setting.
2. **Given** the app starts, **When** cleanup runs, **Then** saved crawls, crawls in the recent list that live outside the app folder, the open crawl and any crawl that is running, paused or locked are never removed, whatever their age.
3. **Given** leftover helper files with no crawl file next to them (lock files, write-ahead log files, interrupted Save As copies) older than 24 hours, **When** cleanup runs, **Then** they are removed; helper files of an existing crawl are never removed.
4. **Given** the user sets the age to another value (7, 14, 30, 90 days) or turns automatic cleanup off in Settings › Storage, **When** the app next starts, **Then** the new rule applies; with cleanup off, no unsaved crawl is ever removed automatically.
5. **Given** an unsaved crawl would be removed for the first time ever on this machine, **When** cleanup runs, **Then** it is not removed; instead the app shows a one-time notice naming the crawls that will be removed at the next start, so the user can save them or change the setting.
6. **Given** cleanup runs, **When** it removes anything, **Then** each removal is listed in an app-level cleanup history (date, file, size, reason) that the user can open from Settings › Storage.

---

### User Story 5 - Clean up migration backups and choose the safety reserve (Priority: P3)

When a crawl is upgraded to a newer file format, 006 keeps a backup copy next to it. In the manager these backups appear under their crawl, with their size, and the user can delete them like any crawl. In Settings › Storage the user can also set the free-space safety reserve that pauses a crawl before the disk is full.

**Why this priority**: Backups are useful for a while and then only waste space; the reserve setting was promised by 006 and belongs with the other storage settings.

**Independent Test**: Open an older-format crawl so a backup is created, check it appears under the crawl in the manager with its size, delete it, then set the reserve to 5 GB and check a crawl pauses when free space falls below 5 GB.

**Acceptance Scenarios**:

1. **Given** a crawl with a migration backup, **When** the manager lists it, **Then** the backup appears under that crawl with its format version, date and size, and can be opened view-only or deleted.
2. **Given** the user sets the safety reserve to a value between 256 MB and 50 GB, **When** a crawl runs and free space drops below it, **Then** the crawl pauses as in 006 FR-027 with the new value in the message.
3. **Given** the user enters a reserve outside the allowed range, **When** they save, **Then** the value is refused with the allowed range shown.

### User Story 6 - Each schedule keeps only its last runs (Priority: P2)

A user has a daily scheduled crawl (spec 021). Every run saves a new crawl file, so the folder grows forever. Each schedule gets a "Keep the last" setting: 10 runs by default, any number from 1 to 999, or "Keep all". When a schedule has more run files than that, the oldest ones are moved to the trash automatically. Only run files that the schedule itself recorded, and that are still where it saved them, are ever considered. Anything the user moved, renamed or saved elsewhere is left alone.

**Why this priority**: Spec 021 asked for it: a daily schedule that is never trimmed fills a disk. The user picks the limit, so this is not a silent deletion of work.

**Independent Test**: Make a schedule with keep-last 3 and 5 finished run files, move one of them to another folder, and run cleanup. Check that only the oldest run files beyond the newest 3 that still exist in the schedule's folder went to the trash, and the moved one is untouched.

**Acceptance Scenarios**:

1. **Given** a schedule with "keep last 3" and 5 run files in its output folder, **When** cleanup runs, **Then** the 2 oldest (lowest run numbers) go to the trash with their helper files, and the newest 3 stay.
2. **Given** a schedule saved before this feature (no setting stored), **When** cleanup runs, **Then** it keeps all runs and nothing is removed.
3. **Given** a new schedule, **When** the user creates it without touching the setting, **Then** it keeps the last 10 runs.
4. **Given** a run file the user moved, renamed or saved elsewhere, **When** cleanup runs, **Then** it is not removed, and it does not count toward the limit.
5. **Given** a run is in progress, or its crawl is open or being written, **When** cleanup runs, **Then** that file is never removed in this pass.
6. **Given** the trash is not available, **When** cleanup runs, **Then** nothing is removed and the same files are tried again in the next pass. Nothing is ever deleted permanently by this rule.
7. **Given** a schedule is set to "Keep all", **When** cleanup runs, **Then** none of its runs are removed.
8. **Given** run files were removed by this rule, **When** the user opens Settings > Storage, **Then** each removal is in the cleanup history with the reason "scheduled run kept-last limit", and the cleanup notice says how many older scheduled runs were moved to the trash.
9. **Given** the user lowers the limit on a schedule, **When** they save it, **Then** the app says how many existing run files will go to the trash at the next cleanup.

### Edge Cases

- A crawl file lives on a removable or network drive that is not connected: it is listed as unavailable with its last known size, excluded from totals, and never deleted by cleanup.
- Two app windows or processes run at once: only one runs automatic cleanup at a time; the other skips it.
- The system clock jumps backwards or forwards: cleanup uses the crawl's last-used date recorded by the app, and a date in the future is treated as "used now".
- The user restores an unsaved crawl from the trash into the app folder: it is listed again, and because the cleanup history shows automatic cleanup already removed it once, automatic cleanup never removes it again; the user saves or deletes it by hand.
- A crawl file is renamed or moved in the file manager while the manager is open: the list refreshes when the window regains focus and marks the old entry unavailable.
- A delete or purge fails part way (permissions, file in use by another program): the crawl stays complete, the action reports which file failed and why, and nothing is listed as freed that was not.
- A file in the app folder that is not a crawl (a user's own file): it is listed under "Other files" with its size, never deleted automatically, and can be revealed in the file manager.
- Very large crawls (tens of gigabytes): sizes and totals appear without reading the file contents; the stored-HTML size may appear a moment later, with a placeholder until then.
- A crawl with no stored HTML: Remove stored HTML is not offered.
- The crawl was created by a newer app version (006 view-only): it can be deleted, but Remove stored HTML is not offered.
- A schedule is deleted: its run files are no longer governed by any rule and are kept like any saved crawl.
- Two run records point to the same file, or a failed run has no file: only existing files named by a run's own record count, once each.
- The user opens an older scheduled run: while it is open it is never removed. After it is closed it is judged again at the next pass.
- A schedule's output folder is on a drive that is not connected: its files do not exist at the recorded path, so none are counted or removed.

## Requirements *(mandatory)*

### Functional Requirements

**Crawl manager**

- **FR-001**: The app MUST provide a "Manage crawls" view reachable from the File menu, the command palette and the start screen.
- **FR-002**: The manager MUST list every unsaved crawl in the app folder and every crawl in the recent list, plus files the user adds with "Add crawl file", showing for each: name or start URL, location, created and last-used dates, URL count, status, file size, and stored page HTML size.
- **FR-003**: The manager MUST show totals for all listed crawls, for unsaved crawls, for removable leftover files, and the free space on the drive holding unsaved crawls.
- **FR-004**: File sizes and totals MUST appear within 2 seconds for 200 listed crawls on the reference hardware, without reading crawl contents; stored-HTML sizes MAY load afterwards.
- **FR-005**: The manager MUST support sorting by name, size, created and last-used date, filtering by unsaved, saved and unavailable, and multiple selection.
- **FR-006**: For each crawl the manager MUST offer Open, Show in folder, Crawl information, Save As (unsaved crawls), Remove stored HTML (FR-012), Remove from list (saved crawls) and Delete.

**Deleting**

- **FR-007**: Deleting MUST require confirmation that states the number of crawls, which are saved files outside the app folder, and the space to be freed.
- **FR-008**: Deleting MUST move the crawl file and all its helper files to the operating system's trash or recycle bin. When the trash is unavailable for a file, the confirmation MUST say the delete is permanent for that file before the user confirms.
- **FR-009**: Deleting MUST skip, with a stated reason, any crawl that is running, held by another process or opened by another window; the remaining crawls MUST still be deleted.
- **FR-010**: Deleting the crawl open in the window MUST close it first; a running crawl MUST be stopped by the user before it can be deleted.
- **FR-011**: The existing start-screen delete for unsaved crawls (006 FR-023) MUST use the same confirmation and trash behaviour.

**Removing stored HTML**

- **FR-012**: Users MUST be able to remove all stored page HTML (and, once 014 lands, stored rendered HTML) from a crawl that is not running, after a confirmation that shows the space to be freed and what will stop working.
- **FR-013**: Removing stored HTML MUST NOT change any other stored data: every results tab, filter count, issue and export MUST be identical before and after.
- **FR-014**: Removing stored HTML MUST be crash-safe under 006 rules: after any interruption the crawl opens complete, with or without the HTML.
- **FR-015**: After removal the app MUST return the freed space to the drive by compacting the file, checking first that enough free space exists for compaction and offering removal without compaction when it does not.
- **FR-016**: The crawl's event history MUST record each HTML removal with its date and the space freed, and the crawl information window MUST show it.

**Automatic cleanup**

- **FR-017**: At app start (and at most once per day while it runs), the app MUST move to the trash unsaved crawls in the app folder whose last-used date is older than the configured age. Default: on, 30 days; choices 7, 14, 30, 90 days, or off.
- **FR-018**: Automatic cleanup by age MUST NEVER remove a saved crawl, a crawl outside the app folder, a file that is not a crawl, the open crawl, or a crawl that is running, paused, locked or recovering.
- **FR-019**: Automatic cleanup MUST remove leftover helper files in the app folder and next to recent crawls that belong to no existing crawl file and are older than 24 hours: lock files, write-ahead log and shared-memory files, and interrupted Save As copies.
- **FR-020**: The first time automatic cleanup would remove an unsaved crawl on a machine, it MUST instead show a one-time notice naming those crawls and the setting, and remove them no earlier than the next start.
- **FR-021**: After any automatic cleanup that removed something, the app MUST show a short notice with the count and space freed, and links to the trash and the setting.
- **FR-022**: The app MUST keep a cleanup history of the last 500 automatic and manual removals (date, path, size, reason) viewable from Settings › Storage.
- **FR-023**: When free space on the unsaved-crawls drive is below the safety reserve at start-up (006 FR-031), the warning MUST offer to open the manager sorted by size.

**Backups and settings**

- **FR-024**: Migration backups created by 006 MUST appear in the manager under their crawl with format version, date and size, and MUST be deletable like crawls. Automatic cleanup MUST NOT remove them.
- **FR-025**: Settings MUST gain a Storage section with: automatic cleanup on/off and age, the free-space safety reserve (256 MB to 50 GB, default 1 GB, fulfilling 006 FR-027), the location of the unsaved-crawls folder (read-only, with Show in folder), and the cleanup history.

**CLI**

- **FR-026**: The command-line tool MUST be able to list crawl files in a folder with their sizes and stored-HTML sizes, and remove stored HTML from a given crawl file, with the same crash safety and results guarantees. The CLI MUST NOT perform automatic cleanup.

**Scheduled run retention**

- **FR-027**: Each schedule MUST have an optional keep-last limit: a number of runs from 1 to 999, or none (keep all). Schedules created from now on MUST default to 10. Schedules saved before this feature MUST read as keep all.
- **FR-028**: The desktop app MUST move to the trash the run files of a schedule that are older than its newest N existing run files. Order is by run number, newest first.
- **FR-029**: Only a file that is named by a run in that schedule's own history, and that still exists at that recorded path inside that schedule's output folder, MUST be counted or removed. A file the user moved, renamed or saved elsewhere MUST NEVER be touched.
- **FR-030**: The rule MUST NEVER remove a run that is still in progress, nor a crawl that is open or being written (it holds the writer lock). Such a file is skipped for this pass.
- **FR-031**: The removal MUST use the same trash path as other deletes (FR-008), with the crawl's helper files. When the trash is unavailable, nothing MUST be removed and the next pass MUST try again. The rule MUST NEVER delete permanently.
- **FR-032**: Each removal MUST be a cleanup history record (FR-022) with the reason `schedule_retention`, and the cleanup notice (FR-021) MUST say how many older scheduled runs were moved to the trash.
- **FR-033**: The command-line tool MUST be able to set and show the rule (`schedule add` and `schedule edit` with `--keep-last N|all`, shown by `schedule show`). The CLI MUST NOT remove run files.

### Key Entities

- **Managed crawl**: one crawl file the app knows about: location, saved or unsaved, dates, URL count, status, file size, stored-HTML size, availability, helper files, migration backups.
- **Helper file**: a file the app creates next to a crawl file (lock, write-ahead log, shared memory, interrupted copy). It belongs to the crawl with the same base name; without one it is a leftover.
- **Cleanup rule**: on/off and age in days for unsaved crawls; fixed 24 hours for leftovers.
- **Cleanup record**: one removal: date, path, size, reason (manual delete, automatic age, leftover, HTML removal, scheduled run kept-last limit).
- **Keep-last rule**: a property of a schedule (spec 021): `keep_last`, a number from 1 to 999 or none for keep all.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can find the largest crawl and free its space (delete or remove stored HTML) in under 30 seconds from the start screen.
- **SC-002**: Removing stored HTML from a crawl where HTML is at least half the file reduces the file size by at least 40% and leaves every results export byte-identical.
- **SC-003**: Automatic cleanup never removes a saved crawl, other than a scheduled run beyond its schedule's keep-last limit, in any test, including clock changes, moved files and crashes during cleanup.
- **SC-004**: After 100 simulated crashes during crawls, Save As and deletes, launching the app leaves no leftover helper files older than 24 hours in the app folder.
- **SC-005**: Every crawl deleted from the app can be restored from the operating system's trash and opened, on all three supported operating systems.
- **SC-006**: The manager lists 200 crawls with sizes in under 2 seconds on the reference hardware.
- **SC-007**: In every test, the keep-last rule removes only files named in the schedule's own run history that exist inside its output folder, never the newest N, never an open, locked or in-progress file, and never anything when the trash is unavailable.

## Assumptions

- "App folder" means the folder where 006 keeps unsaved crawls; its location does not change in this spec.
- "Last used" is the date the app last opened, resumed or saved the crawl, as recorded in the recent list or the file's metadata, not the file system's modified date.
- Using the trash is the safest default because it matches what users expect from deleting a document; deletes that cannot use the trash are allowed only after an explicit warning.
- Thirty days for unsaved crawls balances disk space against users who return to an unsaved crawl after a few weeks. It can be tuned in planning.
- Size of stored HTML is known per crawl without reading every page, so the manager stays fast; how is a planning decision.
- Removing stored HTML is all-or-nothing per crawl in this spec; removing it only for some pages, or only rendered HTML, is out of scope.
- Spec 021 owns the schedule and its run history. This spec owns the rule that trims run files (decided with the owner 2026-10-18: default 10 per schedule, configurable 1 to 999 or keep all).
- Out of scope: automatic deletion of saved crawls by age or size (except scheduled runs beyond a schedule's keep-last limit, FR-027), a disk quota for the app folder, moving the unsaved-crawls folder, compressing or archiving crawls, deleting exported CSV files or settings profiles, cloud storage, and managing crawls on other machines.
