# Chat list: Escape from a chat keeps the selection on its row

## What tdesktop does
`HistoryWidget::escape` -> `MainWidget::handleHistoryBack` closes the chat and
focuses the dialogs list; `Dialogs::InnerWidget` keeps `_selected` on the row of
the chat that was open, so the next Up/Down/Enter steps from it.

## Bug
After Escape from a chat that was opened with the mouse, the list drew Demo chat A
as selected but the keyboard selection (`ChatListKeys::list`) was `None`, so the
first Down did "nothing selected -> first row" (A again) and Enter opened A.

## Change
`escape_to_chat_list` now writes the closed chat's row into `ChatListKeys::list`
(`select_list_row`, which also scrolls it into view) before leaving the chat. The
highlight is drawn only from that field and the next key steps only from it, so
the list is a function of one piece of state. Archive and community Escape paths
only change the filter, so they leave no highlight and no hidden selection (the
two agree). Search results already use one field (`ChatListKeys::search`) for both
paint and stepping.

## Focus ring
The list-wide ring is the kit's `focus_ring_style` (3 px outward ring, `ring`
token at 50% alpha). It sits in the sidebar's 12 px padding, so it is not
clipped, and it follows the design guide ("preserve outward focus rings").
Kit lists do not ring their container, but nothing in the guides asks for a
different treatment, so it is unchanged.

## Verified
- GPUI test `escape_then_down_then_enter_from_a_mouse_opened_chat`; all existing
  chat_list_keys tests still pass.
- Captures (light and dark) of Escape and Escape+Down in
  /Volumes/Idan1TB/quill-work/scratch/sweep-chat-list-escape-selection/.
