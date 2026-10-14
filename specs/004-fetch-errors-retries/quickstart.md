# Quickstart: errors and retries

Everything below runs against local fixtures; no live site is needed.

## 1. Error classes (US1)

```sh
cargo test -p spiderlens-engine --test errors
```

The `errors` fixture serves 200, 404, 410, 500, a non-existent host, a
connection that is refused, a page that never answers, a truncated body, an
over-size body, a gzip body that does not decompress and a malformed link.
The test checks each URL's final class, sub-reason, message and attempts,
and that the totals of all final states equal the discovered URLs (US1.8).

## 2. Retries (US2)

```sh
cargo test -p spiderlens-engine --test retries
```

A URL that answers 503 twice then 200 ends 200 with 3 attempts and waits
of about 1 s and 2 s (scaled down in the test); a URL answering 500 forever
is tried exactly 3 times; a 429 with `Retry-After` is respected for the
whole host; permanent classes are requested once.

## 3. Host pausing (US4)

```sh
cargo test -p spiderlens-engine --test host_pause
```

Two hosts; one starts refusing connections. It is paused after 5 failures,
probed, paused again with a doubled wait, and the other host's rate stays
within 10% (SC-005).

## 4. Manual retry (US5)

```sh
cargo run -p spiderlens-cli -- crawl http://flaky.test/ --out /tmp/f.spiderlens
cargo run -p spiderlens-cli -- retry /tmp/f.spiderlens
```

(With the fixture's `--resolve`, see the test `cli_retry` for the exact
invocation.) Only the transient failures are listed and retried.

## 5. In the app

Start a crawl of a site with a flaky host: the status bar shows "Waiting
for host" with a countdown and the paused host with "Resume now". After the
crawl, Toolbar › Retry failed URLs shows the counts by class before it
starts. The Response codes tab has "Transient errors" and "Permanent errors"
filters, and the Attempts column is filled.

## 6. Speed

```sh
cargo run --release -p spiderlens-test-fixtures --bin bench-site
```

Numbers before and after are recorded in [benchmarks.md](./benchmarks.md).
