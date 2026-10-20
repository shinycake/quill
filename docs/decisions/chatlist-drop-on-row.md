# Files dropped on a chat row

## What Telegram Desktop does

`dialogs_widget.cpp` `Widget::dragEnterEvent/dragMoveEvent/dragLeaveEvent/dropEvent`
(lines 3784-3875):

- A drag is accepted when `ComputeMimeDataState` is not `None` (something that
  can be sent).
- While it moves over the list, the row under the pointer is selected
  (`updateFromParentDrag`, `dialogs_inner_widget.cpp:4522`), so it lights up,
  and the list order is frozen (`_shownList->freeze()`, 2 s `kFreezeTimeout`
  after the last update, unfrozen by `dragLeft`). Every move restarts
  `_chooseByDragTimer`; a pointer that rests for `ChoosePeerByDragTimeout`
  (1000 ms, `config.h:32`) calls `chooseRow`, which opens that chat while the
  drag goes on.
- Leaving the list clears the selection (`dragLeft`).
- A drop on a row calls `filesOrForwardDrop(thread, data)`
  (`mainwidget.cpp:686-735`): a forum or direct messages group shows a topic
  chooser (`ShowDropMediaBox`) and does not open; a chat that takes no files
  shows a toast and does not open; otherwise the chat opens and the files go to
  its send box. A single folder goes as one archive
  (`history_widget.cpp:7960-7975`). Dragged messages (`application/x-td-forward`)
  forward instead.

## What changed

- `src/ui/chat_list/drop_on_row.rs`: every chat row is wrapped
  (`drop_row_wrap`) with drag-move, file-drop-exit and drop handlers for GPUI's
  `ExternalPaths`, the same drag type the conversation's drop zones use, so
  it behaves alike on macOS, Windows and Linux.
  - The row under a live file drag gets an accent outline and tint (state:
    `DropOnRow.hover`, valid only while a drag is active, so the paint is a
    function of state). Only one row is lit. Leaving the row, the list or the
    window (`on_file_drop_exit`) clears it and the wait. A row half hidden
    under the list's header does not react: hits are checked against the list
    viewport too.
  - The wait is one cancellable 1 s task (`DropOnRow.rest`): a new row or a
    different pointer position replaces it. Platforms that resend the update
    while the pointer rests (macOS `draggingUpdated:`, Windows `DragOver`)
    do not restart it, because the position is compared.
  - A drag that holds nothing sendable (`drop_modes::classify` is `None`, e.g.
    a folder together with files) is not accepted: no light, no drop.
  - The list order is frozen while the drag hovers the list and for 2 s after
    its last update (`DropOnRow.frozen`, read by the list ordering in
    `toggle_chat_pin.rs` through `keep_frozen_order`; chats that arrive meanwhile
    go last). The drop only goes to the lit row.
  - A drop opens the chat and attaches the files through `drop_files_plain`, the
    plan a drop on the conversation uses (media or documents by what was
    dropped). One folder goes through `drop_on_zone(DropAction::Archive)`, the
    drop zones' archive path.
  - A chat that takes no files (`start_files_refusal`, the same check as
    "start with files") shows tdesktop's refusal and does not open.
  - A forum or direct messages group is not opened or attached to: Quill has no
    topic chooser yet, so a note says to open the forum and choose a topic.
  - Selection mode (checked rows) ignores the drag. If the chat shows in a window
    of its own, nothing is attached here.
- State is one field on `ChatListUi` (`drop_on_row`).
- Screenshot demo `ready-drop-on-row` shows the lit row.

## Deviations and follow-ups (the README item stays open)

- Dragging messages from a conversation onto a row (tdesktop forward drag,
  `x-td-forward`): Quill has no message drag source today. Follow-up, so no
  parity fragment is added; the README item covers both.
- A topic chooser for forums and direct messages groups (`ShowDropMediaBox`).
- Dropping onto search results is not handled.
- tdesktop's `ComputeMimeDataState` also refuses files over the size limit and
  non-local URLs; `classify` does not check size (as for the conversation drop).

## How verified

- GPUI tests dispatch `FileDropEvent`s at the list: one row lights up and the
  light moves and clears on exit; a drop opens the chat and attaches the file; a
  single folder is archived; a drop on a forum row or on a chat that takes no
  files opens and attaches nothing; a folder-plus-file drag is refused; one
  second of rest opens the chat; updates at the same point do not restart the
  wait but a real move does; leaving the window cancels the wait; the list keeps
  its order while hovered and moves after two seconds.
- Light and dark captures of `ready-drop-on-row` were viewed.
- Not run with a real OS drag on any platform.
