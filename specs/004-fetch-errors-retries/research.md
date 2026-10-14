# Research: Fetching Pages, Errors and Retries

## R1. Where reqwest tells us which timeout fired

reqwest reports `is_timeout()` for both, and `is_connect()` only when the
failure happened while connecting. A timeout that is also `is_connect()` is
a connect timeout; any other timeout (after the connection, while waiting
for headers or reading the body) is a read timeout. Body read errors come
from `Response::chunk()`; a timeout there is always "read".

## R2. Telling "refused" from "reset" and TLS reasons

The cause chain carries the OS error (`io::ErrorKind::ConnectionRefused`,
`ConnectionReset`, `ConnectionAborted`) and rustls errors
(`InvalidCertificate(Expired | NotValidForName | UnknownIssuer | ...)`).
Matching on the lowercased text of the chain, as spec 001 does, is stable
across platforms for these; `io::ErrorKind` is checked first where present.

## R3. Truncated body vs. over size

`read_body_partial` (spec 005) already distinguishes the size limit (we
stop reading) from a dropped connection (`chunk()` returns an error).
A dropped connection with `Content-Length` larger than what arrived, or a
chunked body that ends with an error, is `truncated`; reaching the limit is
`over_size`. Decode errors from gzip/brotli are `decompression`.

## R4. Retry-After forms

Already parsed by spec 008 (`parse_retry_after`): delay-seconds and
IMF-fixdate; a past date means no wait; an unparsable value is ignored.
Spec 004 adds only the cap at the maximum wait and the log line when the
cap applies.

## R5. Backoff and jitter

AWS's "Exponential Backoff and Jitter" analysis recommends jitter to avoid
synchronized retry waves. The spec fixes the shape: exponential, ±20%.
With base 1 s: attempt 2 waits 0.8-1.2 s, attempt 3 waits 1.6-2.4 s,
attempt 4 waits 3.2-4.8 s, capped at 60 s.

## R6. Circuit breaker parameters

resilience4j defaults: failure-rate window, half-open with a few probe
calls, exponential wait in open state optional. The spec chooses the
simpler consecutive-failures rule (5 in a row) and a single probe, which
matches crawlers' needs: one URL's failure is noise, five in a row on one
host is an outage.

## R7. Detecting a dead local network

Browsers and OS connectivity checks probe a known endpoint; we cannot
(principle II, no SpiderLens server, and probing a third party would be a
surprise request). The signal we have is "everything fails, across hosts".
10 consecutive network-class failures across at least 2 hosts with no
success in between: on a working network, two unrelated hosts failing
10 times in a row with nothing succeeding in between is very unlikely,
and the cost of a false positive is a pause the user resumes with one
click.

## R8. Full disk in SQLite

rusqlite surfaces `SQLITE_FULL` as `ErrorCode::DiskFull` and a read-only
file as `ErrorCode::ReadOnly` / `CannotOpen`. In WAL mode a failed commit
leaves the database unchanged; in-flight rows are still `in_flight` and the
existing resume path re-queues them.

## R9. Back of the queue

The queue order is `(depth, queue_seq)`, breadth-first. "Back of the
queue" is implemented as a new `queue_seq` at the URL's own depth: it goes
after every URL of its depth already queued, and still before deeper URLs,
which keeps breadth-first order (spec 003 FR-020) and keeps the retry from
being starved behind an ever-growing deeper level.
