# Feature Specification: Crawl Storage and Crawl Files

**Feature Branch**: `006-crawl-storage`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Crawl storage and crawl files: crawl results are saved continuously to disk as the crawl runs, so memory stays flat; the user can pause, stop, save a crawl as one file, open it later (also after quitting the app or a crash) and resume an unfinished crawl; a recent-crawls list; crawl files record their configuration and app version and remain openable by newer app versions; disk-full or write failures pause the crawl safely with a clear message."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Crawl results are never lost (Priority: P1)

An SEO specialist starts a large crawl and leaves it running. Every result is written to disk as soon as it is collected, not held in memory. If the app crashes, the computer loses power or the user force-quits, the next launch offers to recover the crawl, and almost everything collected so far is still there. Memory use stays roughly the same whether the crawl has 10,000 or 1,000,000 URLs.

**Why this priority**: Large crawls take hours. Losing them to a crash, or having the app slow down and run out of memory as the crawl grows, makes the product unusable for its core audience. Everything else in this feature builds on results living on disk.

**Independent Test**: Run a crawl against the local 100,000-page fixture site, measure memory throughout, then kill the app process mid-crawl, relaunch, recover the crawl and compare the recovered results with what had been collected before the kill.

**Acceptance Scenarios**:

1. **Given** a crawl is running, **When** each URL reaches a final state, **Then** its result is written to the crawl's file on disk within a few seconds, without waiting for the crawl to finish or for the user to save.
2. **Given** a crawl of the 100,000-page fixture site, **When** the crawl progresses from 10,000 to 100,000 crawled URLs, **Then** the app's memory use stays flat (see SC-002) instead of growing with the number of URLs.
3. **Given** a crawl is running, **When** the app process is killed without warning, **Then** on the next launch the app offers to recover that crawl, and the recovered crawl contains every result that was collected more than 5 seconds before the kill.
4. **Given** a recovered crawl, **When** the user opens it, **Then** it is shown as unfinished (paused) with accurate counts of crawled, failed and still-queued URLs, and can be resumed.
5. **Given** a crawl that was interrupted while writing, **When** it is recovered, **Then** no URL appears twice, no result is half-written, and every URL is either in a recorded final state or back in the queue.

---

### User Story 2 - Pause, stop and resume a crawl (Priority: P1)

The user can pause a running crawl and resume it later, including after closing the app or restarting the computer. The user can also stop a crawl, which ends it as incomplete; an incomplete crawl can still be resumed later from its file. Resuming continues from where the crawl left off and never re-crawls URLs that already have a final result.

**Why this priority**: Users routinely need to free bandwidth, move their laptop or stop a crawl that is hitting a fragile site. Resume is what makes multi-hour and million-URL crawls practical.

**Independent Test**: Start a crawl of the fixture site, pause at about 30%, quit the app, relaunch, resume, and confirm the finished crawl matches an uninterrupted crawl of the same site with the same configuration.

**Acceptance Scenarios**:

1. **Given** a running crawl, **When** the user presses Pause, **Then** no new requests start, in-flight requests either complete and are recorded or are returned to the queue, and the crawl shows as Paused within 5 seconds.
2. **Given** a paused crawl, **When** the user presses Resume, **Then** crawling continues from the remaining queue, using the crawl's stored configuration, and URLs already in a final state are not requested again.
3. **Given** a running crawl, **When** the user presses Stop, **Then** the crawl ends, is marked "Stopped (incomplete)", and every URL still queued is recorded as "not crawled: crawl stopped" so that every discovered URL has exactly one final state.
4. **Given** a stopped crawl, **When** the user chooses Resume, **Then** the "not crawled: crawl stopped" URLs return to the queue and crawling continues.
5. **Given** a running or paused crawl, **When** the user quits the app, **Then** the app tells the user the crawl will be paused and kept, pauses it, and the crawl can be resumed after relaunch.
6. **Given** a paused crawl of the fixture site that is resumed after a relaunch, **When** it finishes, **Then** the set of URLs and their final states matches an uninterrupted crawl of the same site with the same configuration.

---

### User Story 3 - Save a crawl as one file and open it later (Priority: P1)

The user saves a crawl as a single file at a location of their choice, for example to archive it or send it to a colleague. Later, on the same or another computer and operating system, they open the file and see the full results in the results table, exactly as they were when saved.

**Why this priority**: A single, portable crawl file is a constitutional product constraint and the basis for sharing, archiving and later comparing crawls.

**Independent Test**: Crawl the fixture site, save it, close it, copy the file to a different operating system, open it there, and compare row counts, tab counts and the detail pane of sample URLs with the original.

**Acceptance Scenarios**:

1. **Given** a crawl (running, paused or finished), **When** the user chooses Save As and picks a location and name, **Then** the crawl is stored there as exactly one self-contained file, and any results collected afterwards continue to go into that file.
2. **Given** a saved crawl file, **When** the user opens it via the Open command, by double-clicking it in the operating system's file manager, or by dropping it on the app window, **Then** the results table, all tabs, filters and the per-URL detail pane show the same data as when it was saved.
3. **Given** a crawl file created on one supported operating system, **When** it is opened on another supported operating system, **Then** it opens and shows identical data.
4. **Given** a saved crawl file of 1,000,000 URLs, **When** the user opens it, **Then** the first page of results is visible within 5 seconds and the app does not load the whole crawl into memory.
5. **Given** a crawl file that is already open in one SpiderLens window or process, **When** a second attempt is made to open the same file for crawling, **Then** the second attempt is refused with a message explaining the file is in use, or it opens read-only; the file is never written by two crawls at once.

---

### User Story 4 - Recent crawls list (Priority: P2)

On the start screen and in the File menu, the user sees a list of recent crawls (both saved files and crawls not yet saved to a chosen location), showing name or start URL, date, URL count and status (finished, paused, stopped, recovered). One click opens a crawl.

**Why this priority**: It is the main way users get back to their work, and the only easy way back to crawls they never explicitly saved. It is valuable but the feature still works through Open without it.

**Independent Test**: Run and save three crawls, leave one paused unsaved, restart the app and check that all four appear with correct details and open with one click.

**Acceptance Scenarios**:

1. **Given** the user has run or opened crawls, **When** the app starts, **Then** the recent crawls list shows them, most recently used first, with name or start URL, last-used date, URL count and status.
2. **Given** a recent crawl whose file has been moved or deleted, **When** the list is shown, **Then** that entry is marked as unavailable, and the user can locate the file or remove the entry.
3. **Given** a recent crawl entry, **When** the user removes it from the list, **Then** the entry disappears but the crawl file itself is not deleted; deleting an unsaved crawl's data happens only through an explicit, confirmed delete action.
4. **Given** a crawl that was recovered after a crash, **When** the list is shown, **Then** that crawl is marked as recovered and unfinished.

---

### User Story 5 - Disk full and write failures pause safely (Priority: P2)

If the disk fills up, the crawl file becomes unwritable (for example a removed USB drive, a lost network share or revoked permissions), the crawl pauses instead of crashing or silently dropping results. The user gets a clear message saying what happened, where the file is, and what to do, and can resume once the problem is fixed.

**Why this priority**: Large crawls are exactly the ones likely to hit disk limits. Silent data loss would violate the "correct and complete data" principle.

**Independent Test**: Crawl the fixture site onto a small, size-limited test volume until it fills; confirm the crawl pauses, the message appears, the file opens without damage, and after space is freed the crawl resumes and completes.

**Acceptance Scenarios**:

1. **Given** a running crawl, **When** free space on the crawl file's drive drops below the safety reserve, **Then** the crawl pauses before the drive is completely full, and a message states that the disk is nearly full, where the crawl file is, and how much space is free.
2. **Given** a running crawl, **When** a write to the crawl file fails for any reason, **Then** the crawl pauses, the URLs whose results could not be written are returned to the queue (not lost and not marked as crawled), and a message explains the cause in plain language.
3. **Given** a crawl paused because of a disk or write problem, **When** the user fixes the problem and presses Resume, **Then** the crawl continues and finishes with no missing or duplicated results.
4. **Given** a crawl paused because of a disk or write problem, **When** the user opens the crawl file afterwards (even without fixing the problem), **Then** it opens and shows all results written before the failure.
5. **Given** the drive cannot be made writable again, **When** the user chooses Save As to a different location, **Then** the crawl is moved there and can be resumed.

---

### User Story 6 - Crawl files explain themselves and keep opening after upgrades (Priority: P2)

Each crawl file records the full crawl configuration and the SpiderLens version that created it (and any later version that resumed it). Users can view this information from the open crawl. When SpiderLens is upgraded, crawl files made by older versions still open and their data is unchanged.

**Why this priority**: Results must be explainable and reproducible, and users keep crawl archives for months or years. Breaking old files would destroy trust.

**Independent Test**: Keep a set of reference crawl files produced by each released version; in CI, open every one with the current build and check counts and sample values against stored expectations.

**Acceptance Scenarios**:

1. **Given** an open crawl, **When** the user views the crawl information, **Then** it shows the start URL or URL list source, mode (Spider or List), the full configuration used, start and end times, status, URL counts, and the app version(s) that created and resumed it.
2. **Given** a crawl file created by any earlier released version, **When** it is opened by the current version, **Then** it opens and shows the same data it showed in the version that created it.
3. **Given** an older-format crawl file, **When** the user resumes it or otherwise changes it with a newer version, **Then** the app first tells the user the file will be upgraded and may no longer open in older versions, and keeps an untouched copy of the original unless the user declines.
4. **Given** a crawl file created by a newer version than the one running, **When** the user tries to open it, **Then** the app either opens it read-only (if it can read it safely) or refuses with a message naming the version needed; it never modifies or damages the file.
5. **Given** a crawl file that is damaged or is not a crawl file, **When** the user opens it, **Then** the app shows a clear error, does not crash, and, if part of the data is readable, offers to open what can be recovered.

---

### Edge Cases

- The app is closed while a Save As to a new location is in progress: the original crawl data remains complete and openable; no half-written file is left looking like a valid crawl.
- The user saves a crawl over an existing file of the same name: the app asks for confirmation; the old file is only replaced once the new one is complete.
- The crawl file lives on a network share or a cloud-synced folder: the app works if the location behaves like a normal disk; if the connection drops, the write-failure behaviour (User Story 5) applies. The app warns that such locations may be slower.
- The crawl file is renamed or moved in the file manager while the crawl is open: the app either keeps writing to the same file or pauses with a write-failure message; it never silently writes elsewhere.
- The crawl file is read-only (permissions or read-only media): it opens read-only for viewing; resume is offered only after Save As to a writable location.
- The computer sleeps or hibernates during a crawl: the crawl continues or resumes on wake without losing results; requests that timed out because of sleep are handled by the normal retry rules.
- A crash happens during pause, stop, or Save As: recovery on next launch still produces a consistent crawl.
- Two crashes in a row on the same crawl: recovery still works each time.
- The machine runs out of space on the drive holding unsaved crawls even though no crawl is running: the app warns at start-up and lists unsaved crawls by size so the user can delete or save elsewhere.
- Very large crawls (several million URLs or tens of gigabytes): the file keeps working; the app shows the file size in the crawl information.
- A crawl with zero results (stopped immediately or start URL unreachable) can still be saved, opened and resumed.
- The configuration contains secrets (for example passwords or authentication headers used for a site the user controls): these are not stored in the crawl file, so sharing a file never leaks them; on resume the user is asked to re-enter them.
- Opening a crawl while another crawl is running: the user is asked whether to pause the running crawl first; a running crawl is never silently interrupted.

## Requirements *(mandatory)*

### Functional Requirements

**Continuous storage**

- **FR-001**: The system MUST store every crawl, from its first result, in one crawl file on disk; results MUST NOT be kept only in memory until a save.
- **FR-002**: The system MUST write each URL's result (and newly discovered URLs added to the queue) to the crawl file within 5 seconds of it being produced while a crawl is running.
- **FR-003**: Memory used by the app during a crawl MUST NOT grow in proportion to the number of URLs crawled or stored; no part of the app may load an entire crawl into memory, including when opening, viewing, filtering or resuming.
- **FR-004**: Writes to the crawl file MUST be all-or-nothing per batch, so that after any interruption the file contains only complete results and a consistent queue: each URL is either in exactly one final state or still queued, never duplicated or half-recorded.
- **FR-005**: The crawl file MUST store, for each URL, all data collected by the crawl (including the extracted page data and the compressed raw HTML defined in other specs) so that the opened crawl shows the same results table, tabs, filters and detail pane as during the live crawl.
- **FR-006**: The crawl file MUST also store the crawl queue and enough crawl state (discovered URLs, depth, retry counts, per-host state needed for respectful crawling) to resume the crawl exactly where it left off.
- **FR-007**: New crawls that the user has not yet saved to a chosen location MUST be stored automatically in an app-managed location for unsaved crawls, so that they survive quitting and crashes.

**Pause, stop, resume, quit and recovery**

- **FR-008**: Users MUST be able to pause a running crawl; on pause, no new requests start, in-flight requests are completed and recorded or returned to the queue, and all collected results are written to the file. The crawl MUST show as Paused within 5 seconds.
- **FR-009**: Users MUST be able to resume a paused, stopped, recovered or write-failure-paused crawl; resuming MUST NOT re-request URLs that already have a final result (except through the separate "retry failed URLs" action).
- **FR-010**: Users MUST be able to stop a running or paused crawl; stopping MUST mark the crawl "Stopped (incomplete)" and record every still-queued URL with the final state "not crawled: crawl stopped", which is replaced if the crawl is later resumed.
- **FR-011**: When resuming, the system MUST use the crawl's stored configuration. Users MAY change speed and limit settings (concurrency, rate limits, maximum URLs) before resuming; settings that change which URLs are in scope MUST NOT be changeable on resume. Every change MUST be recorded in the crawl's configuration history.
- **FR-012**: When the user quits the app with a crawl running, the system MUST inform the user, pause the crawl, write all results and close the file cleanly before exiting.
- **FR-013**: On launch after a crash or forced exit, the system MUST detect each crawl that was not closed cleanly, recover it to a consistent state, and offer to open it; a recovered crawl MUST be marked as recovered and shown as paused.
- **FR-014**: After recovery, at most the results produced in the last 5 seconds before the crash may be missing; any URL whose result was lost MUST be back in the queue.

**Save and open**

- **FR-015**: Users MUST be able to save a crawl at any time (running, paused, stopped or finished) to a location and name of their choice as exactly one self-contained file; a running crawl MUST keep running and continue writing to the saved file.
- **FR-016**: Saving to a new location MUST NOT leave the crawl in a broken state if interrupted: until the new file is complete, the previous copy remains the valid one. Overwriting an existing file MUST require confirmation.
- **FR-017**: Crawl files MUST use one distinctive SpiderLens file extension and be associated with the app on all supported operating systems, so they open by double-click.
- **FR-018**: Users MUST be able to open a crawl file via an Open command, the recent crawls list, double-click in the file manager, and drag-and-drop onto the app window.
- **FR-019**: A crawl file MUST be portable: a file created on any supported operating system MUST open with identical data on any other supported operating system.
- **FR-020**: The system MUST prevent two crawls or processes from writing to the same crawl file at the same time; a second open MUST be refused with an explanation or opened read-only.
- **FR-021**: Read-only crawl files MUST open for viewing; resuming them MUST require Save As to a writable location first.
- **FR-022**: Opening a crawl MUST NOT require the whole file to be read before showing results; the first page of results MUST appear within 5 seconds for a 1,000,000-URL crawl on the reference hardware.
- **FR-023**: Users MUST be able to delete an unsaved crawl from the app, with confirmation; the app MUST show the disk space used by unsaved crawls.

**Recent crawls**

- **FR-024**: The system MUST maintain a recent crawls list (default: the 20 most recently used) showing for each entry the name or start URL, last-used date, URL count, status (running, paused, stopped, finished, recovered) and whether it is saved or unsaved.
- **FR-025**: The recent crawls list MUST be shown on the start screen and in the File menu, and opening an entry MUST take one action.
- **FR-026**: Entries whose file is missing MUST be marked unavailable; the user MUST be able to locate the moved file or remove the entry. Removing an entry MUST NOT delete the crawl file.

**Disk and write failures**

- **FR-027**: The system MUST monitor free space on the drive holding the active crawl file and pause the crawl when free space drops below a safety reserve (default: 1 GB, user-configurable), before the drive is completely full.
- **FR-028**: When any write to the crawl file fails, the system MUST pause the crawl, keep already-written data intact, return any URLs whose results could not be written to the queue, and never mark such URLs as crawled.
- **FR-029**: For a disk-full or write-failure pause, the system MUST show a clear message stating the cause in plain language (for example "disk is full", "drive was disconnected", "permission denied"), the crawl file's location, free space where relevant, and the available actions (free space and resume, Save As elsewhere, or keep paused).
- **FR-030**: Disk and write failures MUST be recorded in the crawl's event history as "local" errors, distinct from website errors, and MUST NOT count as failures of any URL.
- **FR-031**: The app MUST warn at start-up and before starting a crawl when free space on the drive used for unsaved crawls is below the safety reserve.

**Configuration, version and compatibility**

- **FR-032**: Each crawl file MUST record: the full crawl configuration (mode, start URL or URL list, scope, speed, limits, robots and other settings), start and end times, status, URL counts, the app version and operating system that created it, and each app version that later resumed or modified it.
- **FR-033**: Users MUST be able to view this crawl information from an open crawl, and to start a new crawl using the same configuration.
- **FR-034**: Crawl files MUST NOT store secrets such as passwords, authentication tokens or credential headers; on resume, the system MUST ask for any secrets the configuration needs.
- **FR-035**: Every released app version MUST open crawl files created by every earlier released version (from the first public release onwards) and show the same data.
- **FR-036**: Before upgrading an older-format file (for example on resume), the system MUST tell the user and keep an untouched copy of the original unless the user declines. Merely opening and viewing an older file MUST NOT modify it.
- **FR-037**: When a file was created by a newer app version, the system MUST either open it read-only (if it can do so safely) or refuse with a message naming the minimum version needed; it MUST NOT modify such a file.
- **FR-038**: When a file is damaged or is not a crawl file, the system MUST show a clear error without crashing, and MUST offer to open whatever data can be recovered when part of it is readable.

**Command line**

- **FR-039**: The command-line crawler MUST be able to write a crawl to a named crawl file, resume an unfinished crawl file, and print a crawl file's stored configuration, version and status; files written by the command line and by the desktop app MUST be interchangeable.
- **FR-040**: The command-line crawler MUST apply the same disk-full and write-failure rules, exiting with a distinct non-zero exit code and a clear message while leaving the crawl file resumable.

### Key Entities

- **Crawl**: One crawl session and all its data. Attributes: identifier, name, mode, start URL or URL list source, status (running, paused, stopped, finished, recovered), start and end times, URL counts by final state, whether saved to a user-chosen location.
- **Crawl File**: The single self-contained file that holds one Crawl: its results, queue, crawl state, configuration, version history and event history. Has a location, size, format version and lock state (in use or not).
- **Crawl Configuration**: The full settings used for the crawl, minus secrets. A crawl has an original configuration plus a history of changes made on resume.
- **URL Result**: The recorded outcome and extracted data for one URL, including its single final state. Belongs to one Crawl.
- **Crawl Queue**: URLs discovered but not yet in a final state, with the state needed to continue (depth, attempts, scheduling). Belongs to one Crawl.
- **Version Record**: The app version, operating system and date for each version that created, resumed or upgraded a crawl file.
- **Crawl Event**: A timestamped entry in a crawl's history: started, paused, resumed, stopped, recovered, saved, upgraded, local disk or write failure.
- **Recent Crawl Entry**: A pointer in the recent crawls list to a crawl file, with cached display details (name, date, URL count, status) and availability.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In repeated forced-kill tests during crawls of the 100,000-page fixture site, 100% of recovered crawls open and are consistent (no duplicates, no half-written results, every URL in exactly one final state or queued), and no more than the last 5 seconds of results are lost.
- **SC-002**: During a crawl of the 100,000-page fixture site at 500 URLs per second or more, the app's memory use at 100,000 crawled URLs is no more than 20% higher than at 10,000 crawled URLs; continuous storage does not reduce sustained crawl speed below 500 URLs per second.
- **SC-003**: A crawl paused, quit, relaunched and resumed ends with the same set of URLs and final states as an uninterrupted crawl of the same fixture site with the same configuration, in 100% of test runs.
- **SC-004**: A saved 1,000,000-URL crawl shows its first page of results within 5 seconds of opening on reference hardware, and the UI stays responsive (interactions respond within about 100 ms) while browsing it.
- **SC-005**: Pause takes effect (crawl shown as Paused, no new requests) within 5 seconds in 99% of attempts during a full-speed crawl.
- **SC-006**: In disk-full and write-failure tests (full volume, removed drive, revoked permissions), 100% of runs pause without a crash, show a message naming the cause and the file location, keep all previously written results, and complete without missing or duplicated results after the problem is fixed and the crawl is resumed.
- **SC-007**: Every crawl file in the reference set (at least one per released version) opens in the current build with identical URL counts and sample values, checked automatically in CI on all three operating systems.
- **SC-008**: A crawl file created on each supported operating system opens with identical data on the other two, checked automatically in CI.
- **SC-009**: A user can find and reopen a crawl from the last session in one action from the start screen, and in usability testing at least 90% of users can save, reopen and resume a crawl without help.

## Assumptions

- This spec belongs to milestone M1 (core crawl, first usable). It depends on the crawl engine and frontier (crawl configuration, modes Spider and List), fetch errors and retries (final states, error classes including "local"), extraction (what is stored per URL) and the results table (what is displayed) specs; it defines how all of that is stored, not what is collected.
- "Reference hardware" means a mainstream laptop or desktop from the last 4 years with an SSD, as used for the other M1 performance criteria.
- The app has one crawl open for crawling at a time in M1. Opening several crawls side by side and comparing crawls are out of scope (M3).
- Unsaved crawls are kept until the user deletes or saves them; there is no automatic deletion in M1. The app shows how much space they use.
- Backward compatibility is guaranteed from the first public release onwards; pre-release development builds may break file compatibility. Opening files from newer versions is best-effort read-only (forward compatibility is not guaranteed).
- A crawl file is not encrypted or password-protected in M1; users are responsible for where they share it. Secrets are excluded from the file to make sharing safe.
- The 5-second write window and 1 GB disk safety reserve are defaults chosen for a good balance of safety and speed; they can be tuned in planning if tests show a better value.
- "Retry failed URLs" at the end of a crawl is defined in the fetch errors and retries spec; this spec only ensures it works on reopened and resumed crawls.
- Out of scope: exporting results to CSV/spreadsheet formats (M2), crawl comparison and scheduled crawls (M3), cloud storage or sync of crawl files (not planned; the product is local-first), merging two crawl files, and signed or encrypted crawl files.
