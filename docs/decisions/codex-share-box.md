# Share box, forward bar, send as (B4)

## What Telegram Desktop does

- `boxes/share_box.cpp`: a checkable chat list with a search field (local
  dialogs, then global results), a comment field, and a Send button. Right
  click on Send opens the send menu (`menu/menu_send.cpp`): send without
  sound, schedule (Reminder wording when the only target is Saved Messages),
  plus forward options (show/hide sender names, captions). Enter ticks the
  first match, or sends when something is ticked. A "Copy share link" button
  appears when the item has a direct link (one message).
- `history_view_forward_panel.cpp`: after a single destination is chosen, the
  chat opens with a bar above the composer: "Forward", the original sender
  names (one name, "A and B", or "A and N others"; "Sender names removed" when
  hidden) and a preview (the text for one message, "N forwarded messages"
  otherwise). Its options box offers show/hide sender names, show/hide
  captions and "Change recipient". The text typed in the composer is the
  comment; Send forwards after it.
- `ui/chat/choose_send_as.cpp`: "Send message as..." lists the personal
  account, the group itself ("Anonymous admin") and channels; entries that need
  Premium are shown but locked with a Premium hint.

## What changed in Quill

- Requests: `forwardMessages` now takes `messageSendOptions`
  (`forward_messages_with_options`: silent, scheduled; `options: null` when
  default), `searchChatsOnServer`, `getChatAvailableMessageSenders`,
  `setChatMessageSender`.
- State: `chat.message_sender_id` / `updateChatMessageSender` ->
  `Session::chat_message_sender`; `chatMessageSenders` ->
  `send_as_options`; `ShareSearch` (local + server answers, stale answers
  dropped by request id); `share_destinations` merges local matches with the
  server results and drops channels where the user is known not to be able to
  post. Several forwards can be in flight (`queued_forward_flights`), so each
  destination resolves its own result.
- Driver: `forward_messages_with_options`, `search_share_chats`,
  `get_chat_available_message_senders`, `set_chat_message_sender` (only
  listed, non-premium senders).
- UI (`src/ui/share_box_ui.rs`): the old single-click picker is now the share
  box: tick circles, search with server search, comment, Hide sender names /
  Hide captions, Copy link (one message), Schedule (the existing schedule
  picker via `ScheduleTarget::Share`), Send without sound, Open chat (one
  destination) and Send. The comment goes first as a plain message to each
  destination, then the forward. "Open chat" shows the forward bar above that
  chat's composer; Send (and Enter) in the composer sends the typed comment
  and then the forward, using the composer's silent/schedule chips. Change
  recipient reopens the box with the chat ticked; Cancel there returns to the
  bar. The composer shows the current send-as avatar when TDLib reports a
  selectable sender; clicking it opens "Send message as..." (Esc closes).
- Pure logic in `quill::share_box` (selection, send modes, bar wording, done
  label) with unit tests.

## Differences from tdesktop

- Send options are separate buttons, not a right-click menu.
- The forward bar shows only while the destination chat is open; leaving it
  drops the draft (tdesktop keeps it as that chat's draft).
- Several destinations: each result updates the banner; the last one wins.
- Not done: the deep-link share chooser (`deep_link_routes.rs`) still lists
  recent chats without search; "Copy link" gating relies on the existing
  `getMessageProperties.can_get_link` check; no premium purchase flow.

## Verified

- Unit and driver tests: request shapes, send options, two simultaneous
  forwards, share search with stale answers, send-as load/set gating, bar
  wording.
- `--screenshot-demo ready-share-box`, `ready-forward-bar`, `ready-send-as`
  (English fixtures, dark theme) captured and checked.
- Not verified against a live account (no live sends).
