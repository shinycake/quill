# History rows rebuild only when their inputs change

This covers findings 1, 2, 5 and 8 of the round 2 performance and memory sweep.

## Before
- `HistoryRowsKey` included `session_revision`. Any session update rebuilt every row and
  remeasured the list: typing in another chat, an online status, a chat list reorder.
- When anything was playing, `history_rows_ui_hash` added the playback position, so every
  250 ms tick rebuilt all rows. The tick also called `cx.notify()` on the whole app, which
  re-rendered the sidebar, the composer and every chat window.
- The slow-mode (1 s) and self-destruct ticks also notified the whole app.
- Thread and topic views cloned their whole history into a `Vec` on every render, even when
  the rows were cached. The saved-messages sublist had no revision at all.

## After
### Rows key only on what they show
A row has two parts:
- Its content (`HistoryRowInputs`) depends on the message, its neighbours, the day, the unread
  divider and the UI toggles. It is keyed on the list's `HistoryMessages::revision()` and is
  rebuilt only when that key changes.
- Its chrome (`RowChrome`) is the part read from the session: sender label and avatar, outbox
  read state, reply, forward and via-bot headers. When `session_revision` moves, a chrome
  pass (`ChromeInputs::stale_rows` in `src/ui/conversation/row_chrome.rs`) derives the chrome
  for each row, patches only the rows whose chrome differs, and remeasures only those rows.

Narrow per-input counters (users, read state, chats) were considered and rejected. Session maps
such as `users` and `chats` are public and are mutated directly at more than 65 sites. The same
maps also carry status fields (online, typing) that no row reads, so a users counter would still
fire on every status update. Comparing the derived chrome is exact and keeps rows a pure
function of state.

`HistoryMessages` revisions now come from one process-wide counter (`next_revision`). A history
that replaces another (a reloaded chat, a new thread) can never repeat a revision that a cached
key holds. `TopicHistory.messages` is now `HistoryMessages`, which gives topics, threads and
saved sublists a revision.

In demo mode, fixtures change users and chats without going through `Session::apply`, so
`session_revision` cannot be trusted there. Demo mode (other than the stress fixture) runs the
chrome pass on every render. It costs well under a millisecond on demo histories.

### Playback is read at render time
The seek bar, the playing-animation flag and the GIF frame were taken out of
`HistoryRowInputs`. `history_row_seek_bar` and the single-row renderer read them from
`self.playback` when the visible row is drawn. A tick changes no row input, so the cache holds.

### Ticks redraw only what shows them
`notify_tick(SliceKind, cx)` in `src/ui/app_slice.rs` notifies:
- inside a chat window scope, that window's view;
- in the main window, the slice that shows the timer;
- the whole app only when the conversation is not on screen.

`notify_conversations` notifies the conversation and composer slices and every chat window.
The playback tick uses it, plus `touch_bar::request_sync` on macOS, because the Touch Bar used
to follow app-wide notifies. The final playback tick (track ended) still does a full notify, since
the player bar closes. The slow-mode tick notifies the composer, and the self-destruct tick
notifies the conversation.

The story viewer (100 ms) and call timer (1 s) ticks were left alone. They are overlays drawn at
the `QuillApp` root, so they have no slice to target.

### Thread, topic and saved views use the keyed cache
Each list computes its key from its history revision and an `extra` hash (the joined notice for
the main list, the root ids for a thread). The history is cloned only when the key misses
(`src/ui/conversation/rows_cache.rs`).

## Measurements
Setup:
- debug build with `--features demo-capture`, run at 1200x800 with `QUILL_ASSUME_ACTIVE=1`;
- CPU seconds counted over 30 s, after a 12 s warm-up (`ps` time);
- the before and after binaries ran at the same time, so both saw the same machine load (other
  builds were running).

| Scenario | Before | After |
| --- | --- | --- |
| `ready-audio`, playback tick running (run 1) | 3.53 s | 2.68 s |
| `ready-audio`, playback tick running (run 2) | 3.43 s | 2.58 s |
| `ready-chats`, 200 chats x 1500 messages, 30 updates/s stream (run 1) | 3.54 s | 3.39 s |
| `ready-chats`, same stream (run 2, traced) | 3.53 s | 3.50 s |

Slice renders per second during playback (`QUILL_TRACE_TICKS=1`):
- Before: app notified 4, sidebar 4, composer 4, conversation 4.
- After: app notified 0, sidebar 0, composer 4, conversation 4. No row rebuilds; one chrome pass
  per tick at about 40 µs, with 0 rows patched.

Under the update stream, the after build logs one chrome pass per second at 0.6 to 1.9 ms with
0 rows patched, and no rebuilds after startup. A full rebuild of the same window measured about
2 ms. The before build rebuilt and remeasured every row on each of those redraws. The stream
CPU difference is within noise: the chrome pass still visits every row in the window, and the
saving is mostly the skipped remeasure.

Memory (RSS at the end of the window) moved within run-to-run noise: 107 to 165 MB in both builds.

## Tests
`src/ui/conversation/rows_cache_tests.rs`:
- an unchanged redraw reuses the rows;
- a sender rename patches only that sender's row;
- a read receipt patches the outgoing row;
- an edit or a reaction rebuilds the rows and shows the change;
- typing and status updates in another chat touch no row;
- voice playback keeps the rows while the seek bar follows the position;
- a thread reuses its rows until a reply arrives;
- timer ticks redraw only the slice that shows them.

The `ready-thread` demo (`src/ui/threads_demo.rs`) is new. It gives the thread view a capture.

## Not verified
- Not run against a live TDLib account.
- The Touch Bar resync during playback was not checked on hardware.
- Tick scoping in separate chat windows is covered by the `notify_tick` logic only. No capture
  shows a detached window.
