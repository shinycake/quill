# Keyboard reach for the archive, story and playlist context menus

## What tdesktop does

tdesktop popup menus (`Ui::PopupMenu`) are fully keyboard driven: the first
action is reachable with the arrow keys or Tab, Enter runs the selected
action and Escape closes the menu.

## What changed

- `archive_row`, `stories_menu_ui` and `playlist_manage`: each menu item has
  `tab_index(0)` and a `focus_visible` fill (same colour as hover), the same
  idiom the folder tab menu got in #1012. The kit's keyboard click turns
  Enter and Space on a focused item into the item's `on_click`.
- `app_render/mod.rs`: the playlist menu was missing from `menu_open`, so
  opening it never moved focus into the menu frame (Tab had nothing to
  start from). It is in the list now, with the other menus.
- Escape already closed all three through the Escape stack.

## How verified

`--screenshot-demo ready-mute-archive` with `r:` on the archive row, then
`k:tab` (first item gets the fill), `k:tab` again (second item), light and
dark; and `k:tab` then `k:enter` ran Collapse and closed the menu. GPUI tests
for each menu check Tab lands on an item inside the menu frame. No demo kind
shows a story tile or the playlist page, so those two menus are covered by
the tests and the shared idiom only. Down arrow does not move between items
(unchanged; menus use Tab).
