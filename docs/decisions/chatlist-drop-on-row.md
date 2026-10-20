# Files dropped on a chat row

## What Telegram Desktop does

`dialogs_widget.cpp` `Widget::dragEnterEvent/dragMoveEvent/dragLeaveEvent/dropEvent`
(lines 3784-3875):

- A drag is accepted when `ComputeMimeDataState` is not `None` (something that
  can be sent).
- While it moves over the list, the row under the pointer is selected
  (`updateFromParentDrag`, `dialogs_inner_widget.cpp:4522`), so it lights up.
  Every move restarts `_chooseByDragTimer`; a pointer that rests for
  `ChoosePeerByDragTimeout` (1000 ms, `config.h:32`) calls `chooseRow`, which
  opens that chat while the drag goes on.
- Leaving the list clears the selection (`dragLeft`).
- A drop on a row calls `filesOrForwardDrop(thread, data)`: the chat opens and
  the files go to its send box (a forum asks for a topic first). Dragged
  messages (`application/x-td-forward`) forward instead.

## What changed

- `src/ui/chat_list/drop_on_row.rs`: every chat row is wrapped
  (`drop_row_wrap`) with a drag-move and a drop handler for GPUI's
  `ExternalPaths`, the same drag type the conversation's drop zones use, so
  it behaves alike on macOS, Windows and Linux.
  - The row under a live file drag gets an accent outline and tint (state:
    `DropOnRow.hover`, valid only while a drag is active, so the paint is a
    function of state). Only one row is lit; leaving the row or the list, or
    the drag ending, clears it.
  - A rest timer ticks every 100 ms; ten ticks without a move over the same
    row open its chat (`select_listed_chat`). A move over the row resets it.
    Counting ticks keeps it deterministic under the test clock.
  - A drag that holds nothing sendable (`drop_modes::classify` is `None`, e.g.
    a folder together with files) is not accepted: no light, no drop.
  - A drop opens the chat and attaches the files through
    `drop_files_plain`, the plan a drop on the conversation uses (media or
    documents by what was dropped). If the chat shows in a window of its own,
    nothing is attached here.
  - Selection mode (checked rows) ignores the drag.
- State is one field on `ChatListUi` (`drop_on_row`).
- Screenshot demo `ready-drop-on-row` shows the lit row.

## Deviations and follow-ups

- tdesktop freezes the list order while a drag hovers it; Quill does not.
- Forums: tdesktop asks for a topic; Quill opens the forum and attaches to
  the open topic state.
- A chat the person cannot send to opens but attaches nothing, with no note.
- Dragging messages from a conversation onto a row (tdesktop forward drag):
  Quill has no message drag source today. Follow-up.
- Dropping onto search results is not handled.

## How verified

- Unit test for the rest timing; GPUI tests dispatch `FileDropEvent`s at the
  list: one row lights up and the light moves and clears; a drop opens the chat
  and attaches the file; a folder-plus-file drag is refused; a one-second rest
  opens the chat; a move restarts the wait.
- Light and dark captures of `ready-drop-on-row` were viewed.
- Not run with a real OS drag on any platform.
