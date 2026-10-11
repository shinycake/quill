# Go To Message

## What Telegram Desktop does

`AddGoToMessageAction` (`history/view/history_view_context_menu.cpp`) adds
"Go to message" only when the list's element context is `Pinned` or
`ChatPreview`, and only for regular (server) messages. It calls
`controller->showMessage(item)`. Search results, shared media, Saved Messages
sublists, scheduled and thread lists have no such item: a click on the row
opens the message. (In the pinned context tdesktop also requires an
out-layout view; Quill's pinned list has no layout split, so every pinned row
qualifies.)

## What changed

- `src/go_to_message.rs`: pure rules. `go_to_message_offered(context, id,
  pending)` is true for Pinned and ChatPreview with a server message;
  `go_to_target` says whether the chat must be opened first.
- Pinned list rows already had a "Go To Message" item; it now goes through the
  shared rule.
- The chat-list peek preview rows gain a right-click "Go To Message" that
  closes the preview, opens the chat if needed and jumps with the existing
  highlight path.
- Nothing is added to search, Saved sublists, shared media or threads, since
  tdesktop does not offer it there.

## Opening at the message

From the preview the chat opens directly at the message: `ConnectDriver::select_chat_at(chat, Some(message))`
skips the newest-page load and runs the existing jump pipeline
(`getChatHistory` around the message, then highlight). The UI passes the
target through `chat_list.open_at`, taken by `select_chat_with`. A target in
the already open chat is a plain jump.

## Verified

- Unit tests for the contexts and the jump target.
- Driver test: `select_chat_at` sends exactly one history request, around the
  message, and the jump becomes Ready (highlight) when it arrives.
- GPUI UI test (`--features demo-capture`, `message_actions/go_to_tests.rs`):
  right-click on a preview row opens the menu, and clicking its item is not
  eaten by the overlay's mouse-up close; the chat opens and the preview closes.
- Gate. Not run against live TDLib.
