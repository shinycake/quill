# Mention / reaction jump buttons and the "N Unread Messages" bar

## tdesktop behavior

- `history/view/history_view_corner_buttons.cpp`: three round corner buttons
  stacked above each other at the bottom-right of the history: scroll-to-bottom
  (down arrow), unread mentions ("@"), unread reactions (heart). Each carries a
  22px count badge (`historyToDownBadgeSize`, `chat_helpers/chat_helpers.style`
  ~1177-1209). The mention and reaction buttons show whenever the chat has
  unread markers; the arrow appears after scrolling away from the bottom.
- Clicking "@" / heart jumps to the oldest unread mention / reaction and shows
  the message; the message is read when it is viewed, which lowers the counter.
  Right-click offers "Mark all as read" (`history_widget.cpp` mention/reaction
  jump flow).
- `lng_unread_bar#one` / `#other` (`Resources/langs/lang.strings` ~5320): the
  bar reads "{count} Unread Message(s)".

## What changed

- `searchChatMessages` with `searchMessagesFilterUnreadMention` /
  `searchMessagesFilterUnreadReaction` (from_message_id 0, limit 100). TDLib
  answers newest first, so `oldest_message_id` takes the smallest id of the page
  (`RequestPurpose::JumpToUnread`, `Session::unread_jump`). `ConnectDriver::ingest`
  takes it and runs the existing jump-to-message path
  (`jump_to_chat_search_message`: already loaded, or `getChatHistory` around).
- Reading: Quill already sends `viewMessages` for rows the history reports
  visible; TDLib then reads the mention/reaction and sends
  `updateChatUnreadMentionCount` / `updateChatUnreadReactionCount`, which the
  reducer already applies. The badge therefore decrements from the update, not
  from a local guess.
- Right-click on either button: "Mark all as read" sends `readAllChatMentions`
  / `readAllChatReactions` (`RequestPurpose::ReadAllUnreadMarkers`).
- UI: `src/ui/jump_buttons.rs` stacks the buttons bottom-right of the history
  container (kit `Button` with `AtSign` / `Heart` icons and a 22px accent
  badge). They show and hide instantly, with no fade, so nothing animates on
  the main window. The kit's centered scroll-to-bottom button is unchanged.
- Unread bar: `HistoryState::unread_at_open` captures the chat's unread count
  when it is opened with an unread anchor; the divider shows
  `unread_bar_text(count)` ("1 Unread Message" / "N Unread Messages").

## Verification

- Unit tests: `unread_bar_text` pluralisation, `oldest_message_id`, filter
  constructors (`src/ui/jump_buttons.rs`); driver tests with the recording
  sender (`src/connect/tests/history_window.rs`): filter and from_message_id of
  the search, in-flight dedupe, oldest hit becomes an around-load jump,
  read-all RPC shapes, `unread_at_open` surviving a counter decrease.
- Not verified: visual look in the running app and a live TDLib round trip;
  the badge decrement after the jump relies on TDLib's update as described
  above. Demo mode has no driver, so the buttons only render there.
- Not done: the fade of the buttons (150ms in tdesktop), "next after the
  current position" ordering (always oldest first), and paging past 100 unread
  markers (the oldest of the newest 100 is used).
