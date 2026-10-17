# Keeping the UI thread off disks and helper pipes (sweep round 3, items 5, 6, 9, 10)

Telegram Desktop has no direct counterpart here: it runs its own event loop and
writes settings from its storage thread. The point of this change is that the
Quill UI thread never waits on a disk sync, a helper pipe or a child process.

## What changed

- **Settings writes (5).** `settings::write_json_deferred` serializes on the
  caller's thread and queues the bytes on a short-lived writer thread
  (`src/settings/deferred.rs`). Queued writes are coalesced by path, so the
  newest bytes for a path win. Each write is still the atomic temp-file,
  sync, rename write (`write_bytes_atomic`, split out of `write_json_atomic`
  with identical output). `flush_deferred_writes` waits for the queue (5 s cap)
  and runs on app quit next to the window-state flush. Window geometry already
  had its own latest-wins saver; the other listed callers moved to the deferred
  writer: saved chat windows, notification prefs (`store` and `persist`),
  fast buttons and web app trust. `load_chat_windows_from` reads a queued write
  first, so a load right after a save sees the new state. The file contents are
  the same as before.
- **Mini-app helper pipe (6).** `HelperWriter` (`src/ui/web_app_ui/helper_pipe.rs`)
  owns the helper's stdin on its own thread behind a 64-message bounded queue.
  `send` uses `try_send`; when the queue is full or the writer has died the
  message is dropped, a line is logged to stderr and the helper is killed once,
  which closes the window through the reader's `Closed` event. The reader uses
  `read_bounded_line`, which never stores more than the cap and skips the rest
  of an oversize line.
- **Reaping children (9).** `child_reap::kill_and_reap` and `reap_after` hand
  the `Child` to a detached thread. `CameraStream` and `MiniAppWindow` use them
  in `Drop`; the 200 ms grace wait for the helper moved off the UI thread.
- **PTT listener (10).** The key-edge listener waits with `recv_timeout`
  (250 ms slices) and checks a stop flag held in `GroupCallState`. The flag is
  set when the hook is torn down or replaced. A disconnected sender also ends
  the loop. The sender lives in the hook's sink, which the platform thread
  drops when its handle is dropped; the stop flag covers a hook that does not.

## Tests

- Coalescing writer: last write wins while a write is in flight, separate paths
  are all written, flush waits and the file on disk holds the last bytes.
- Helper writer with a pipe that never drains: 20 sends and the drop return
  promptly, with the surplus reported as wedged. A broken pipe reports wedged.
  Healthy pipes keep order.
- Bounded line reader: oversize line skipped across many small buffers and the
  next line still reads; unterminated tail line is returned.
- `kill_and_reap` returns at once and the process ends.

## Not covered

The PTT listener change and the Windows fsync stall were not run on hardware.
The behaviour is checked by the tests above and the existing emoji-status
window tests, which now run through `HelperWriter`.
