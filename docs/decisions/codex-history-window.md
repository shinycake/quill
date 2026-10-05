# codex/history-window — one contiguous history window, opened at the first unread

## Problems fixed
- Chats always opened at the newest message and only paged backwards. With 200 unread
  messages the reader landed below all of them.
- Until the UI reported visible rows, opening a chat viewed its newest message with
  `force_read`, which marked the whole chat read before anything was on screen.
- A search hit or reply jump outside the loaded messages inserted an old slice into the same
  `BTreeMap`. Global search spliced in the lone hit. Either way the timeline rendered as
  gapless while missing every message in between, and new messages then attached after the
  gap.

## Model
- `HistoryState` is one contiguous window. It gains `has_newer` (the window stops short of the
  chat's latest message), `unread_anchor` (the last read incoming message when the chat opened
  with unread), `window_epoch` (bumped when the window is replaced), `latest_seen` (a
  high-water mark that survives resets, for chats whose summary lacks `last_message`) and
  `newer_failed`.
- `ConnectDriver::select_chat` → `Session::prepare_history_window`: a window that covers the
  read boundary and reaches the tail is kept. Otherwise it is replaced, and the first page
  loads around the boundary (`getChatHistory` offset −25). The session only records the
  anchor, so demo sessions with no driver keep their injected history.
- `fetch_history_newer` pages forward (offset −49, limit 50) until the latest message is in.
  `jump_to_latest` replaces the window with the newest page.
- An incoming `updateNewMessage` while `has_newer` stays out of the window. An outgoing one
  means the user sent from the middle, so the window resets to the latest run.
- Jumps outside the window (`jump_to_chat_search_message`, global `select_search_message`)
  replace the window and load around the target. `select_search_message` no longer promotes
  the lone hit.
- Replacing the window marks its in-flight page requests stale
  (`Session::stale_history_requests`), so a late page can't land in the new window.
- `message_ids_to_view` no longer falls back to "the newest row" when the chat opened with
  unread messages: only rows the UI reports on screen are viewed.

## UI
- An "Unread messages" bar above the first unread incoming message.
- The scroller anchors once rows exist after an open or a window replacement: the jump
  highlight, else one row above the unread divider, else the bottom.
- Newer pages prefetch 8 rows before the window's end. If the list is following the tail when
  a newer page lands, it stays on the old last row instead of skipping the page.
- The kit jump-to-latest button shows the unread count and calls `jump_to_latest_messages`,
  which replaces a short window instead of only scrolling to its end.

## Tests
- `connect::tests::history_window` covers six cases: open around the boundary without marking
  read, newer paging to the end, live messages outside a short window, sending from the
  middle, jump-to-latest dropping stale pages, and opening a chat with no unreads at the
  latest page.
- Updated tests: the search tests (hits open in context, and a jump replaces the window) and
  the replay unread test (nothing is viewed until reported).
