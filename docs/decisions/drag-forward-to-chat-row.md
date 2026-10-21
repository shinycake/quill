# Drag messages onto a chat row to forward them

README: `parity:chatlist-drop-on-row`. The file half lives in
`src/ui/chat_list/drop_on_row.rs`; this adds dragged messages.

## What Telegram Desktop does

- `ListWidget::prepareDrag` (`history_view_list_widget.cpp`): a press that
  starts a drag from a selected message carries the whole selection
  (`uponSelected`), when every selected message `canForward`. A press on a
  message's date, or on its media, carries that one message. A chat with copy
  restrictions never starts a drag. The drag starts after
  `QApplication::startDragDistance` (10 px Manhattan).
- `Dialogs::Widget::dragEnterEvent` accepts an `application/x-td-forward` drag
  only outside one-column mode, with no choose-by-drag timer. Entering the
  list shows the `lng_forward_choose` hider (`showDragForwardInfo`); leaving
  it cancels (`dialogsCancelled`). The list is frozen while the drag hovers it.
- `MainWidget::filesOrForwardDrop` / `setForwardDraft`: the drop opens the
  thread with the messages as its forward draft (the forward bar above the
  composer, the same state the Forward box leaves for one chat). A send error
  (`GetErrorForSending`) toasts instead. A forum asks for a topic first.

## What changed

- `quill::forward_drag` holds the rules with no UI types: `ForwardDrag::begin`
  (selection or the lone message; a protected chat, a message that cannot be
  forwarded or is not sent yet refuse), `Destination::route` (forward / choose
  topic / cannot), `list_accepts` (not in one column). Unit tested.
- Sources: a GPUI `on_drag` with the typed `ForwardDrag` payload on the
  selection overlay of a selected row and on a message's time in its bubble
  footer (`ui/forward_drag_source.rs`). The ghost is a small pill with the
  count. GPUI starts a drag after 2 px, tdesktop after 10: a release that
  moved under 10 px puts the picture back and counts as the click it was
  (`selection_release`). A press on a selected row no longer range-selects
  (tdesktop `PrepareDrag`); once any drag is active, row selection and swipe
  to reply stop.
- Target: the `DropOnRow` state gains the forward drag and whether the pointer
  is inside the list. The row highlight, the frozen order and `drop_hover_row`
  are shared with files; a forward drag never starts the one-second wait.
  `on_drop::<ForwardDrag>` on the row wrapper routes the drop
  (`drop_forward_on_row`): refusals for chats that take no forwards
  (`lng_forward_cant`), secret chats and slow mode (the existing
  `share_dest_refusal`), and forums.
- The "Forward to…" banner (`lng_forward_choose`) dims the conversation while
  the drag is inside the list. It derives from the state and the active drag,
  so it cannot outlive the drag. Never in one column.

## Deviations

- Media thumbnails (`dragItemByHandler`) do not start a drag yet; the time
  label and selected rows do.
- A drop on a forum (or direct messages) row shows a note instead of the topic
  chooser tdesktop opens, as the file drop already does.
- The drag picture is a count pill, not the rendered messages.

## Verified

Unit tests (`forward_drag`), GPUI UI tests (`chat_list/forward_drop_tests.rs`:
lift, light, banner, no timer, frozen order, drop routing, forum, protected,
one column), and demo captures `ready-drop-forward` in light and dark.
