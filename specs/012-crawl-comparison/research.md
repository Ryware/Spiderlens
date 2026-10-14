# Research: Crawl Comparison and Change Detection

## R1. Reading a crawl file while writing a scratch file on the same connection

**Question**: can one SQLite connection have the crawl file as `main`,
read-only, and write to an attached scratch file?

**Finding** (measured 2026-10-14 in the container, SQLite 3.45 via
Python, same engine as rusqlite's bundled build): opening
`file:crawl.sqlite?mode=ro` with URI support, then
`ATTACH 'ws.sqlite' AS cmp`, allows `CREATE TABLE cmp.rows …` and
`INSERT INTO cmp.rows SELECT … FROM urls u`. An `INSERT INTO urls` fails
with "attempt to write a readonly database". The crawl file's size and
modification time were unchanged afterwards.

**Decision**: D1. `mode=ro` is a property of the `main` database only; the
attached workspace stays writable.

**Caveat**: a read-only connection to a WAL-mode file may create the
`-shm` and `-wal` side files next to the crawl if they are missing (the
same as the existing results reader). The crawl file itself is not written.
SC-004 is tested on the crawl file's bytes and modification time.

## R2. Filter membership storage

Options: one row per (URL, filter) (≈100 rows per URL, 100 M rows for 1M
URLs, too large); a BLOB bitmap built by a Rust function (variadic SQL
functions are capped at 127 arguments by default); INTEGER bit columns
built by pure SQL shifts and ORs (`(cond) << k`).

**Decision**: INTEGER bit columns (D3), 62 bits each so the sign bit is
never used. Pure SQL, no argument cap, counts with `(fb >> k) & 1`.

## R3. Matching key and normalisation

Spec decision 4: no hidden normalisation. The key is the stored URL
(already normalised by the crawl per 002/003) or its mapped form. An index
on `(side, key)` makes the joins index lookups.

## R4. Not reached

The current crawl's `urls` table keeps queued URLs and limit skips with a
`reason` (spec 003). A previous URL whose counterpart is queued or skipped
by a limit is Not reached. A previous URL absent from an incomplete
current crawl is labelled "Not reached (crawl incomplete)" only when its
previous depth exceeds the current crawl's deepest fully finished depth;
otherwise it is Missing. This keeps "Missing" trustworthy for complete
crawls and honest for partial ones.

## R5. Element value expressions

Reused from `results::tabs` where a column exists: title, meta
description, H1, H2, word count, status, final state, indexability,
canonical, redirect target, depth, inlinks, outlinks, response time.
New: set of H1s (`page_text` kind h1, sorted, joined), directives (meta
and header combined, sorted), hreflang set (`links` kind hreflang:
`lang=target`, sorted), structured data types and fingerprint, content
fingerprint (`pages.text_hash`).

## R6. Threshold function

009 D3 registers `thr()` over shared settings so a threshold change needs no
SQL rebuild. The comparison does the same with `cthr(element)`, read during
the element count query, so FR-019 updates in one query.

## R7. Exit code numbering

Existing: 0, 2, 3, 4, 5; 006 adds 6; 009 expects 7. `COMPARE_FAILED` takes
the next free number at merge time (expected 8) and is recorded in
contracts/cli.md then.

## Measurements to record (filled by T029, T030)

| Case | Summary | Full | Peak memory |
|---|---|---|---|
| 100k vs 100k | | | |
| 1M vs 1M | | | |
