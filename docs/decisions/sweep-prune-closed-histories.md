# Bounded histories for chats nobody is looking at

## What tdesktop does

`Data::Histories` and `History` keep the blocks of the chats that are on
screen. `History::clear`, `unloadBlocks` and the "unload" of inactive
histories drop the rows of the rest, and a chat shown again is built from
what is cached and then loads what is missing (`HistoryWidget::showHistory`
asks for the latest messages and the read boundary). Nothing is dropped by a
timer that races with a view; it follows the active chat changing.

## What Quill did before

`Session::histories` kept every chat's loaded window for the whole session.
`history_trim` capped one chat's window (1,500 rows), nothing ever removed a
chat, and live messages created an entry (up to 1,500 rows) for every chat
that ever received one. `ThreadsState::topic_histories` behaved the same.

## What changed

`src/state/history_prune.rs`, driven only by state transitions in the
reducer:

- A chat that stops being shown (`open_chat` moving on, `leave_open_chat`,
  `close_window_chat`) keeps its newest `CLOSED_TAIL_KEEP` (60) messages.
  `loaded_complete` clears so older rows page back in; a pending or failed
  local send always stays. Its forum topics shrink the same way, and the
  topic paging cursor moves to the oldest kept message. Leaving a topic
  (`select_topic` for another, `deselect_topic`) trims that topic.
- A chat or topic that stops being shown is not trimmed in the same
  event. It joins a release queue that `Session::set_history_holds` settles
  once per frame, after the UI's claims are in, so a claim and a chat
  switch in one event cannot trim the source. A release that finds the chat
  held stays queued and goes through when the hold lifts (also for a chat
  that closed while held). Until it is settled a live message keeps the
  page cap, not the closed tail.
- The closed chats and topics that keep a tail are an LRU
  (`CLOSED_CHATS_MAX` = 16, `CLOSED_TOPICS_MAX` = 16). A live message in a
  closed chat counts as use and keeps the tail within `CLOSED_TAIL_CAP`
  (100). Past the bound the least recently used history is dropped whole,
  except an entry with an unsent or failed local message.
- A chat that was left (status Left and in no list) is dropped as soon as no
  view shows it. `deleteChat` and `leaveChat` answers call the same code.
- A history is never pruned while the main window, a chat window, a topic
  view or a comment thread shows it, or while the UI holds it
  (`Session::set_history_holds`, synced once a frame from
  `ui/history_holds.rs`): the source chat of a pending forward, the chat the
  composer's reply bar points at ("Reply in Another Chat" replies to a
  message of a chat that is no longer shown) and the chat of the track in
  the player bar. The reducer cannot see those UI fields, so it learns of
  them through the sync; the queue above is what makes the sync early
  enough.
- A late topic page for a closed topic goes through
  `Session::topic_history_entry`, which puts the new entry in the topic LRU
  like `history_entry` does for chats.
- Every history entry, however it is created (`Session::history_entry`:
  send answers, deletes, read markers, late pages, search), joins the LRU
  unless a view, the release queue or a hold covers it, so
  `CLOSED_CHATS_MAX` really bounds the kept histories. Only the entries of
  shown chats sit outside it.
- Dropping an entry whole keeps its deleted-message ids (newest 256 per
  chat) and `latest_seen` in a side table for the last 64 dropped chats;
  `message_deleted` reads it and a recreated entry takes it back. Without it
  a reply or forward draft aimed at a message deleted earlier would pass
  the "is it gone" check and fail at send time.
- Releasing a chat touches its topics in id order, so the LRU order and
  the eviction victim do not depend on hash map iteration. Re-selecting the
  open chat releases the topic it left, and closing a comment thread by
  opening another chat releases the thread's chat.
- The reply bar and the history: when the replied-to message is not in
  memory (older than the kept tail, or a draft restored on reopen), the UI
  asks for it with `getMessage` once (`ConnectDriver::fetch_composer_reply_target`).
  The answer is kept in `MessagesState::composer_reply_fetch`, next to the
  history, never inside the window, which has to stay one contiguous run.
  `Session::reply_target` falls back to it; a 400, 403 or 404 answer marks the
  target deleted (the reply cancels like any deleted target). Any other error
  (a lost request settles as 408, flood 429, server faults), or a request
  that could not be sent, leaves the reply alone and marks the fetch failed;
  it is asked again while the connection is Ready after a wait of 2, 4 and
  8 seconds, three attempts in all. The 32-entry cap makes room by dropping
  settled entries only, never one in flight or the one being fetched. tdesktop's `FieldHeader` requests
  the replied-to item when it is missing the same way.
- Chat-list "Mark as read" views the newest loaded message, or the chat
  summary's last message when the history was dropped (TGX views
  `chat.lastMessage`).
- Reopening a closed chat goes through the same path as before
  (`open_chat`, `prepare_history_window`, `fetch_history`): the tail shows at
  once, a window that does not reach the unread boundary is replaced, and
  the next page loads from the oldest kept row. A chat whose history was
  dropped loads as a never-opened chat does.

## Readers checked

Every reader of `Session.histories` and `topic_histories` acts on a shown
chat, except these:

- forward and selection source (`selection_mode`, `share_box_ui`,
  `session_share`): held.
- player bar and voice/audio playlists: the chat is held while its track
  plays. The playlist is limited to the loaded rows, as before.
- `download_display_name`, `sticker_requires_premium`: scan whatever is
  loaded; a dropped history only means a name falls back to the existing
  default.
- `message_deleted`: tombstones are kept when trimming and, past dropping
  an entry, in the side table; "not loaded" is not "deleted" there.
- Pinned messages (`messages.pinned_messages`), unread counters, the chat
  list preview and notifications read their own state (`ChatSummary`, the
  notification payload), not `histories`.
- Scroll restore (#949): a left chat keeps only the newest 60 rows, so a
  saved reading position older than that is not in memory on reopen.
  `Session::prepare_history_window_at` checks the window as it is after the
  trim: when the tail no longer holds the saved row the window is replaced
  and the first page loads around it (`saved_open`); when the tail still
  holds it the window is kept and the row is restored in place. The trim
  runs at the frame's holds sync, before any later open, so the check never
  sees rows that are about to go.
- Chat-list "Mark as read": falls back to `ChatSummary.last_message`, see
  above.

## Sessions with no server

The screenshot demos seed every message in memory and answer no
`getChatHistory`, so a trimmed or dropped history could never come back:
the demo's chats lost their older rows for good on the first switch, and a
saved scroll position older than the tail opened at the unread divider
instead (`ui::scroll_restore_tests::returning_to_a_chat_lands_on_the_row_it_was_left_at`
failed with the top row on the oldest kept message, 2060). The demo open
path has no around-load to fall back on. `Session::keep_all_histories`,
called by `seed_demo_session`, turns pruning off for such a session: closing
queues no release, new entries join no LRU, and live messages in a closed
chat keep the page cap as before this change. Live sessions are unchanged.

## Notes

- `viewed` ids: `trim_to_tail` drops them with the rows, `trim_window_end`
  (a shown chat's page cap) keeps them. A reopen clears `viewed` anyway
  (`prepare_history_view`), so a closed chat never re-sends
  `viewMessages` for rows it already reported; an open chat keeps them so a
  trimmed-then-repaged row is not viewed twice.
- The release queue is settled from the frame's holds sync. A frame that
  never renders (a minimised window) leaves closed chats whole until it
  does; live messages in them keep the page cap meanwhile.

## Not covered

Other per-chat side tables (translations, sponsored messages, media counts)
are not pruned here. The media viewer's prev/next and the unread
mention/reaction jump buttons read the shown chat only.

## Side tables that go with the chat (2026-10 memory sweep)

- `messages.reply_targets`, the originals `getRepliedMessage` fetched for
  rows whose replied-to message is outside the window, and
  `messages.pinned_messages` were kept for the whole session, a boxed
  message each. They now go once nothing of the chat is kept: its history
  entry dropped (LRU or a chat left) and no topic of it left. Opening the
  chat fetches both again. A kept tail keeps them all, rows trimmed off it
  included: dropping those with the trim made reply strips of rows paged
  back in (coming back to a chat left scrolled up) show "Loading" until
  the answer came.
- A background account: releases settle from the frame's holds sync, which
  runs for the account on screen only, so the chat an account had open when
  it went to the background kept its whole window. Going to the background
  (`ConnectDriver::leave_for_background`) now closes the chat and settles
  the release at once; nothing on screen holds a background account's
  chats.

Tests: `state::tests::history_prune::a_kept_tail_keeps_the_replied_messages_of_rows_paged_out`,
`a_dropped_chat_forgets_its_pinned_list_and_replied_messages`,
`a_forum_keeps_its_replied_messages_until_its_last_topic_goes`, and
`connect::tests::chat_window::an_account_going_to_the_background_trims_the_chat_it_showed`.

## How verified

Unit tests at `Session` level in `src/state/tests/history_prune.rs`:
closing trims to the tail, reopening keeps it, leaving drops the chat, open
and window views and held chats are never pruned, live rows stay bounded,
the chat and topic LRUs respect their bounds, and an unsent message keeps
its entry. Added: a hold set in the same event survives an immediate chat
switch and a lifted hold releases the chat, a reply to another chat holds
its source, topic release order is deterministic, entries made outside the
LRU stay bounded, deleted ids survive dropping an entry, and re-selecting the
open chat releases its topic. Driver tests (`connect/tests/chat_state/list_ops.rs`):
the reply bar sends `getMessage` once for a target that left memory and shows
the answer without putting it in the window, and "Mark as read" works after
the history was dropped. The existing history-window tests still pass.
With #949: `connect::tests::history_window` leaves a chat, settles its
release (80 rows trimmed to 21..80) and reopens it at a saved row below the
tail (one `getChatHistory` around it, the row restored, no fallback) and at
a saved row inside the tail (window kept, no request).
`state::tests::history_prune` checks that a session without a server keeps
every history, and `ui::scroll_restore_tests` passes again on the demo.
