# Keyboard control of context-menu submenus

## What tdesktop does

`PopupMenu::handleKeyPress` (lib_ui/ui/widgets/popup_menu.cpp, 577-590)
adds to the menu's Up/Down/Enter handling (`menu.cpp`, 381-420): Right (or
Enter) on an action that has a submenu opens it and selects its first item;
Left (or Escape) inside a submenu hides only that submenu and the selection
goes back to the parent action. Up and Down move inside the submenu that has
the selection.

## What changed

Only the chat-row menu has submenus in Quill (Mute notifications and Add to
folder, `chat_menu_submenus.rs`). The message menu has none: its "Save to"
and audience rows are plain rows.

- Rows that own a submenu register themselves with
  `ContextMenuItems::mark_submenu_parent` each frame, naming the
  `ChatMenuSubmenu` they open. The open submenu is already state
  (`ChatMenuState::submenu`); nothing new is stored for it.
- `context_menu_key` (the interceptor hook from #1018/#1027) now handles
  Right and Enter on a registered parent row: it opens the submenu and asks
  the end of the next render (`apply_pending_focus`, one line in
  `app_render/mod.rs`) to focus the submenu's first row, because those rows
  only exist while it is open. Left and Escape on a submenu row close only the
  submenu and focus the parent row; Escape anywhere else still closes the menu.
- `step` (Up/Down) walks one group: inside a submenu, only that submenu's
  rows (wrapping); otherwise only the menu's own rows. So Down no longer
  steps into an open submenu from its parent (it did in #1027), as in tdesktop.
- `context_menu_item` keys are `row_key(n)`, so every non-submenu key has a
  zero low byte and a submenu key is recognised by a non-zero one.
- Left stays "close" and Right stays "open" in right-to-left layouts. Mirroring
  them is an owner decision and is not done here.

## How verified

- GPUI tests (`context_menu_submenu_tests`): Right and Enter on Mute focus the
  first submenu row; Left returns to the parent row with the menu open;
  Escape in the submenu closes only it and a second Escape closes the menu;
  Down walks the submenu rows in the order drawn (read from the accessibility
  tree), wraps to the first, and Up wraps back to the last without reaching the
  menu; with a submenu open, Down on the menu's own rows visits those only.
- `--screenshot-demo ready-chats`, right-click "Demo chat B", `k:down` to Mute
  notifications, `k:right`, light and dark; PNGs read back, not committed.
- Not covered: the Add to folder submenu (the demo has no folders; it shares the
  code path with Mute), and no real-hardware run.
