# Download status is derived, not hand-synced

## What Telegram Desktop does

`DownloadManager::check` (data/data_download_manager.cpp) decides what a
queued download is by asking the document: `document->filepath(true)` set
means loaded (and it is added to the recent list), `!document->loading()`
means the entry is removed, otherwise progress comes from
`document->loadOffset()`. The manager keeps the queue (what the user asked
for) and reads everything else from the document, so a finished transfer or
a cleared cache changes the row without a second copy to update.

## What was wrong

Download state lived in six collections on `MediaState`: `downloading`,
`user_downloads`, `paused_downloads`, `failed_downloads`,
`stalled_auto_downloads` and `completed_downloads`. Eleven writers kept
them in step by hand. Desyncs found by reading the writers:

- A cleared cache did not touch `completed_downloads`, so the Recent list
  kept offering a file that was no longer on disk.
- A failed row stayed in `failed_downloads` when TDLib resumed the same
  transfer, so the manager listed it as Failed and downloading.
- A cancel dropped the id from `downloading`, but the UI also trusts
  `local.is_downloading_active`, which stays true until TDLib's idle update.
  The row kept its progress after Cancel. The cancelled request's error
  answer then marked the file stalled (automatic) because the user set was
  already cleared.
- Closing the session cleared `downloading` and `stalled_auto_downloads`
  only. User, paused, failed and recent entries survived.
- `apply_download_added` for a download started elsewhere added it to
  `user_downloads` but not `downloading`, so `should_download` could start a
  second transfer of the same file.
- A finished download was only removed from `downloading` by the update
  path; any other route that stored a completed file left the row on its
  spinner.

## What changed

`MediaState.downloads: DownloadIntents` (src/state/domains/media/downloads.rs)
replaces the six collections. It stores one record per file id: who asked
(`Auto` or `User`), whether the list says paused, and a phase (`Requested`,
`Ended`, `Cancelled`), plus the log of finished user downloads. It stores
nothing TDLib already knows.

`DownloadIntents::status(id, &files)` computes what a row shows from that
record and `ParsedFile::local`:

- completed locally: Done, whatever was asked;
- requested: Downloading, or Paused when the list says so;
- cancelled: Idle until TDLib confirms with an idle update;
- ended user request: Failed, unless TDLib reports the file active again;
- no record but TDLib reports it active: Downloading;
- otherwise Idle.

`recent()` filters the finished log by the file's current state, so a cache
clear removes the entry without a writer. The `failed` parameter threaded
through the chat history and sponsored rows is gone; rows ask
`downloads.is_failed`. `file_is_downloading` is `downloads.is_downloading`.

## How it was verified

- `src/state/tests/downloads_derived.rs`: file updates alone drive the
  status; a stale request never shows over a finished file; cancel hides
  progress at once, a late progress update and the cancelled request's error
  do not bring it back or mark it failed; a failed row follows TDLib when
  the transfer resumes; the Recent list drops a file removed from the cache;
  closing the session forgets every record.
- The existing download tests (state, driver, redraw, updates sync) pass
  against the new accessors.
- `ready-downloads` demo captured in light and dark and read by eye (active,
  paused, failed and recent rows; in-chat progress chips).

## Not covered

`open_chat_media_downloads` (a redraw hint pruned to the requested ids) and
`download_extras` (request id to file id) are separate indexes, not download
status, and stay.

## Review round

- Per-frame cost: the downloads manager, tab strip and media signature read
  `user_count`, `requested_count` and `failed_ids` every frame, and the old
  versions scanned every thumbnail and avatar record. `DownloadIntents` now
  keeps a requested counter, a user-id set and a failed-id set, all updated
  by `put`, the single writer of the entries map, so the counts are O(1) and
  the lists O(user entries). They are not a copy kept elsewhere.
- `should_download` also refuses a file TDLib reports as actively downloading
  with no record here (another device, a resume), so no duplicate transfer.
- Tests: counts stay right across begin, end, cancel, settle, forget, adopt
  and clear; an auto-download failure never yields Failed or a Retry row; an
  active transfer with no record is not requested again, and is once it
  stalls.
