# Media sends and helper programs stop waiting forever

Round 3 stability audit, items 3, 7 and 12.

## What tdesktop does

`Api::SendAction` hands a prepared file list to a task queue
(`Storage::PrepareTask`, `PreparedList`): the pictures are scaled and the
videos probed on worker threads, the history shows the message as sending, and
the UI never waits on the work. The send goes to the history it was started
in. tdesktop reads media with its own decoders, so it has no child process to
time out; Quill shells out to `ffprobe`/`ffmpeg` and the desktop's settings
tools, so the equivalent is a bounded child.

## What changed

### One helper for child processes (item 12)

`src/child_timeout.rs`: `output_within`, `status_within`, `stdout_within` and
`wait_within`. A child still running at the limit is killed and reaped and the
caller gets `io::ErrorKind::TimedOut`; stdout/stderr are drained on reader
threads so a chatty child never stalls, and `no_console()` is applied on
Windows like `media_tools::command` does. `transcode_limit(seconds)` scales the
limit to the media (30 s plus 4 s per second of media, at most ten minutes).

Every ffmpeg/ffprobe child that is waited on now goes through it: the photo
size probe and scale (`composer/media_options.rs`), the local video probe and
round video thumbnail (`video/mod.rs`), frame extraction for playback, the
video editor's probe, frames and H.264 encode (`video_edit.rs`, limit from the
edit length), the sticker WebM encode (`sticker_video.rs`) and the video avatar
encode. Capture children that run until the user stops them and the device
listings (already `stdout_within`/`stderr_within`) are unchanged.

### Sends prepare their files off the UI thread (item 3)

- `composer/media_prep.rs`: `PreparedMedia::prepare(&snapshot)` reads what the
  driver used to read inside the send (scaled photo copy, video probe, round
  video probe and thumbnail, GIF size). `ComposerSnapshot.prepared` carries the
  result; the driver (`connect/messages/media_options.rs`) uses it and only
  reads the file itself when a snapshot has none (tests, other callers). One
  prepare is bounded by `PREP_BUDGET` (three minutes) on top of each child's
  limit.
- `ui/send_prep.rs`: Send on a photo, video or round video no longer calls the
  driver on the UI thread. The snapshots go to `background_spawn`; the
  composer text, tray and reply stay as they are, and the tray shows
  "Preparing the files…" (derived from `SendPrepState`, not from a stored
  flag). The send goes out when the files are ready, through the account and to
  the chat Send was tapped in (`SendOrigin`), not to the chat open by then.
- Order: a slot (account, chat, topic) has at most one send in flight. A second
  Send in it while the files are being read is held back with the same
  "Preparing the files…" note, so messages keep their order. Other chats are
  not held.
- Failure or timeout: nothing was taken from the composer, so the draft, tray
  and reply are still there; a plain toast says what failed
  (`PrepareError` implements `UserError`). The old re-probe on the UI thread
  after a failed send (`ui/composer/mod.rs`) is gone with it.
- After a successful send only what Send took is cleared: text typed since is
  kept, files added since stay in the tray (`plan_cleanup`). For a chat that is
  no longer open the stored draft and tray are cleared the same way.
- Logging out or removing the account cancels its jobs unsent.

### Linux helper calls (item 7)

`gdbus` (Do Not Disturb, battery saver, system accent), `gsettings`, `fc-list`
and `xdg-mime` use the helper with 5 s (30 s for the first `fc-list`, which can
rebuild the font cache). The "refresh running" flags of Do Not Disturb and the
accent are now reset by a drop guard, so neither a timeout nor a panic leaves
them set. The reduce-motion probe at startup (`defaults`, `gsettings`) moved
from `main.rs` to `ui/reduce_motion_probe.rs` and runs on the background
executor after the first window is up. The link handler setting
(`link_handler::state`, called while the settings page draws) returns the last
answer at once on Linux and refreshes on a thread; the page redraws when the
read lands.

## Not covered

- A send to a forum topic that is no longer open goes to the chat's general
  thread: the driver picks the topic from the open one (`send_topic`).
  Unchanged by this work, and only reachable by leaving the topic during the
  prepare.
- Stickers, GIFs and voice messages sent from another surface while a prepare
  is running are not ordered with it.
- Starting the default-handler change (`xdg-mime default`) still runs on the
  click that asks for it, now with a timeout.

## How verified

- Unit tests: a hung child is killed and reaped at the limit (`output_within`,
  `status_within`), large output does not stall, `transcode_limit`; prepare
  failures; `SendPrepState` (one job per slot, other chats free, job keeps its
  chat, account cancel); a slow prepare does not hold the caller and keeps
  order; the cleanup plan keeps text and files added meanwhile.
- Driver tests with the recording sender: prepared videos, GIFs, round videos
  and albums are sent to chat 7 while chat 8 is open without reading the (junk)
  files; the unprepared junk video fails as before.
- Screenshot demo `ready-send-preparing` in light and dark: the tray with
  "Preparing the files…". Not exercised: a real TDLib send, a stalled network
  share, a hung `gdbus` on a real Linux desktop.
