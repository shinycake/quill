# Chat actions (typing, recording, uploading) in the chat list and header

Telegram Desktop parity for `updateChatAction`.

## Decisions

- **All TDLib kinds parsed.** `ChatAction` now covers typing, recording and
  uploading of video / voice note / video note, photo, document, choosing
  sticker / location / contact and playing a game. Unlabelled kinds
  (watching animations) stay `Other` and clear the sender's action, like
  `Cancel`.
- **State.** `Chat.sender_actions` (one `SenderAction { sender, action, name }`
  per sender) replaces `typing_senders` / `choosing_sticker_senders`. The
  first name is resolved when the update is applied.
- **Expiry.** No local timer: TDLib sends `chatActionCancel` when an action
  expires (about 6 seconds without refresh), which already clears the entry.
- **Wording** follows `history_view_send_action.cpp` and `lang.strings`:
  typing wins over other actions; 1 typer is "typing" (private) or
  "Dana is typing" (groups); 2 are "Dana and Eli are typing"; 3 or more
  are "N people are typing". Other actions read "recording a voice
  message" or "Dana is recording a voice message". Choosing a location
  or contact reads as typing. The trailing ellipsis is gone, since the
  animated indicator replaces it.
- **Rendering.** Chat row and header status show an accent-colored line
  with an indicator before the text: three dots pulsing in sequence for
  typing, one pulsing dot for other actions (`ui/activity_indicator.rs`,
  `with_animation` + `Animation::repeat`). The element exists only while
  an action is active, so an idle list animates nothing.
- No new README parity item: the existing typing items already apply.
