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
  `ui/history_holds.rs`): the source chat of a pending forward or selection
  and the chat of the track in the player bar.
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
- `message_deleted`: tombstones are kept when trimming, and "not loaded" is
  not "deleted" there, so a dropped entry never reads as deleted.
- Pinned messages (`messages.pinned_messages`), unread counters, the chat
  list preview and notifications read their own state (`ChatSummary`, the
  notification payload), not `histories`.
- Scroll restore (#949) loads around a saved message, so a shorter window is
  fine.

## Not covered

`messages.pinned_messages` and other per-chat side tables are not pruned
here. A reply target in a draft of a closed chat that is older than the kept
tail is not in memory after the chat reopens until it is loaded; this change
adds no fetch for it.

## How verified

Unit tests at `Session` level in `src/state/tests/history_prune.rs`:
closing trims to the tail, reopening keeps it, leaving drops the chat, open
and window views and held chats are never pruned, live rows stay bounded,
the chat and topic LRUs respect their bounds, and an unsent message keeps
its entry. The existing history-window tests still pass.
