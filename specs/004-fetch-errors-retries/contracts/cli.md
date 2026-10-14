# Contract: command line

## `spiderlens crawl` (new retry options)

Next to the existing `--attempts`, `--max-wait`, `--no-retry-5xx` and
`--no-retry-429`:

| Option | Setting |
|---|---|
| `--retry-base-wait SECONDS` | `retry.base_wait_secs` |
| `--host-pause-after N` | `retry.host_pause_after` |
| `--host-pause SECONDS` | `retry.host_pause_secs` |
| `--host-pause-max SECONDS` | `retry.host_pause_max_secs` |
| `--host-give-up-after N` | `retry.host_give_up_after` |

While a crawl runs, host pauses and resumes are printed on stderr as they
happen, and the progress line shows URLs waiting to retry. A crawl that
pauses itself because the network is down exits with code 5 (paused) and
says why; `spiderlens crawl --resume FILE` continues it. A full disk exits
with code 4 and the message; the file can be resumed after freeing space.

## `spiderlens retry <FILE> [--yes] [--json]`

Retries the transient failures of a paused or finished crawl (US5, FR-039).

1. Prints how many URLs will be retried, by error class, for example:

   ```
   40 URLs failed for temporary reasons and will be retried:
     Timed out              31
     Server error (5xx)      9
   Retry them? [y/N]
   ```

2. Without `--yes`, waits for `y`. With nothing to retry, prints
   "Nothing to retry: no URL failed for a temporary reason." and exits 0.
3. Re-queues them and runs the crawl with the stored configuration, exactly
   like `crawl --resume`, then prints the summary.

`--json` prints the counts as `{"total": n, "by_class": {"timeout": 31}}`
before running, and the summary as JSON at the end. Exit codes as for
`crawl`. Refuses (exit 2) a file whose crawl is running in another process.

## `spiderlens report FILE --summary` (extended)

The summary gains errors by class (HTTP classes included), transient and
permanent totals, automatic retries made, URLs recovered by retry, hosts
paused and manual retry runs (FR-036). JSON keys as in data-model.md.

## `spiderlens report FILE --log [--json]`

Prints the crawl log (retries, failures, host pauses, local problems,
manual retries) in time order (FR-035).
