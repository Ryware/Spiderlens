# Where crawls live and how cleanup works

## Where crawls live

- A saved crawl is a file in the folder you chose when you saved it.
- A crawl you have not saved is kept in the app's Unsaved folder. Settings,
  Storage, "Kept in" shows where that is, with a Show in folder button.

## Manage crawls

Open it from the command palette ("Manage crawls…") or from the warning shown
when the drive is nearly full. It lists every crawl the app knows about with
its size, dates and URL count. For each crawl you can:

- Open it or show it in its folder.
- Move it to the Trash.
- Remove the stored page HTML. The results stay identical. Only the saved
  copy of each page's HTML is dropped.

## Automatic cleanup

- Unsaved crawls not opened for 30 days are moved to the Trash. You can choose
  7, 14, 30 or 90 days, or turn this off, in Settings, Storage. The first time
  a crawl would be removed, the app only shows a notice and waits until the
  next start, so you can save it or change the setting.
- Leftover helper files from crashes, older than 24 hours, are removed.
- Scheduled runs beyond each schedule's "Keep the last" limit (10 by default)
  are moved to the Trash, oldest first.

Each removal is listed in the cleanup history in Settings, Storage.

## What cleanup never does

- It never deletes a saved crawl because of its age.
- It never touches files outside the app folder, or files you moved or renamed.
  Scheduled runs are only judged where the schedule saved them.
- It never removes the crawl that is open, or one that is running or paused.
- It never deletes permanently. If the Trash is not available, nothing is
  removed.
