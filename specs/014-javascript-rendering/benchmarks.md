# Benchmarks: JavaScript Rendering

## Baseline (T004, SC-001)

**Date**: 2026-10-13. **Machine**: 4-core cloud container, 16 GB, no swap,
bench site on the same machine (not the reference laptop). **Code**: `main`
at 5e6f2ff (this branch at 48d802f differs from it only in `specs/`).
Release builds (`lto = "thin"`, `codegen-units = 1`, stripped).

Site: `bench-site --pages 100000` (spec 008). Crawl: `spiderlens crawl
http://127.0.0.1:8700/p/0 --speed fast --out FILE --quiet --json`. Each run
starts with an empty crawl file and the site's counters reset; runs are
separated by 30 s of idle time. Time is wall time of the `spiderlens`
process, URLs per second is the site's own request count (100,000 in every
run) divided by that time, and peak memory is the process's maximum
resident set size (`wait4` rusage).

| Run | Time | URLs/s | Peak RSS | Crawl file |
|---|---|---|---|---|
| 1 | 35.12 s | 2,847 | 32.0 MB | 336.0 MB |
| 2 | 36.92 s | 2,709 | 31.8 MB | 335.8 MB |
| 3 | 32.69 s | 3,059 | 31.7 MB | 335.8 MB |
| 4 | 35.74 s | 2,798 | 32.2 MB | 335.9 MB |
| 5 | 35.67 s | 2,804 | 32.9 MB | 335.7 MB |
| 6 | 36.57 s | 2,735 | 32.1 MB | 335.6 MB |
| 7 | 34.72 s | 2,880 | 31.9 MB | 335.4 MB |
| **Median** | **35.67 s** | **2,804** | **32.0 MB** | 335.8 MB |
| Spread (min to max) | 32.69 to 36.92 s (11.9% of the median) | 2,709 to 3,059 (12.5%) | 31.7 to 32.9 MB (3.8%) | |

The spread between single runs (about 12% here, 7% in spec 005) is far
larger than the 2% throughput budget of SC-001, so T080 compares medians of
7 alternating runs, as the plan says. Even medians of 7 runs on this
container probably move by a few percent between sessions; T080 should run
the baseline binary and the new one alternately in the same session rather
than compare against these numbers alone.

Release binary sizes on Linux x86-64 (stripped):

| Binary | Size |
|---|---|
| `spiderlens` (CLI) | 13,358,904 bytes (12.7 MiB) |
| `spiderlens-desktop` (built with `--features tauri/custom-protocol`, as `tauri build` does) | 19,007,352 bytes (18.1 MiB) |

To reproduce: build both binaries and `bench-site` from the merge base with
`cargo build --release`, then run the crawl command above 7 times with an
idle pause between runs.
