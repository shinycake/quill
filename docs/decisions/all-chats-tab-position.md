# "All chats" among the folder tabs (`parity:premium-main-list-position`)

## What tdesktop does

- `data/data_chat_filters.cpp` keeps "All chats" (filter id 0) as one entry of
  the folder list, at the index the server gives it. The tab strip
  (`ui/widgets/chat_filters_tabs_strip.cpp`) and the folder column
  (`window/window_filters_menu.cpp`) draw that list in order.
- Both reorder by dragging a tab. With Premium (`reorderAll =
  isPremium()`) every tab moves, "All chats" included. Without it the
  reorder pins the first slot (`addPinnedInterval(0, 1)`), so "All chats"
  cannot be dragged and nothing can be dropped in front of it, and before
  saving a move it puts "All chats" back in front (`moveAllToFront`).
- The order is saved with `messages.updateDialogFiltersOrder`, and an order
  change from another device arrives as an update that replaces the list.
- Settings > Folders lists only the real folders; "All chats" is not a row
  there, and the list has no drag reorder. Its save path keeps "All chats"
  after the folder that preceded it.
- No Premium preview opens when a free account tries to drag "All chats":
  the tab is simply pinned. Locked folders past the folder limit are a
  separate feature (`parity:chatlist-folder-limits`).

## What changed in Quill

- `updateChatFolders.main_chat_list_position` is parsed and kept in
  `ChatListState::main_list_position`, so a move from another device lands
  on the next update.
- `quill::folder_order` holds the tab order (`FolderOrder`) and the drag
  rules above as pure functions: Premium moves anything; free accounts keep
  "All chats" first and get position 0 back on any save.
- `reorderChatFolders` sends the real `main_chat_list_position`; the driver
  applies the order and the position optimistically.
- The strip and the folder column put "All" at its position and accept drags
  (`ui/folder_tabs_drag.rs`): a dragged tab takes the slot of the tab it is
  dropped on, as `base::reorder` does. Kit `Tab` and the column items use
  GPUI's `on_drag` / `drag_over` / `on_drop`, the same pattern as the sticker
  manager rows. The ghost is the tab name in an accent chip; tdesktop slides
  the tab itself, which the kit `TabBar` has no API for.
- Ctrl+Shift+Up/Down walks the tabs in their real order.
- Settings > Folders' up/down buttons keep "All chats" at its index (Premium)
  or send 0 (free), instead of always 0.

## How it was verified

- Unit tests for the order rules, a state test for the parsed position, a
  request test, a driver test with the recording sender (send, optimistic
  apply, update from another device) and GPUI UI tests that drag tabs in the
  strip and the column on Premium and free accounts.
- `ready-folder-tabs` demo (`QUILL_DEMO_MAIN_LIST_POSITION`,
  `QUILL_DEMO_FOLDER_RAIL`) captured in light and dark.
