# Private-chat sharing protection: requests, Accept / Reject, who protects

Parity id: `premium-noforwards-requests`.

## What Telegram Desktop does

- **Request in the history** (`history_item.cpp`, `messageActionNoForwardsRequest`;
  `history_view_no_forwards_request.cpp`; `history_view_element.cpp`). A request
  shows as a service card: "{from} would like to enable sharing in this chat,
  which includes:" (the name in bold), then three bullets (Forwarding messages,
  Saving photos and videos, Copying messages). An incoming request that has not
  expired gets a reply-markup row, Reject | Accept. An expired request shows the
  one-line "Sharing enable request has expired", unless a toggle already answered
  it (`actionTaken`, set when the next item is a toggle), in which case the card
  stays.
- **Accept / Reject** (`api_suggest_post.cpp`, `RespondToNoForwardsRequest`)
  send `messages.toggleNoForwards` with the request's message id. A second press
  while one is in flight does nothing. An error shows as a toast.
- **Toggle wording** (`prepareNoForwardsToggle`). The same old and new value
  reads "Sharing in this chat is still disabled". That is the rejection.
- **Enable Sharing** (`window_peer_menu.cpp`, `Filler::addToggleNoForwards`).
  When the other person protects the chat (`NoForwardsPeerEnabled`) and you did
  not disable sharing in the last day (`sharingRecentlyDisabledByMe`, recorded
  when your disable succeeds and cleared when the chat is unprotected), a box
  asks "You need **{name}'s** approval to enable sharing. Send a request?" with
  Send Request. Confirming sends the toggle and opens the chat.
- **Context-menu note** (`AddSelectRestrictionAction`). In a private chat it says
  "You disabled copying and forwarding in this chat." when you hold the
  protection (`NoForwardsMyEnabled`), otherwise names the other person.

## How Quill maps it to TDLib

- TDLib computes `messageChatHasProtectedContentDisableRequested.is_expired`
  itself, from the message date and the `has_protected_content_disable_request_duration`
  option, and sends `updateMessageContent` when a request runs out
  (`MessageContent.cpp`, `UserManager::on_noforwards_request_timeout`). Quill
  uses that flag and computes no expiry of its own.
- "Answered" is derived from the loaded history, as tdesktop does: the message
  right after the request is a toggle, or a toggle names the request through
  `request_message_id`. It is not stored.
- TDLib has no chat-level my/peer split. It does expose the split on each
  message: `messageProperties.has_protected_content_by_current_user` and
  `_by_other_user` read the user's `noforwards_my_enabled` and
  `noforwards_peer_enabled`. Enable Sharing therefore asks `getMessageProperties`
  for the chat's last message and decides from the answer. If nothing is there to
  ask about, or the check fails, it sends the toggle and lets TDLib decide. The
  message menu note uses the menu's own `messageProperties`, and in a private chat
  it waits for that answer instead of guessing.
- Enabling sharing in a private chat is no longer applied optimistically. It may
  turn into a request, so the chat stays protected until TDLib says otherwise.

## Shape

- `PrivateSharingState` (groups domain): `enable` holds `TurnOff(chat)` or
  `AskToRequest(chat)` once TDLib answered. It also holds when this run disabled
  sharing per chat. The request box is open exactly while `enable` is
  `AskToRequest`, so the dialog is a function of session state.
- `Session::sharing_request_view` gives the row its look (card, buttons) from the
  message and the history.
- `GroupsPurpose::AnswerSharingRequest { chat_id, message_id }` doubles as the
  in-flight guard.

## Verification

Driver tests (`connect/tests/private_sharing.rs`) use the recording sender to
cover the properties check, both answers, the recent-disable shortcut, a failed
check, Accept / Reject with the in-flight guard and the error path, and the
answered card. UI tests (`ui/sharing_request_tests.rs`) cover the buttons, the
press without a connection, the card after an answer, and the request box.
The `/verify` captures of `ready-sharing-request` (with and without
`QUILL_DEMO_SHARING=box`) were checked in light and dark.

Not verified against a live account: the server's exact message sequence after
Accept / Reject (whether it edits the request to expired). The card covers both
cases.
