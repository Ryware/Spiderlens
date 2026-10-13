# CLI contract: crawl storage

## `spiderlens info FILE [--json]`

Prints the crawl information (FR-032, FR-039): name, path, size, format,
status and pause reason, recovered flag, mode and start URL, URL counts,
start/end times, configuration (secrets shown as `<secret>`), versions that
created/resumed/upgraded it, events, and whether it can be written now.
Never modifies the file. Exit 0, or 2 if the file is not a crawl file or
cannot be read.

## `spiderlens crawl --resume FILE`

New options allowed with `--resume`: `--speed`, `--max-concurrent`,
`--max-per-host`, `--rate`, `--max-urls`, `--header-secret NAME=VALUE`,
`--no-backup`, `--disk-reserve MB`, `--json`, `--quiet`. Any other crawl
option is refused as today. Behaviour:

- Not closed cleanly: recovers first, prints `recovered: N URLs returned to
  the queue`.
- Older format: prints `upgrading crawl file from format 2 to 3; original
  kept as <path>` (or `not kept` with `--no-backup`).
- Newer format or read-only: exit 2 with the reason.
- In use by another process: exit 2, `in use by SpiderLens <v> (pid <p>)
  on <host> since <time>`.

## `spiderlens crawl ... --disk-reserve MB`

Default 1024. The crawl pauses when free space on the crawl file's volume
drops below it.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | completed |
| 2 | usage or file problem |
| 5 | paused by Ctrl-C (resumable) |
| **6** | **paused by a disk or write problem (resumable); message names the cause, the file and free space** |
