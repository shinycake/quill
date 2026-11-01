CHANGES

# Review of PR 1308 (export-quit) against main

Reviewed head 2d1be8a against main 948ad90. The PR does what it says for the
common path. A Dock Quit or log out during an export shows the box. Stop ends
the app. A second quit replaces the exit, and nothing loops. The PR's own
tests pass here:

- `cargo test --features demo-capture --locked --lib -- export:: closing chat_state::export rich_html`: 202 passed.
- `cargo test --features demo-capture --locked --bin quill -- export_quit link_inbox background_accounts::closing`: 26 passed.

Three defects block it. Two of them are limits the PR text states. Fix each at
its root, one commit per finding.

## 1. A worker mid-copy past the 400 ms hold is left behind (data left in the cache, truncated export file)

Where: `src/ui/background_accounts/closing.rs:163`, `src/connect/closing.rs:62`,
`src/export/worker.rs:300` and `:355`.

`close_all_for_quit` cancels and waits at most `EXPORT_STOP_HOLD`. The cancel
flag is read only inside `ChannelTdlib::call`, and `place_into` is a single
`fs::copy` that never looks at it. A worker copying a large file when Stop is
pressed copies until the end. Only then does `release` post its `deleteFile`.
By that time `begin_close` has already flushed and sent `close`, or the process
has exited. Two results:

- The file the export downloaded for itself stays in TDLib's cache for good.
  This is the L20 bug the PR set out to fix, still open for large files.
- If the process exits during the copy, the export folder keeps a truncated
  file under its final name. It looks like a valid photo or video.

This PR makes it more likely. N18 replaces the hard link with `fs::copy`
everywhere, so on NTFS and ext4 every file now takes copy time, where before
it took none.

Fix:

1. Make the copy cancellable. Add `fn is_cancelled(&self) -> bool` to the
   `Tdlib` trait (`ChannelTdlib` reads its `cancelled` flag). Copy into
   `<target>.part` in 1 MiB chunks and check the flag between chunks. On
   cancel, remove the `.part` and return `ExportError::Cancelled`. On success,
   rename it to the target. `release` already posts the `deleteFile` on
   `Cancelled`. A stop then always lands within one chunk, well inside 400 ms.
   Keep `fs::copy` for the case where the volume clones (APFS, btrfs), since a
   clone finishes at once. This keeps the N18 owner default.
2. Make the cleanup outlive the process. When `fetch_file` finds
   `only_for_export`, record the file id in a small per-account journal next to
   the TDLib database. Remove the id when the `deleteFile` is answered or the
   view keeps the file. When the client reaches `authorizationStateReady`, send
   `deleteFile` for each id still listed, after the same `getFile`
   untouched check. Any leftover, from a slow copy, a crash or a forced
   shutdown, then goes on the next start.

Test: a `Fake` whose copy source is large and a cancel set mid-copy. Assert
`Cancelled` comes back before the copy ends, no file sits at the target, and
`deleteFile` was posted. A journal test: an id recorded and not released is
deleted on the next ready driver.

## 2. A row that loads after the delete fetches the file again

Where: `src/export/handle.rs:123-125`, `:134`; `src/state/session_files.rs:167`.

`held_file` covers only the window between the export's `downloadFile` and its
`deleteFile`. The common case falls outside it. The user exports the chat
they have open and scrolls up while the export runs. Each row that loads after
the export released its file finds the cache empty and downloads the file
again. The export just had that file. Both the network and the time are spent
twice.

Fix: keep the release for files a view can still load. Hold back each
`deleteFile` for a file of a chat that is open in any window, instead of
sending it. The driver knows the open chats. Answer the worker `ok` at once,
as `keep_view_file` does now, so the export is not slowed. When the chat is
closed, or the export ends or stops, send the held deletes, minus any file in
`view_files`. Files of other chats are released at once as now, so the cache
grows by at most the open chat's media. The journal from finding 1 covers
held deletes the quit cuts off. After the export ends, a row has only one
place to get the file, since the cache copy is gone by then: its exported copy. That
is optional. Telegram Desktop downloads into the export folder outside its
cache, so it too fetches once there.

Test: extend `open_chat_shows_a_file_the_export_holds`. The export downloads,
copies and deletes file 1 first, then the page with the photo arrives. Expect
`downloads: 1` and `deletes: 0` while the chat is open, and `deletes: 1` after
the chat closes.

## 3. A quit request that cannot ask is let through, so the export dies with no question

Where: `third_party/gpui-pre/src/app.rs:1385`, `src/ui/os_menus/mod.rs:337`,
`third_party/gpui-pre-macos/src/platform.rs:1432-1435`,
`third_party/gpui-pre-windows/src/platform.rs:1321`.

`App::on_quit_request` answers true when the app is already borrowed. The
Windows platform's own comment at `platform.rs:1301` says broadcasts arrive
while the app is mid-update, for example inside a COM call.
`WM_QUERYENDSESSION` is such a broadcast. The macOS `should_terminate` also
answers `TerminateNow` when the callback is out of its slot, which happens on
re-entry. `os_menus` treats an `Err` from the window or entity update the same
way. Each of these ends a running export with no box. That is the L18 bug
again.

Fix: when the app cannot be asked, refuse and ask later. Do not go ahead.

- macOS: answer `NSTerminateLater` and post a foreground task. The task runs
  the callback. When it allows the quit, the task calls
  `[NSApp replyToApplicationShouldTerminate:YES]`. When it refuses (the box is
  up), it calls `replyToApplicationShouldTerminate:NO`. Stop then quits through
  `Platform::quit`, as now.
- Windows: answer FALSE and post `WM_GPUI_TASK_DISPATCHED_ON_MAIN_THREAD` work
  that runs the same check. `ENDSESSION_CRITICAL` stays as it is.
- `App::on_quit_request`: on `try_borrow_mut` failure, return "ask later"
  instead of true. A tri-state `QuitAnswer { Allow, Refuse, Later }` makes
  this explicit for both platforms.
- `os_menus`: go ahead only when the update ran and nothing holds the exit.
  On `Err`, allow only if no session is exporting.

Also add `ShutdownBlockReasonCreate` on the platform window while the box is
up, with text from the lang system. Without it, Windows shows the shutdown
screen with no reason. Remove it with `ShutdownBlockReasonDestroy` when the
box closes.

Test: `simulate_quit_request` while the test app is borrowed must not return
allow.

## Smaller points

- `held_file` goes stale. It is cleared only by `deleteFile`. After a
  `cancelDownloadFile`, a skipped file, or a file cached before the export,
  it stays set until the next download. `should_download` then answers yes
  for a complete file. That costs one no-op pass through
  `send_auto_download` and an entry in `view_files`, and nothing more. Clear
  it on `cancelDownloadFile`, and on the next `getFile` for another file. Or
  have the worker name the file it holds. Finding 2's fix replaces this field
  anyway.
- `stop_exports_and_wait` (`src/ui/export_quit.rs:37`) blocks the main thread
  in a 5 ms sleep loop for up to 400 ms. It runs only on quit with an export
  running, so it is acceptable. With finding 1 fixed, the wait is one chunk.
- N18 doubles peak disk use for each file on volumes that do not clone, which
  is the owner's choice. A near-full disk now fails an export that used to
  work. Say so in the PR text.

## Checked and found sound

- No quit loop. Stop runs `quit_app`, and `Platform::quit` sets
  `quit_decided`, so `terminate:` does not ask again. Restart goes through
  `self.quit()` too.
- No prompt loop. A second request replaces the stored exit and opens no
  second box. `on_close` clears it.
- `flush_export_requests` ends. Each pass takes one message, a cancelled
  worker posts at most a `cancelDownloadFile` or `deleteFile` and then ends,
  and a link that is not cancelled stops at its first sent request.
- The wait reads progress set by the worker thread itself, so it does not need
  the poll loop it blocks.
- Failed saves now release the cache copy before the error returns, in both
  `save_once` and the account sections.

## Not checked here

- macOS and Windows code does not compile in this container.
  `cargo check -p gpui-pre-macos --target aarch64-apple-darwin` fails in
  `objc2-exception-helper`, since there is no Apple `cc`.
  `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu` fails in
  `gpui-pre`'s build script, since there is no `x86_64-w64-mingw32-windres`.
  The PR reports a clean msvc check. A Mac check should confirm the
  `NSApplicationTerminateReply` names, and the `NSTerminateLater` fix should
  get a Mac run.
- No real logout, shutdown or TDLib run. Whether TDLib finishes a `deleteFile`
  sent just ahead of `close` is still unknown. The journal in finding 1
  removes the need to know.
