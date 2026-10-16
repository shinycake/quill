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

## Review round 2

- Icon-only tabs now carry `flex_1`, so the three icons spread evenly across
  the segmented bar instead of clustering at the left. Labelled tabs keep
  their natural width.
- The Delete row in the More menu lets the kit draw the icon
  (`PopupMenuItem::element(..).icon(..)`), so it lines up with Select
  unread. The element only carries the label and the danger colour. The kit
  element item has no accessible-label hook; the visible text is the label.
- The labelled-strip width is no longer a fixed 228. `labelled_tabs_width`
  estimates it from the translated labels (7 px per character plus 34 px per
  tab, which gives 228 for the English labels) and is passed into the pure
  `tabs_icon_only`. Longer language packs raise the breakpoint. The estimate
  is not a text measurement; a measured width would need the window text
  system on every render.
- Tests: `one_button_breakpoint_is_pinned` (icon-only at 240 and 287,
  labelled at 288), `longer_translations_push_the_breakpoint_up`,
  `english_labels_need_228`.
- Verified with demo captures (temporary local env hack, not committed): 240
  px light, 240 px with lock and proxy in dark, 292 px labelled, and the
  More menu in dark. Icons spread evenly and the Delete icon aligns.
