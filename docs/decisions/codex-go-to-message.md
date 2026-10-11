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

## Verified

Unit tests for the contexts and the jump target; gate. The cross-chat jump
right after opening a chat has not been exercised against live TDLib: the
history window may be replaced by the initial load.
