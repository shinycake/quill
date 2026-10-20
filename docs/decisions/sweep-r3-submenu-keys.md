# Keyboard control of context-menu submenus

## What tdesktop does

`PopupMenu::popupSubmenu` sets `_activeSubmenu` whenever a submenu is shown,
by hover or by key (lib_ui/ui/widgets/popup_menu.cpp:540); `childHiding` and
`hideMenu` clear it (:688, :560). While it is set, `handleKeyPress`
(:571-590) hands every key to the submenu first, whichever row of the parent
has the selection: Down and Up enter and walk the submenu (its selection was
cleared on show, so Down takes its first item and Up its last,
`Menu::handleKeyPress`, menu.cpp:393), Left and Escape hide only the submenu
and the selection stays on the parent action. With no submenu open, Right on
an action that has one opens it (`setClicked(Keyboard)`).

## What changed

Only the chat-row menu has submenus in Quill (Mute notifications and Add to
folder, `chat_menu_submenus.rs`). The message menu has none.

- Rows that own a submenu register themselves with
  `ContextMenuItems::mark_submenu_parent` each frame, naming the
  `ChatMenuSubmenu` they open. Mute and Add to folder use one named order
  constant each, shared by the row's key and the mark.
- `ChatMenuState::submenu` is the one source of truth for "a submenu is
  open". `submenu_key_press` reads it: when set, Up and Down walk that
  submenu's rows (`step` with the parent key), Left and Escape close only it
  and focus the parent row, and Right or Enter on the parent row moves into
  its first row. This holds for a hover-opened submenu with the focus still on
  the parent row, and for a key-opened one.
- With no submenu open, Right or Enter on a parent row opens it and asks the
  end of the next render (`apply_pending_focus`) to focus its first row, since
  those rows only exist while it is open.
- Left stays "close" and Right stays "open" in right-to-left layouts.
  Mirroring them is an owner decision and is not done here.
- Quill difference: Enter on the parent row of an open submenu moves into it
  instead of doing nothing, since in Quill that row's click toggles it.

## How verified

- GPUI tests (`context_menu_submenu_tests`): Right and Enter on Mute focus the
  first submenu row; Left returns to the parent row with the menu open;
  Escape in the submenu closes only it and a second Escape closes the menu;
  Down walks the submenu rows in the order drawn (read from the accessibility
  tree), wraps to the first, and Up wraps back to the last without reaching the
  menu. With the submenu opened by the pointer and the focus on the parent
  row: Down enters at the first row, Up at the last, Left closes only the
  submenu, Escape closes only the submenu (a second closes the menu).
- `--screenshot-demo ready-chats`, right-click "Demo chat B", `k:down` to Mute
  notifications, `k:right`, and `m:` onto the Mute row then `k:down`, light
  and dark; PNGs read back, not committed.
- Not covered: the Add to folder submenu (the demo has no folders; it shares the
  code path with Mute), and no real-hardware run.
