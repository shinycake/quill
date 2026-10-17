# Scroll position restore per chat

## What tdesktop does
`HistoryInner` counts `History::scrollTopItem` / `scrollTopOffset` on every scroll
(`countScrollState`), forgets them while the view is at the bottom
(`forgetScrollState`, history_inner_widget.cpp ~4585) and whenever the chat is shown at an
explicit target (history_widget.cpp 2850, 3110, 3339). Showing a chat at its default position
(`ShowAtUnreadMsgId`) with a saved item sets `_showAtMsgId = _history->showAtMsgId`
(history_widget.cpp ~3338), so the first load is the window around that message, and
`countInitialScrollTop` puts the saved item back at its offset when that window is shown.
A notification opens at `ShowAtUnreadMsgId` too, unless it is for an own message or a mention
(notifications_manager.cpp ~1343), so it restores like any default open.

## What Quill does
- `src/state/scroll_memory.rs`: `ScrollMemory`, an LRU (300) of `(chat, topic, thread)` to
  `(anchor message id, pixel offset)`. `ScrollMemory::open` is the open decision: an explicit
  target wins and drops the entry (`HistoryOpen::Message`), else a saved entry opens around its
  message with no highlight (`HistoryOpen::Saved`), else the default. `scroll_open` decides what
  the first rows scroll to: the saved row, the jump target, or the default, and drops the entry
  when its message is not in the window opened for it.
- Opening a chat (chat list, search, a chat link) goes through `QuillApp::chat_open_at` and
  `ConnectDriver::select_chat_open`. A saved position keeps a cached window that holds its
  message; otherwise the window is replaced and the first page is `getChatHistory` around the
  message, the same request a jump uses (`HistoryState::saved_open` marks it). The UI holds the
  first anchoring until that page is in, so the position is applied once, with the first rows.
- The saved message is missing from that page (deleted), or the page fails or is swept: the
  window is emptied and the driver loads the default window (read boundary or latest), and the
  first rows drop the entry (`ScrollOpen::Missing`).
- Threads do the same with their first `getMessageThreadHistory` page
  (`ThreadView::saved_anchor`, set by the UI before the page is requested). Forum topics have no
  around-load: their histories stay cached across visits, so a saved row is restored when it is
  still in that cache and dropped otherwise.
- Recording: the top row is saved on every render and every scroll (the scroller observer)
  while the reader is away from the tail, and forgotten at the tail. Sending a message and jump to
  latest forget the entry; replacing the open chat's window forgets it; an account switch clears
  the memory.
- Explicit targets win: "Go To Message", a found message from search and a message link drop the
  saved entry before the chat opens. Quill's notification clicks carry no message, so they open
  at the default position and restore, as tdesktop does for most notifications.
- Chats with no saved entry open exactly as before: around the read boundary with the unread
  divider, else at the latest message.
- Vendored `gpui-component` scroller: `top_anchor` and `scroll_to_item_offset` only (listed in
  its QUILL-CHANGES.md; re-apply when re-vendoring).

## Why the late restore was removed
The first version restored the position whenever the saved message arrived, including after the
chat was already on screen at its default position. Three review rounds each found another way
the reader could move in that gap and then be pulled back by a page landing later: the scrollbar
track click, search and reply-quote jumps, the screen-reader focus scroll, an idle wait that never
ended. Each fix added another counter (`note_user_scroll`, `is_scrollbar_dragging`) to end the
wait, and every input path the app gains later would need one too.

The wait existed because the chat opened somewhere the saved message might not be. tdesktop never
has that gap: it loads around the saved item before showing the chat. Quill now does the same, so
there is no "later": the restore happens on the first rows or not at all. `RestoreWait`, the
late-page restore branch in `history_message_list.rs`, the scroller's `user_scrolls` /
`note_user_scroll` / `is_scrollbar_dragging` and their calls in `shortcut_pack.rs` are gone.

The one hold left is the first anchoring waiting for the around page, a single request while the
history shows its loading skeleton. A live message landing in the empty window during that request
is shown, as it is today for a jump or an unread open, and the anchoring still waits for the page.

## Tests
- `state::scroll_memory`: saved entry opens around its message without a highlight, an explicit
  target wins and forgets, a missing anchor falls back and forgets, LRU and key rules.
- `connect::tests::history_window`: the saved open sends one `getChatHistory` around the message
  and no highlight; a missing message or a failed load falls back to the default window; a cached
  window holding the message is kept; an explicit target highlights.
- `connect::tests::threads`: a thread's first page loads around its saved reply and falls back to
  the read position when the reply is gone.
- `ui::scroll_restore_tests` (real app, scroller and render loop): two round trips land on the same
  row and offset; "Go To Message" wins over the saved row; a saved row that is gone opens the
  default and is forgotten.

## Verified
Demo `ready-chats` with `QUILL_DEMO_STRESS=3,300`: a 300 px wheel scroll in Demo chat A, then Demo
chat B, Demo chat A, twice. After both round trips the view equals the one left (stress messages 3
to 9 at the same offsets), light and dark.
Not shown: real TDLib round trips (the demo history is all in memory, so the restore there is the
in-window case; the around-load is covered by the driver tests).

## Known gap
Other default openers (notification clicks, the profile "Message" button, chat windows) do not go
through `QuillApp::chat_open_at`, so they open at the default position. The saved entry is then
applied only if its row happens to be in that window and is forgotten otherwise; when it is in the
window it can win over the unread divider. Harmless, and left for a follow-up that routes them
through `chat_open_at`.

## Fallback hardening (review round 4)
- A failed default load after a missing saved message sets `load_failed` (the usual Retry row)
  instead of leaving the skeleton up.
- A failed send of the around request returns the default load's result, so the chat list does not
  report "could not open chat" while the chat opens.
- The fallback runs right after the reducer applies a payload, before any `?` return in the ingest.
- A stale-view or chat-not-shown around page clears `saved_open` for its chat.
- Tests in `connect::tests::history_window`: chat switch during the request, a swept request falls
  back once, a live message in the empty window, an open with no history entry, and both error paths.
