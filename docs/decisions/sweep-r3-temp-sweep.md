# Temp working files, notification threads and review follow-ups

Round 3 stability items 4 and 8, plus four follow-ups from the review of #997.

## Temp working files (item 4)

Hardening, not a tdesktop-visible feature. Quill's editors wrote into fixed names under the
shared temp dir (`quill-edits`, `quill-send-photos`, `quill-video-edits`,
`quill-sticker-video`, `quill-editor-sticker`, `quill-profile-photo`) and never
removed anything. On Linux, `/tmp/quill-edits` could also be pre-created by
another user.

- `scratch::work_root()` is a per-user private folder, `<temp>/quill-work-<uid>`
  (no uid tag on Windows, where the temp dir is already per user), created
  with `local_path::secure_create_dir`: 0700 on unix, a planted symlink is
  replaced rather than followed. If the name cannot be taken, the run uses
  `quill-work-<uid>-<pid>`.
- `scratch::temp_work_dir(name)` returns `<work root>/<name>`; `editor_art.rs`
  now uses it too. Test builds keep their per-process folders.
- `scratch::sweep_stale` removes entries of those folders whose newest change
  (looking up to four levels inside a folder entry) is older than 24 h. Only
  age decides, so nothing a send or edit made in the last day is touched.
  A symlinked root or folder is skipped; symlinks inside are unlinked, not
  followed. It runs on a `spawn_guarded` thread from `sweep_media_caches` (the
  startup sweep) and also covers the old names in the shared temp dir.
- `is_editor_temp` (sticker set box) accepts files in either location.

Limit: a message that waits offline for more than a day to send an edited copy
loses that copy at the next start.

`ui/profile_photo_sources` now uses `temp_work_dir` too (it waited on #998, now merged).

## notify-send threads (item 8)

`thread_guard::InflightSlot` takes a place in the counted pool (max
`MAX_OS_NOTIFICATION_THREADS`) and returns it on drop, including after a panic
and when the thread never starts. The two spawners use it instead of the
manual `fetch_sub`s. The cap and the five-minute wait limit already existed.

## Follow-ups from the #997 review

- The mini-app reader logs a line over the cap and the cap is 4x the IPC limit.
- `fast_buttons`, `notify_prefs::load_from_disk` and `web_app::trust` read
  through `settings::read_with_pending`, so a first read sees a queued write.
- If the settings writer thread cannot start, the bytes stay queued (reads see
  them, the next submit retries) and `flush` at quit writes them; the UI
  thread no longer fsyncs.
- `child_reap` waits after the kill when its thread cannot start.

## Verified

Unit tests: sweep by age, in-use folder kept, wholly stale folder removed,
symlinked root and folder not followed, private root 0700 and symlink replaced,
slot cap and release after a panic, queued write flushed. Not run on Windows or
real Linux with a second user.
