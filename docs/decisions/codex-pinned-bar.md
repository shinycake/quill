# Pinned bar parity (tdesktop)

How tdesktop's pinned bar works (`history_view_pinned_bar.cpp`,
`HistoryWidget::checkPinnedBarState`):
- **Source.** The bar tracks all of the chat's pinned messages, not just
  loaded ones.
- **Title.** "Pinned message" for the newest, "Previous message" when
  there are two, otherwise "Pinned message #N".
- **Segments.** A segmented accent bar on the left, up to four segments
  with the current one lit.
- **Click.** Jumps to the message; the bar then moves to the next older
  pin.
- **Right button.** With one pin, ✕ unpins after "Would you like to unpin
  this message?", or for readers hides the bar until a new pin. With
  several, a list button opens the pinned messages ("N pinned messages",
  "Unpin all N messages" / "Don't show pinned messages").

Quill showed the newest pinned message among *loaded* rows only, with
"Unpin" / "Unpin all" text buttons. Now:
- **State.** The driver fetches the pinned list on chat open
  (`searchChatMessages` + `searchMessagesFilterPinned`,
  `RequestPurpose::GetPinnedMessages`). It refetches when a pin arrives
  for the open chat. Unpins and deletes drop rows at once, and a
  confirmed unpin-all empties the list. `Session::pinned_list` prefers
  loaded rows, which carry edits.
- **Bar.** tdesktop's titles and segments; a click jumps and steps the
  per-chat cursor. ✕ (one pin) or the list button (several) on the right,
  with tdesktop's confirmations through gpui-kit alert dialogs.
- **List.** A panel below the bar instead of a separate section: rows
  with day and preview that jump on click, and the unpin-all / hide
  footer.

Covered by a state test (ordering, unpin, delete). Verified live in Saved
Messages (six pins): segments, the jump moving the bar to "Pinned message
#5", and the list.
