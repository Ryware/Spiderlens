# Quickstart: Crawl Data Management

## CLI

```sh
cargo run -p spiderlens -- crawl https://example.com --out a.spiderlens   # stores page HTML by default
cargo run -p spiderlens -- files .
cargo run -p spiderlens -- export a.spiderlens --tab internal -o before.csv
cargo run -p spiderlens -- purge-html a.spiderlens --yes
cargo run -p spiderlens -- files .           # size drops, HTML 0 B
cargo run -p spiderlens -- export a.spiderlens --tab internal -o after.csv
cmp before.csv after.csv                          # identical
```

## App

1. Run two quick crawls without saving, save a third with Store page HTML on.
2. File › Manage crawls: three rows, sizes and totals; HTML size appears for the saved one.
3. Remove stored HTML on the saved crawl: confirmation shows the space; afterwards the size drops and the results are unchanged.
4. Select the two unsaved crawls, Delete: they are in the OS trash with no `.lock`, `-wal` or `-shm` left behind.
5. Settings › Storage: set cleanup to 7 days and the reserve to 5 GB.
6. Set an unsaved crawl's last-used date back 10 days (test hook `SPIDERLENS_CLEANUP_NOW=+10d`), restart: the one-time notice names it; restart again: it is in the trash and the notice says how much was freed.
