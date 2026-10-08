# codex/chatlist-archive-stories — archive row, story rings, pinned drag

tdesktop refs are under `Telegram/SourceFiles/`.

## B8 — "Archived chats" row

- tdesktop: the archive is a fixed-on-top `Data::Folder` entry of the main list
  (`dialogs/dialogs_inner_widget.cpp:655-690`), painted as a normal row with the archive
  userpic (`data/data_folder.cpp:240-290`), the preview `ComposeFolderListEntryText`
  (`data/data_folder.cpp:38-108`: newest 8 chats by last-message date, unread names semibold,
  "and N more chats", the last name dropped when exactly one would remain) and a muted unread
  badge counting unread chats (`data/data_folder.cpp:385-399`).
- `archiveCollapsed` -> a 37px bar (`st::dialogsImportantBarHeight`, `dialogs/ui/dialogs_layout.cpp:1408-1450`);
  `archiveInMainMenu` -> row gone, entry in the main menu (`window/window_main_menu.cpp:508-526`).
  Both are persisted (here in `AppearancePrefs`, were an unpersisted session flag).
- Context menu order from `window/window_peer_menu.cpp:2034-2090`: Collapse/Expand (hidden when in
  the menu), Move to main menu / chat list, Mark all as read, Archive settings. "How does it work?"
  skipped.
- Archived chats no longer render inline under the main list (the old collapsible section header
  is gone). Clicking the row opens the archive = Quill's existing **Archived** category tab, which
  is kept: tdesktop has no tab, but Quill's category tabs mirror TGX and the tab is the opened-folder
  view. The row is hidden in folder tabs, Unread and search (tdesktop: `!_filterId`).
- Main menu: "Archived chats" appears when moved there, plus "Move archive to chat list" (kit
  dropdown entries have no right click, so the way back is an explicit item; the toast says so).
- Logic: `src/chatlist_archive.rs` (+ `Session::archive_row_summary`); UI: `src/ui/archive_row.rs`.

## B9 — story rings

- tdesktop paints rings in the row's userpic (`dialogs/dialogs_row.cpp:520-580, 755-840`): one
  arc per story, unread first, accent gradient 2px; read arcs muted 1px; avatar scaled by
  `1 - 2*skip/photo` (46px -> 40px); `PaintOutlineSegments` (`ui/effects/outline_segments.cpp`)
  layout (10 degree gaps, shrinking above ~32 stories). Only users and channels.
- Ported as `src/story_ring.rs` (geometry, tested) + `src/ui/story_ring.rs` (path strokes,
  gradient). Data: `story_tray` now keeps every chat with active stories (previously dropped
  non-main lists); `ordered_story_tray` still filters to the main list. Unread = story id >
  `max_read_story_id` (`chatActiveStories`, td_api.tl:6783).
- **Click behaviour:** tdesktop's chat list does *not* open stories from the row avatar (no handler
  in `dialogs_inner_widget.cpp`; stories open from the top strip). Row click opens the chat, as
  before. Quill's existing story tray above the list is unchanged.
- Gradient is accent-derived (lighter top-right to accent), not tdesktop's green/blue palette.

## B6 — reorder animation

- tdesktop animates **only** pinned drag (`_pinnedShiftAnimation`, `dialogs_inner_widget.cpp:2851-2989`).
  A chat moving up on a new message jumps; there is no `_rowsAnimation`. Not invented here.
- Quill had drop-only pinned reorder (no animation). Replaced with tdesktop's live drag
  (`src/pin_reorder.rs`): dragged row follows the pointer on top, swaps when the pointer crosses
  `max(h/2, neighbour - h/2)`, displaced rows start offset by the dragged height and slide home
  200ms sine-in-out (`st::stickersRowDuration`), release slides the row into place and saves via
  `setPinnedChats` (unchanged `set_pinned_chat_order`). Timestamp + `request_animation_tick(60)`
  gated by the inactive-window rule; state dropped when settled; only the sidebar slice redraws
  (`notify_sidebar`). Not ported: 30px start threshold (GPUI's drag start used), edge auto-scroll.
- Pinned drag in the main list is the unfiltered All view; archived pinned order in the Archived tab.

## Verification

- Unit: `story_ring`, `chatlist_archive`, `pin_reorder` modules; `tests/chatlist_archive_stories.rs`
  (reducer plumbing). `gate.sh` OK.
- Visual (light only; dark would need the user's real prefs dir): demo kinds `ready-archive-row`,
  `ready-archive-bar`, `ready-archive-menu`, `ready-pin-drag`.
