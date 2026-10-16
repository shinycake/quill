# Folder tab strip fits any sidebar width

## What tdesktop does
`window/window_filters_menu.cpp` and lib_ui's `ScrollArea`-backed filter strip
keep the tabs in one row that scrolls horizontally (wheel or trackpad) when
they do not fit. The strip never overlaps the controls beside it.

## What changed
- `src/ui/folder_tabs.rs`: the kit `TabBar` is built without its own menu and
  suffix. A row lays out the scrolling strip (`flex_1`, `min_w_0`,
  `overflow_hidden`) and then the folder-list chevron and the settings gear as
  `flex_none` siblings, so nothing can overlap a tab or clip it under a button.
- An edge fade (sidebar colour gradient, 28 px) shows on each side that hides
  tabs, so a label is never cut mid-glyph without a cue. The strip scrolls by
  wheel/trackpad through the kit `TabBar` scroll container.
- The selected tab is scrolled into view whenever the selection, the tab count
  or the sidebar width changes (`RevealKey`), not on every render, so manual
  scrolling is kept.
- The chevron menu is the same kit `PopupMenuItem` list the kit's `menu(true)`
  built (checked item, scrollable), selecting through `select_folder_slot`.
- Pure helpers (`fade_edges`, `needs_reveal`) live in
  `src/ui/folder_tabs_fit.rs` with unit tests. Unread badges are untouched
  (`flex_none` pills inside the tab).
- The segmented Chats/Contacts/Calls row (`header_fit.rs`) is unchanged.

## Verified
Demo captures (`ready-chats`, temporary env hack for sidebar width and extra
folders, not committed) at 240 px light and dark, 240 px with 6 folders (fade
on the right), the same scrolled to the end (fade on the left), and 400 px.
Not verified: real trackpad inertia, Linux/Windows rendering, live TDLib folders.
