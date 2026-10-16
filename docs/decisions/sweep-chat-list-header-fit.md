# Chat-list header fits at every sidebar width

tdesktop: the sidebar top row holds the menu button, an optional lock button,
the search field and the proxy button; the search field gives way, never the
buttons. Its bulk selection bar (`SelectedBar`) names what the next click does
(Pin or Unpin, Mute or Unmute, Archive or Unarchive).

Quill changes:

- The header row (`chat_list/header_fit.rs`) keeps the side buttons at their
  natural width and gives the tabs the remaining space. When the labelled
  Chats / Contacts / Calls strip cannot fit (decided per render from the
  sidebar width and the visible buttons) the kit `TabBar` switches to
  icon-only tabs with tooltips and accessibility labels.
- The select bar derives Pin/Unpin, Mute/Unmute and Archive/Unarchive from the
  same any-predicates the bulk handlers use (`SelectionFacts`), per render.
  "Read" only marks unread chats read (never toggles), so it stays "Read".
- "Delete" in the More menu uses the row menu's destructive colour. The kit
  `PopupMenuItem` has no danger style, so it is an element item.

Verified with demo captures at a 240 px sidebar (light and dark) with the
passcode lock and proxy shield visible, and with mixed and all-pinned
selections.
