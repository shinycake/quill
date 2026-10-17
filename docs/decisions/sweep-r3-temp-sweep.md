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
  followed. `sweep_work_dirs_once` runs it on a `spawn_guarded` thread, guarded
  by a `OnceLock`, called from `ui_main` at real app startup only (after the
  screenshot-demo early return). `restart_live_connection`, demo runs and
  `sweep_media_caches` do not start it, so a mid-session sweep cannot remove
  a file a tray or editor still holds.
- Only the private work root is swept. The old shared-temp names are left
  alone: a pass over a world-writable `/tmp` has a check-then-read race and no
  owner check, and with `TMPDIR=$HOME` it would have reached `~/quill-*`.
  Leftovers from older versions stay in the temp dir for the OS to clear.
- `is_editor_temp` (sticker set box) accepts files in either location.

Limits: a message that waits offline for more than a day to send an edited copy
loses that copy at the next start. Because the sweep runs once at startup, a
file kept in a tray across a session longer than 24 h is only at risk at the
next start, never during the run; if the app stays open for days and the file
is older than 24 h when it restarts, it goes then.

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
- `child_reap`, when its thread cannot start, kills the child and polls
  `try_wait` for about 50 ms, then gives up; it never blocks the caller.
- The settings `flush` keeps its 5 s bound, including when it writes queued
  bytes itself.
- The mini-app reader logs an over-long line once per helper, not per line.

## Verified

Unit tests: sweep by age, in-use folder kept, wholly stale folder removed,
symlinked root and folder not followed, private root 0700 and symlink replaced,
the once-only guard, slot cap and release after a panic, queued write flushed. Not run on Windows or
real Linux with a second user.
