# Research: Crawl Storage

## R1. Writer exclusion

`std::fs::File::try_lock` (stable in Rust 1.89, our toolchain is 1.97) maps
to `flock(LOCK_EX|LOCK_NB)` on Unix and `LockFileEx` on Windows. Locking the
database itself on Windows would take a whole-file lock that collides with
SQLite's byte-range locks, so the lock goes on a sidecar. On Linux and macOS
`flock` and SQLite's `fcntl` locks are independent anyway. Network shares:
SMB honours both; NFS `flock` emulation varies, which matches the spec's
"works if the location behaves like a normal disk".

Rejected: `fs2`/`fs4` lock APIs (same syscalls, extra dependency),
`sysinfo` pid checks (heavy, unreliable across machines), SQLite
`locking_mode=EXCLUSIVE` (blocks live readers).

## R2. Durability with `synchronous=NORMAL` in WAL mode

SQLite docs: in WAL mode with NORMAL, a commit is durable against an
application crash immediately (the WAL write is in the OS cache), and
against power loss once the WAL is synced, which happens at each
checkpoint. Our checkpointer runs every 250 ms. `FULL` would add an fsync to
every 200 ms commit. Measured cost of FULL on the bench (spec 008 notes):
not measured there; not needed for FR-014's 5 s bound, so not adopted.

## R3. Copying a live database

- `VACUUM INTO 'file'`: compact copy, but rewrites every page (slow for
  multi-GB files) and runs on the writer.
- Online backup API (`rusqlite::backup`): page copy, consistent, works with
  readers present. Used as the fallback.
- Checkpoint `TRUNCATE` then `std::fs::copy`: the database file is complete
  once the WAL is empty. `std::fs::copy` uses `fclonefileat`/`copyfile` on
  macOS (APFS clone, O(1) on the same volume) and `copy_file_range` on Linux
  (reflinks on btrfs/XFS). Chosen as the primary path.

Rename atomicity: `rename(2)` within a directory is atomic on POSIX;
`std::fs::rename` on Windows uses `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`,
which is atomic on NTFS for files in one directory.

## R4. Free space

`fs4::available_space(path)` (statvfs `f_bavail * f_frsize` /
`GetDiskFreeSpaceExW` caller-available bytes). This is the space the user
can actually write, not the raw free blocks.

## R5. Simulating a full disk

`PRAGMA max_page_count = N` makes SQLite fail inserts with `SQLITE_FULL`
("database or disk is full"), the same code a real full disk yields. Used in
tests; the real-volume test (SC-006, tmpfs with `size=`) runs on Linux CI
only, where mounting is allowed in the runner.

## R6. File association in Tauri 2

`bundle.fileAssociations: [{ ext: ["spiderlens"], name: "SpiderLens crawl",
role: "Editor", mimeType: "application/x-spiderlens" }]`. macOS sends
`RunEvent::Opened { urls }` (also at launch); Windows and Linux start the app
with the path as an argument. A second launch on Windows/Linux starts a
second process: it gets the file read-only if the first one is crawling it
(D2), which satisfies FR-020 without a single-instance plugin.

## R7. Drag and drop

Tauri 2 webviews emit `tauri://drag-drop` with `paths`; the UI subscribes
with `getCurrentWebview().onDragDropEvent`. Drop handling is in the UI, the
open itself is the same `crawl_open` command.
