# sweep-r3-history-chrome: row chrome, in-place patches and a bounded file map

Round-3 perf audit, items 1, 2 and 4.

## Problem

1. Every change to `Session::revision` rederived the chrome (sender label, receipt, avatar,
   reply/forward headers) of every kept history row, up to 1500. A message in another chat or a
   download tick for a file no row shows was enough to trigger it. `history_media_signature`
   also scanned all of `media.files` on every render.
2. `HistoryMessages::get_mut`, `values_mut` and `iter_mut` moved one revision, and the rows key
   included it. One edit, reaction or read-state change cloned the whole open history and built
   every row again.
3. `media.files` grew with every `updateFile` and never shrank.

## Data shapes

**`HistoryMessages`** (src/state/history_types.rs) keeps the map plus:

- `changed_at: HashMap<i64, u64>`: the global revision each message last changed at.
- `revision`: the newest change of any kind.
- `ids_revision`: moves only when a message is added or removed.

`changed_since(rev)` returns the ids that changed after `rev`. An insert of a known id, `get_mut`
of an existing id and `retain` touch only the ids they reach. A miss changes nothing.

**`RowsInput`** (src/ui/conversation/rows_cache.rs) is what the history list gets each render:

- `Keep`: nothing about the rows changed.
- `Patch(Vec<(row index, HistoryMessage)>)`: only messages changed in place, at most 32, and each
  still sits in the same row with the same sender, direction, date and album.
- `Build(Vec<HistoryMessage>)`: anything else.

`HistoryRowsKey` carries `ids_revision`. `differs_only_in_place` says when two keys differ only
in the message revision, which is when a patch is possible.

**Chrome generation.** Each row carries a `chrome_stamp`, and `HistoryState` keeps
`chrome_epoch`, `chrome_seen` (the session revision the epoch stands for) and `chrome_due` (rows
drawn while stale). `chrome_mark` starts a new epoch when
`Session::revision_without_file_progress` moves. That is the session revision minus the
`updateFile`s that changed no ready path, counted by `FileMap::quiet_updates`, so it is derived
from state rather than kept in step by hand. Screenshot demos change state without moving the
revision, so there the epoch moves on every render, which matches the old behaviour.

**`FileMap`** (src/state/domains/media/file_map.rs) replaces `HashMap<i32, ParsedFile>`:

- Each entry holds the file and a last-use tick. `get` advances the tick (a read-touch through a
  `Cell`). `values` and `iter` do not.
- `paths_revision` moves when a file's usable path changes. It replaces the per-render scan in
  `history_media_signature`.
- Past `FILES_MAX` (8192), `evict` drops the least recently used files down to `FILES_KEEP`
  (6144), and skips every file the hold predicate keeps.

`ParsedFile` gained `uploading` (`remote.is_uploading_active`) so the hold predicate can see
sends.

## Design

**Chrome only near the viewport.** On a new epoch, only the rows from `scroll_view_probe` (the
last painted range) plus 16 on each side get their chrome rederived, along with any rows in
`chrome_due`. Each recheck compares the result with the stored chrome and remeasures only rows
whose chrome changed. A row further away keeps its stamp. When the list draws it,
`render_history_row` adds it to `chrome_due` and asks for one more render. The list draws rows
during the window draw, when a `cx.notify()` is lost, so the notify goes through `cx.defer`,
once per frame. The first frame after scrolling far shows the old chrome. The next one fixes it
(test: `a_row_scrolled_into_view_shows_a_rename_made_while_away`).

Every other part of each row (message, layout, cached rich text) is unchanged, so the key and
the scroller state are as before.

**Patching in place.** `rows_input` asks `HistoryMessages::changed_since` for the ids changed
since the rows were built. Each id's row comes from the existing index. The translation overlay
is applied the same way as on a build. Only the patched rows are cloned, get new chrome and are
remeasured. If too many ids changed, an id has no row, or a change would move a row boundary, it
falls back to a full build.

**Eviction holds.** A file is held, and never evicted, while it is:

- downloading (`is_downloading_active`) or uploading (`uploading`)
- requested, paused or user-started in `downloads`
- an avatar (`avatar_file_refs`)
- one of the open chat's media downloads, or a download extra
- a notification sound
- listed in the Downloads panel (finished or failed) or a track of a queued playlist

A file on screen is not held. It is protected by recency: `FileMap::get` touches it on every
render, so it stays among the most recently used, and 6144 of those survive each pass. If held
files alone fill the map, a pass that frees too little is not retried until 512 more writes. An evicted file that comes back shows up
again in the next `updateFile`, the same as a file seen for the first time.

## Measured

All numbers below are counts taken by tests on this machine.
`rows_cache_long_tests.rs` opens a group with 1003 messages in live (follow-revision) mode and
applies one update, then reads the counters: chromes derived (`row_chrome::chrome_built`, test
only), rows patched, row rebuilds and messages cloned into rows. The baseline is the same test
run on d848a7c8.

| Update | Before: chromes / patched / rebuilds / cloned | After |
| --- | --- | --- |
| Message in another chat | 1003 / 0 / 0 / 0 | 25 / 0 / 0 / 0 |
| Download progress, file not shown | 1003 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| Edit one message | 1003 / 0 / 1 / 1003 | 26 / 1 / 0 / 1 |
| Reaction on one message | 1003 / 0 / 1 / 1003 | 25 / 1 / 0 / 1 |

Files kept after 20000 distinct finished `updateFile`s (measured): 20000 before, 7706 after
(cap 8192).

Not measured: wall-clock time per frame, and resident memory. This change makes no claim about
either.

## Tests

- `rows_cache_long_tests.rs`: an unrelated update rebuilds no rows; editing one message
  rebuilds one row; a row scrolled into view shows a rename; the measurement tests above.
- `rows_cache_tests.rs`: an edit and a reaction patch only their row.
- `state/tests/history_messages.rs`: what `changed_since` and `ids_revision` report.
- `state/tests/files_bound.rs`, `file_map_tests.rs`: over 3 x `FILES_MAX` distinct files the
  map stays bounded, keeps the files being downloaded, asked for, sent and shown, and drops an
  unused one. Download progress does not move `paths_revision`.

## Verification

Demo-capture build, light and dark:

- `ready-chats` with `QUILL_DEMO_STRESS=40,1500`, scrolled up by trackpad: bubbles, times and
  receipts render as before.
- `ready-reply-edited`: the edited text shows in the bubble, in the chat list and in the reply
  bar.

## Not covered

- Rows outside the window still hold chrome from an older epoch until they are drawn. The first
  frame that draws one shows the old chrome, and the frame after it shows the new one.
- An edit that changes a row's boundaries (a sender change, or an album join) still builds the
  rows again.
- Topic, thread and saved-messages lists use the same `RowsInput` path but have no
  long-history test of their own.
