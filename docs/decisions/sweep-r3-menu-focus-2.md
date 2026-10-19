# Keyboard reach for the message and chat-list menus

## What tdesktop does

`Ui::Menu::handleKeyPress` (lib_ui/ui/widgets/menu/menu.cpp, 381-420) moves
the selection with Up and Down, wraps at both ends and skips disabled
actions. Enter, Return and Space run the selected action, Escape hides the
popup, Left/Right open and close submenus (popup_menu.cpp). Every context
menu in Telegram Desktop is one `Ui::Menu`, so the message menu and the chat
row menu behave like the small ones.

## What changed

PR #1018 made the archive, story, playlist and folder tab menus keyboard
driven. The message menu (`message_menu_overlay.rs` and the extra rows in
`message_menu_ui/`) and the chat row menu (`chat_menu_submenus.rs`,
`top_person_menu.rs`) drew hand-built rows with no focus handle. Now:

- `MenuKeyTarget::menu_key_target(key, items, cx)` in `context_menu_keys.rs`
  makes any hand-built row a menu item: it claims a focus handle from
  `ContextMenuItems` and shows the shared focus ring while keyboard focused.
  Up, Down and Tab reach it, Enter and Space run its `on_click`, Escape
  closes the menu and gives the focus back (all through the existing
  interceptor and `context_menu_open()`, which already lists both menus).
- The message menu builds its rows in one order and sorts them by `order`
  afterwards, so handles are claimed in build order, not drawing order.
  Each claim now carries a sort key (`row_key(order)`, or `submenu_key` for
  a row of an open submenu, placed right after its parent) and Up/Down walk
  the handles by key. Rows with the same key keep build order.
- The Mute and Add to folder submenus build their rows only while open, so a
  closed submenu adds no hidden tab stops. Enter on the parent row opens it
  and the next Down enters its first row.
- Visuals are unchanged apart from the focus ring.

Left for later: Left/Right to open and close a submenu from the keyboard
(needs the submenu to take the key before the chat list does), and the
reaction strip above the message menu is not a tab stop yet.

## How verified

- GPUI tests (`context_menu_focus_tests`): Down and Up walk the rows of the
  chat menu and of the message menu in the order they are painted (read from
  the accessibility tree, so the expected sequence and row count do not come
  from the sort keys); breaking one row's key makes the test fail. Tab wraps
  inside the menu and never reaches the chat list, Enter and Space run the
  focused row (tested on the chat and message menus; the other menus use the
  same helper, #1018), a menu with no items or only disabled items keeps the
  focus on its frame (that covers stepping only, not Enter), and a submenu's keys stay between its parent and the next row.
  The arrows leave the chat list selection alone; Escape closes both menus
  and restores focus.
- `submenu_key` clamps the index to 254 so a long submenu cannot spill into
  the next row's keys; no RefCell guard is held across a GPUI call.
- `--screenshot-demo ready-chats` with a right-click on a message and on a
  chat row, then `k:down` and `k:down;k:down;k:up`, in light and dark; PNGs
  read back, not committed.
