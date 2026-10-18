# sweep-r3-row-patches: build only the rows around a message that came or went

Round-3 perf audit, item 2 ("any message mutation rebuilds all rows from a full clone").

## What #986 already covered

`sweep-r3-history-chrome` gave `HistoryMessages` per-message revisions and the
history list `RowsInput::Keep / Patch / Build`. A message changed in place (an edit, a
reaction, a view count, a read receipt) patches its own row. Anything that adds or removes
a message moves `ids_revision`, and that still built every row from a clone of the whole
window.

That covers most of the events this item names, because these all add or remove a message:

- a new message in the open chat
- a deletion
- a pending send: the row appears, then `updateMessageSendSucceeded` or
  `updateMessageSendFailed` swaps the temporary id for the server id (`replace_id` is a
  remove plus an insert)

## Measured before

`rows_cache_long_tests.rs::measure_work_per_update_on_a_long_history` opens a group with
1003 rows in live (follow-revision) mode, applies one update and counts chromes derived,
rows patched, whole-list builds and messages copied into rows. Run on `dc7f7b19` with
the new cases added:

| Update | chromes | patched | rebuilds | cloned |
| --- | --- | --- | --- | --- |
| Message in another chat | 25 | 0 | 0 | 0 |
| Edit one message | 26 | 1 | 0 | 1 |
| Reaction on one message | 25 | 1 | 0 | 1 |
| View count of one message | 25 | 1 | 0 | 1 |
| New message at the end | 1004 | 0 | 1 | 1004 |
| Delete one in the middle | 1003 | 0 | 1 | 1003 |
| Own message sent (pending row) | 1004 | 0 | 1 | 1004 |
| Send succeeded (pending to sent) | 1004 | 0 | 1 | 1004 |

So in-place changes were already patches. The cost left was messages coming and going.

## Design

**`HistoryMessages::changes_since(rev)`** returns the ids stored or changed after `rev`,
plus the ids removed after it. Removals sit in a log of the last 64 (`REMOVED_LOG`), each
with its revision. Asking from before the oldest entry still in the log, or from before a
`clear`, returns `None`, and the rows are built whole. The log is bounded, and it is
dropped when the window is replaced.

**`RowsInput::Splice`** (`row_splice.rs`). When the rows key differs only in
`messages_revision` and `ids_revision`, and the changes are not a pure in-place patch:

1. The affected ids are the changed ids plus the removed ids, at most `PATCH_MAX` (32).
   The rows are in id order, followed by the rows the list adds after the messages (the
   joined-channel notice, `RowsSource::tail`). A binary search finds the rows from the
   lowest affected id to the highest.
2. The region adds one kept row on each side. The row above learns who is below it (the
   avatar sits on the last row of a sender run), and the row below learns who is above
   it (the sender name, the day separator, the unread divider, the joined bubble corner).
   A region over 64 rows (`SPLICE_ROWS_MAX`, changes far apart) builds the whole list.
3. The region's messages are the kept rows' own copies, minus the affected ids, plus the
   changed messages from the session (translated as on a build). Only these are cloned.

**`build_rows`** (`row_build.rs`) is the old build loop moved out of
`history_message_list.rs`. Its behaviour is the same. It takes a `RowSeed` (the sender
of the row above, the day of the nearest dated row above, whether the unread divider is
already placed, the rows above) and the sender of the row below, and it returns the seed
it hands down. The full build starts from `RowSeed::default()`. A splice starts from
`RowSeed::before(rows, start)`, read off the kept rows.

**Rows below the region.** After building the region, the splice compares the seed it
hands down with the seed the old rows handed down at the same place (`carries_like`).
If they differ, the next old row is built again from its own messages and the check
repeats. For example, a message that lands as the new first unread moves the divider off
a row further down. Album groups cannot spill across the region's edges. Two adjacent
kept rows were separate groups before, and their boundary messages did not change.

**The scroller.** `sync_spliced_scroller` gets the same events the full build sent:

- same row count: remeasure the region rows that do not render like before
- rows only added at the end: the old `append_tail_rows` path (tail follow, the reveal
  animation, the wallpaper turn on a sent message), plus a remeasure of the changed
  region rows
- anything else: `splice(start..start + old_len, new_len)` on the scroller, which keeps
  the scroll anchor, then the end again if the user was following the tail

The full build's tail-append and post-build bookkeeping moved into `append_tail_rows` and
`rows_replaced`, which both paths call.

**Chrome.** New rows get this render's chrome. Kept rows near the viewport get the
`stale_rows` treatment of the Keep/Patch path, with indices moved past the splice, so
chrome follows the same rules as #986.

**Not spliced, built whole** (same as before):

- thread lists (`tail: None`): the root post and the divider head the replies
- a streaming bot reply below the rows
- a delete animation in progress. The key already changes every frame, and
  `row_splice` also declines while `vanish_rows_hash` is set, because only a full
  build merges the crumbling messages back into the rows
- more than 32 changes, or changes more than 64 rows apart
- a window replaced, a new chat, a highlight or translation change

## Review hardening (#1011 review)

- **Instance token.** Revisions and the removal log mean something only inside one
  `HistoryMessages`. Some code replaces the whole value (`history_entry` default, a
  reopened saved sublist or tag search, `TopicHistory::default`) without moving the window
  epoch, and a small replacement looked like "a few messages changed" against the old
  revision, so rows outside it survived. Each `HistoryMessages` now takes a unique
  `instance` from `next_revision()` at creation, `changes_since(instance, revision)`
  returns `None` for another instance, and the rows key carries the instance, so the
  patch path rebuilds too. No replacement site needed a change.
- **Vanish.** The splice declines while any message is crumbling.
- **Cost.** `RowSeed::before` and `changed_since` (and `changes_since`) are O(rows) per
  splice: they walk the kept rows above the region and every message's revision. That is
  a pointer walk, with no copying and no chrome derived; the measured cost is the copy
  and chrome counts in the tables. A per-row index would remove it if profiles ask for it.
- **Revision wraparound.** `next_revision` is a `u64` counter; it would take centuries at
  a billion changes a second to wrap, so it is documented, not handled.

## Measured after

Same test, same machine:

| Update | chromes | patched | rebuilds | splices | cloned |
| --- | --- | --- | --- | --- | --- |
| Message in another chat | 25 | 0 | 0 | 0 | 0 |
| Edit one message | 26 | 1 | 0 | 0 | 1 |
| Reaction on one message | 25 | 1 | 0 | 0 | 1 |
| View count of one message | 25 | 1 | 0 | 0 | 1 |
| New message at the end | 26 | 0 | 0 | 1 | 2 |
| Delete one in the middle | 27 | 0 | 0 | 1 | 2 |
| Own message sent (pending row) | 26 | 0 | 0 | 1 | 2 |
| Send succeeded (pending to sent) | 26 | 0 | 0 | 1 | 2 |

The chrome count is the rows around the viewport (about 25 in this test) plus the
spliced rows.

Demo trace (`QUILL_TRACE_TICKS=1` with the new `QUILL_DEMO_STREAM_OPEN=1`, below): in
`ready-group-commands`, `ready-albums` and `ready-reactions`, the only whole-list builds
are in the first second, when the chat opens. Every later new message and deletion shows
up as a splice, and every reaction or view count as a patch.

These are counts only. This change makes no claim about frame time or memory.

## Tests

- `row_splice_tests.rs::patched_and_spliced_rows_equal_a_full_build`: a group of 120
  messages with runs of three senders, a new day every ten messages, a three-item album
  that opens a day, a two-item album and an unread divider. After each update, it
  compares every field of every row (except the chrome generation) with the rows a full
  build makes from the same state, and checks that the scroller lists every row. Updates
  covered: edit, reaction and view count (patch); new message at the end, same sender
  again, a new day at the end, pending own message, send succeeded, second pending, send
  failed, a message landing in the middle, delete the first message of a day, delete the
  first unread, delete the last of a run, delete an album item, delete an album down to
  one item, delete a whole run so its neighbours join, delete the first message (splice).
- `a_new_first_unread_moves_the_divider_from_rows_further_down`: the region has to grow
  down to the old divider four rows below. With the `carries_like` loop disabled this
  test fails on row 16, so the loop is needed.
- `changes_far_apart_build_the_rows_whole`: two deletions about 100 rows apart build the
  whole list.
- `rows_cache_long_tests.rs::a_message_coming_or_going_builds_only_the_rows_around_it`:
  on the 1003-row history, a new message, a deletion and a pending send that then
  succeeds cost no whole build, one splice each and at most 3 messages copied.
- `state/tests/history_messages.rs`: `changes_since` names the ids added and removed, and
  forgets removals past the log or before a `clear`; it knows only its own instance;
  the log holds exactly 64 removals.
- `a_replaced_history_builds_the_rows_whole`: the main history replaced by a 5-message
  window with the epoch unchanged builds whole and equals a full build. Fails with the
  instance check removed.
- `a_crumbling_message_makes_the_rows_build_whole`: a delete and a new message while a
  message dissolves build whole and equal a full build, and `row_splice` returns `None`.
- `a_splice_keeps_the_reader_where_they_were`: scrolled away from the tail, a message
  landing or going above and below the viewport leaves the top message and its offset
  unchanged and the list not following the tail.
- `a_splice_keeps_following_the_tail`: at the tail, a splice in the middle, a deletion and
  a message at the end keep following with no scroll anchor.
- `more_removals_than_the_log_holds_build_the_rows_whole`: 66 removals in one render.
- `the_saved_sublist_splices_like_the_open_chat` and
  `a_topic_history_splices_like_the_open_chat`: a message landing and leaving splice and
  equal a full build; replacing the sublist or topic history builds whole.

## Verification

Demo-capture build, `QUILL_DEMO_UPDATE_STREAM=4 QUILL_DEMO_STREAM_OPEN=1`, light and dark,
read back:

- `ready-group-commands` (group "Weekend plans"): stream messages from the Weather bot
  and the user's own alternate. A run of two incoming messages shows the name on its
  first row and the avatar on its last only. A message whose run partner was deleted
  shows both name and avatar. Reactions and view counts show on the bubbles.
- `ready-albums`, scrolled so the own-sent album and the stream show together: the new
  "October 17" day separator sits under the album from September 28, deleted stream
  messages are gone, and reactions show.
- `ready-reactions`: same, under the demo's own reacted messages.

`QUILL_DEMO_STREAM_OPEN=1` is a fixture switch in `demo_stream.rs`. Every other update of
the stream goes to the open chat, in a cycle: a new message, a reaction and view count on
the newest, a new message, a reaction, a new message, then deletion of the stream message
above the newest. Two of the three new messages come from the chat's last incoming
sender, and one is the user's own.

Not shown by the captures: real TDLib ordering of temporary send ids (the tests use ids
above every server id, as TDLib does). Topic and saved sublist lists take the same splice
path and have tests of their own, not captures.

## Not covered

- A change near the bottom plus a change far up in the same render (for example a
  reaction on an old message together with a new message) builds the whole list. The
  region is one run of rows, not one per cluster.
- Thread views still build whole on a new reply.
