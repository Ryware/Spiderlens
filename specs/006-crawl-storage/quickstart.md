# Quickstart: Crawl Storage

Check each user story by hand. Commands assume a release build and the bench
site (`cargo run --release -p spiderlens-test-fixtures --bin bench-site --
--pages 100000`).

## US1: crash recovery

```sh
spiderlens crawl http://127.0.0.1:8700/p/0 --speed fast --out crash.spiderlens &
sleep 5; kill -9 $!
spiderlens info crash.spiderlens          # status: running (not closed cleanly)
spiderlens crawl --resume crash.spiderlens   # "recovered: N URLs back in the queue"
```

In the app: start a crawl, `kill -9` the app, relaunch. The start screen
says the crawl was recovered; open it, it shows Paused with correct counts.

## US2: pause, quit, resume

Start a crawl in the app, quit with ⌘Q. The app says it is pausing the
crawl, then exits. Relaunch, open it from Recent crawls, Resume. It finishes
with the same URL count as an uninterrupted crawl.

## US3: Save As while running

During a crawl press ⌘⇧S, pick a folder. The crawl keeps going; `spiderlens
info` on the new file shows the counts rising. The unsaved copy is gone.
Open the file on another OS: same counts.

## US4: recent crawls

Run three crawls, save two. Restart: the start screen lists all three with
status and URL count. Rename one file in Finder: its entry shows
Unavailable with Locate and Remove.

## US5: disk full

```sh
spiderlens crawl http://127.0.0.1:8700/p/0 --speed fast --out x.spiderlens --disk-reserve 999999999
echo $?   # 6, "paused: less than 953.7 TB free on ..."
spiderlens crawl --resume x.spiderlens --disk-reserve 100
```

Linux, real volume: `mount -t tmpfs -o size=60m tmpfs /mnt/small` and crawl
into it; the crawl pauses with "disk is full", the file opens, resuming
after moving it elsewhere finishes the crawl.

## US6: versions

`spiderlens info crates/engine/tests/reference/v2-paused.spiderlens` shows
format 2. `spiderlens crawl --resume` on a copy upgrades it to 3 and leaves
`v2-paused (format 2 backup).spiderlens` next to it.
